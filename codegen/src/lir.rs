//! Typed HIR-to-LIR lowering and the mandatory LIR verifier.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;

use subscript_compiler::hir;
use subscript_compiler::lir as l;
use subscript_compiler::{ClassId, Pos, Type};

use crate::lir_types::boundary_box_class;

mod address_taken;
mod builder;
mod call;
mod construct;
mod expr;
mod lambda;
mod liveness;
mod lowering;
mod place;
mod stmt;
mod unroll;
mod verify;
mod verify_dominance;
mod verify_instruction;
mod verify_terminator;

use self::liveness::{classify_local_storage, thread_suspension_live_ins};
use self::verify::verify_function;

/// A construct or inconsistent checked fact that cannot be represented in
/// LIR without guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError {
    /// Source position of the construct.
    pub pos: Pos,
    /// Exact reason lowering stopped.
    pub message: String,
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.message)
    }
}

impl Error for LowerError {}

/// One verifier finding with function/block/value context in its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyError {
    /// Human-readable, deterministic finding.
    pub message: String,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for VerifyError {}

fn boundary_class_is_embedded_header(module: &hir::Module, header: ClassId) -> bool {
    if !module
        .classes
        .get(header.0)
        .is_some_and(|class| class.is_value && class.is_boundary)
    {
        return false;
    }
    let nullable_header = Type::Nullable(Box::new(Type::Class(header)));
    let used_as_link = module.classes.iter().any(|class| {
        class.is_boundary && class.fields.iter().any(|field| field.ty == nullable_header)
    }) || module.foreign_fns.iter().any(|function| {
        function
            .params
            .iter()
            .any(|parameter| parameter.ty == nullable_header)
    });
    used_as_link
        && module.classes.iter().any(|class| {
            class.is_value
                && class.is_boundary
                && class
                    .fields
                    .first()
                    .is_some_and(|field| field.ty == Type::Class(header))
        })
}

/// Lowers one complete typed HIR module to ordered LIR.
///
/// # Errors
///
/// Returns the first construct whose checked semantics cannot be encoded by
/// the closed LIR form.
pub fn lower_module(module: &hir::Module) -> Result<l::Module, LowerError> {
    let mut lowered = Lowering::new(module)?.run()?;
    unroll::run(&mut lowered);
    for function in &mut lowered.functions {
        thread_suspension_live_ins(function)?;
        classify_local_storage(function);
    }
    if let Err(errors) = verify_module(&lowered) {
        return Err(LowerError {
            pos: lowered.functions.first().map_or_else(
                || Pos::new("<module>", 1, 1),
                |function| function.pos.clone(),
            ),
            message: format!(
                "produced invalid LIR:\n{}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        });
    }
    Ok(lowered)
}

/// Verifies every function in an LIR module and returns all findings.
///
/// The checks cover single definitions, dominance, terminator shape, address
/// invalidation, and instruction/edge/return operand types.
///
/// # Errors
///
/// Returns every verifier finding in deterministic function/block order.
pub fn verify_module(module: &l::Module) -> Result<(), Vec<VerifyError>> {
    let mut errors = Vec::new();
    verify_module_entries(module, &mut errors);
    for function in &module.functions {
        verify_function(module, function, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_module_entries(module: &l::Module, errors: &mut Vec<VerifyError>) {
    let function = |id: l::FunctionId| {
        module
            .functions
            .get(id.0 as usize)
            .filter(|function| function.id == id)
    };
    if let Some(entry) = module.entry {
        if function(entry).is_none() {
            errors.push(VerifyError {
                message: format!("module entry function {} is missing", entry.0),
            });
        }
    }
    let mut previous = None;
    let mut seen = BTreeSet::new();
    for root in &module.async_roots {
        if !seen.insert(*root) {
            errors.push(VerifyError {
                message: format!("module async root function {} occurs twice", root.0),
            });
        }
        if previous.is_some_and(|previous| previous >= *root) {
            errors.push(VerifyError {
                message: "module async roots are not in declaration order".to_string(),
            });
        }
        previous = Some(*root);
        match function(*root) {
            Some(function)
                if function.exported
                    && function.is_async
                    && function.parameters.is_empty()
                    && Some(function.id) != module.entry => {}
            Some(_) => errors.push(VerifyError {
                message: format!(
                    "module async root function {} is not an exported zero-parameter non-entry async function",
                    root.0
                ),
            }),
            None => errors.push(VerifyError {
                message: format!("module async root function {} is missing", root.0),
            }),
        }
    }
}

#[derive(Clone)]
struct FunctionRecord {
    id: l::FunctionId,
    method: Option<l::MethodId>,
}

#[derive(Clone)]
struct FunctionInput {
    name: String,
    exported: bool,
    is_generator: bool,
    is_async: bool,
    creation_traps: Vec<hir::TrapSite>,
    host_entry_traps: Option<Vec<hir::TrapSite>>,
    params: Vec<hir::Param>,
    ret: Type,
    body: Vec<hir::Stmt>,
    pos: Pos,
}

impl From<hir::Function> for FunctionInput {
    fn from(function: hir::Function) -> Self {
        let creation_traps = function.trap_sites();
        Self {
            name: function.name,
            exported: function.exported,
            is_generator: function.is_generator,
            is_async: function.is_async,
            creation_traps,
            host_entry_traps: None,
            params: function.params,
            ret: function.ret,
            body: function.body,
            pos: function.pos,
        }
    }
}

#[derive(Clone)]
struct CallParam {
    name: String,
    ty: Type,
    default: Option<hir::Expr>,
    pos: Pos,
}

struct StaticCallback {
    target: l::CallTargetKind,
    callable: Option<l::Operand>,
    ty: Type,
}

/// The arguments of one call, between the explicit arguments and the
/// parameter defaults. A construction runs the field initializers
/// between the two steps (compiler.md §57.1).
struct PendingArguments {
    /// The coerced value of every parameter that is already lowered, in
    /// parameter order. A default reads it for the substitutions.
    values: Vec<l::Operand>,
    /// The operands each parameter contributes to the call.
    groups: Vec<Vec<l::Operand>>,
    /// The foreign array parameters, which contribute their pointer and
    /// their length after every argument is lowered.
    delayed_array_snapshots: Vec<(usize, l::Operand, Type, Pos)>,
}

impl From<&hir::Param> for CallParam {
    fn from(parameter: &hir::Param) -> Self {
        Self {
            name: parameter.name.clone(),
            ty: parameter.ty.clone(),
            default: parameter.default.clone(),
            pos: parameter.pos.clone(),
        }
    }
}

struct Lowering<'a> {
    hir: &'a hir::Module,
    free_functions: HashMap<String, FunctionRecord>,
    methods: HashMap<(usize, String), FunctionRecord>,
    foreign_functions: HashMap<String, l::ForeignFunctionId>,
    globals: HashMap<String, l::GlobalId>,
    fields: HashMap<(usize, String), l::FieldId>,
    functions: Vec<Option<l::Function>>,
    next_function: u32,
    classes: Vec<l::Class>,
    foreign: Vec<l::ForeignFunction>,
}

fn intrinsic_operations() -> Vec<l::IntrinsicOperation> {
    fn append<T: fmt::Debug>(
        table: &mut Vec<l::IntrinsicOperation>,
        family: l::IntrinsicFamily,
        values: &[T],
    ) {
        table.extend(
            values
                .iter()
                .enumerate()
                .map(|(operation, value)| l::IntrinsicOperation {
                    family,
                    operation: operation as u16,
                    semantic_name: format!("{value:?}"),
                    runtime_symbol: intrinsic_runtime_symbol(family, &format!("{value:?}"))
                        .map(str::to_string),
                    signatures: Vec::new(),
                }),
        );
    }

    let mut table = Vec::new();
    append(
        &mut table,
        l::IntrinsicFamily::Ambient,
        &hir::AmbientFn::ALL,
    );
    append(
        &mut table,
        l::IntrinsicFamily::ContextBytes,
        &hir::ContextBytesFn::ALL,
    );
    append(&mut table, l::IntrinsicFamily::Math, &hir::MathFn::ALL);
    append(&mut table, l::IntrinsicFamily::Number, &hir::NumFn::ALL);
    append(&mut table, l::IntrinsicFamily::Date, &hir::DateFn::ALL);
    append(&mut table, l::IntrinsicFamily::Json, &hir::JsonFn::ALL);
    append(&mut table, l::IntrinsicFamily::String, &hir::StrFn::ALL);
    append(&mut table, l::IntrinsicFamily::Regex, &hir::RegexFn::ALL);
    append(&mut table, l::IntrinsicFamily::Array, &hir::ArrFn::ALL);
    append(&mut table, l::IntrinsicFamily::Map, &hir::MapFn::ALL);
    append(&mut table, l::IntrinsicFamily::Set, &hir::SetFn::ALL);
    table.extend(
        hir::WorkerFn::ALL
            .iter()
            .enumerate()
            .map(|(operation, value)| {
                let semantic_name = format!("{value:?}");
                l::IntrinsicOperation {
                    family: l::IntrinsicFamily::Worker,
                    operation: operation as u16,
                    semantic_name: semantic_name
                        .split_once('(')
                        .map_or(semantic_name.as_str(), |(name, _)| name)
                        .to_string(),
                    runtime_symbol: intrinsic_runtime_symbol(
                        l::IntrinsicFamily::Worker,
                        semantic_name
                            .split_once('(')
                            .map_or(semantic_name.as_str(), |(name, _)| name),
                    )
                    .map(str::to_string),
                    signatures: Vec::new(),
                }
            }),
    );
    table
}

fn intrinsic_runtime_symbol(family: l::IntrinsicFamily, name: &str) -> Option<&'static str> {
    Some(match (family, name) {
        (l::IntrinsicFamily::Ambient, "Print") => "subscript_rt_print",
        (l::IntrinsicFamily::Ambient, "Collect") => "subscript_rt_collect",
        (l::IntrinsicFamily::Ambient, "UnsafeDelete") => "subscript_rt_delete",
        (l::IntrinsicFamily::Math, "Abs") => "subscript_rt_math_abs",
        (l::IntrinsicFamily::Math, "Acos") => "subscript_rt_math_acos",
        (l::IntrinsicFamily::Math, "Acosh") => "subscript_rt_math_acosh",
        (l::IntrinsicFamily::Math, "Asin") => "subscript_rt_math_asin",
        (l::IntrinsicFamily::Math, "Asinh") => "subscript_rt_math_asinh",
        (l::IntrinsicFamily::Math, "Atan") => "subscript_rt_math_atan",
        (l::IntrinsicFamily::Math, "Atanh") => "subscript_rt_math_atanh",
        (l::IntrinsicFamily::Math, "Cbrt") => "subscript_rt_math_cbrt",
        (l::IntrinsicFamily::Math, "Ceil") => "subscript_rt_math_ceil",
        (l::IntrinsicFamily::Math, "Cos") => "subscript_rt_math_cos",
        (l::IntrinsicFamily::Math, "Cosh") => "subscript_rt_math_cosh",
        (l::IntrinsicFamily::Math, "Exp") => "subscript_rt_math_exp",
        (l::IntrinsicFamily::Math, "Expm1") => "subscript_rt_math_expm1",
        (l::IntrinsicFamily::Math, "Floor") => "subscript_rt_math_floor",
        (l::IntrinsicFamily::Math, "Log") => "subscript_rt_math_log",
        (l::IntrinsicFamily::Math, "Log1p") => "subscript_rt_math_log1p",
        (l::IntrinsicFamily::Math, "Log10") => "subscript_rt_math_log10",
        (l::IntrinsicFamily::Math, "Log2") => "subscript_rt_math_log2",
        (l::IntrinsicFamily::Math, "Round") => "subscript_rt_math_round",
        (l::IntrinsicFamily::Math, "Sign") => "subscript_rt_math_sign",
        (l::IntrinsicFamily::Math, "Sin") => "subscript_rt_math_sin",
        (l::IntrinsicFamily::Math, "Sinh") => "subscript_rt_math_sinh",
        (l::IntrinsicFamily::Math, "Sqrt") => "subscript_rt_math_sqrt",
        (l::IntrinsicFamily::Math, "Tan") => "subscript_rt_math_tan",
        (l::IntrinsicFamily::Math, "Tanh") => "subscript_rt_math_tanh",
        (l::IntrinsicFamily::Math, "Trunc") => "subscript_rt_math_trunc",
        (l::IntrinsicFamily::Math, "Atan2") => "subscript_rt_math_atan2",
        (l::IntrinsicFamily::Math, "Hypot") => "subscript_rt_math_hypot",
        (l::IntrinsicFamily::Math, "Pow") => "subscript_rt_math_pow",
        (l::IntrinsicFamily::Math, "Max") => "subscript_rt_math_max",
        (l::IntrinsicFamily::Math, "Min") => "subscript_rt_math_min",
        (l::IntrinsicFamily::Math, "Random") => "subscript_rt_math_random",
        (l::IntrinsicFamily::Math, "Clz32") => "subscript_rt_math_clz32",
        (l::IntrinsicFamily::Math, "Imul") => "subscript_rt_math_imul",
        (l::IntrinsicFamily::Math, "Fround") => "subscript_rt_math_fround",
        (l::IntrinsicFamily::Math, "F32ToBits") => "subscript_rt_math_f32_to_bits",
        (l::IntrinsicFamily::Math, "F32FromBits") => "subscript_rt_math_f32_from_bits",
        (l::IntrinsicFamily::Number, "IsNaN") => "subscript_rt_num_is_nan",
        (l::IntrinsicFamily::Number, "IsFinite") => "subscript_rt_num_is_finite",
        (l::IntrinsicFamily::Number, "IsInteger") => "subscript_rt_num_is_integer",
        (l::IntrinsicFamily::Number, "IsSafeInteger") => "subscript_rt_num_is_safe_integer",
        (l::IntrinsicFamily::Number, "ParseInt") => "subscript_rt_num_parse_int",
        (l::IntrinsicFamily::Number, "ParseFloat") => "subscript_rt_num_parse_float",
        (l::IntrinsicFamily::Number, "ToFixed") => "subscript_rt_num_to_fixed",
        (l::IntrinsicFamily::Number, "ToStringF32") => "subscript_rt_num_to_string_f32",
        (l::IntrinsicFamily::Number, "ToStringF64") => "subscript_rt_num_to_string_f64",
        (l::IntrinsicFamily::Number, "ToExponential") => "subscript_rt_num_to_exponential",
        (l::IntrinsicFamily::Number, "ToPrecision") => "subscript_rt_num_to_precision",
        (l::IntrinsicFamily::Json, "Begin") => "subscript_rt_json_begin",
        (l::IntrinsicFamily::Json, "BeginTracked") => "subscript_rt_json_begin_tracked",
        (l::IntrinsicFamily::Json, "Finish") => "subscript_rt_json_finish",
        (l::IntrinsicFamily::Json, "Raw") => "subscript_rt_json_raw",
        (l::IntrinsicFamily::Json, "Str") => "subscript_rt_json_str",
        (l::IntrinsicFamily::Json, "I32") => "subscript_rt_json_i32",
        (l::IntrinsicFamily::Json, "U32") => "subscript_rt_json_u32",
        (l::IntrinsicFamily::Json, "I64") => "subscript_rt_json_i64",
        (l::IntrinsicFamily::Json, "U64") => "subscript_rt_json_u64",
        (l::IntrinsicFamily::Json, "F32") => "subscript_rt_json_f32",
        (l::IntrinsicFamily::Json, "F64") => "subscript_rt_json_f64",
        (l::IntrinsicFamily::Json, "Bool") => "subscript_rt_json_bool",
        (l::IntrinsicFamily::Json, "Date") => "subscript_rt_json_date",
        (l::IntrinsicFamily::Json, "Null") => "subscript_rt_json_null",
        (l::IntrinsicFamily::Json, "Visit") => "subscript_rt_json_visit",
        (l::IntrinsicFamily::Json, "Leave") => "subscript_rt_json_leave",
        (l::IntrinsicFamily::Json, "ParseBegin") => "subscript_rt_json_parse_begin",
        (l::IntrinsicFamily::Json, "ParseEnd") => "subscript_rt_json_parse_end",
        (l::IntrinsicFamily::Json, "ParseRoot") => "subscript_rt_json_parse_root",
        (l::IntrinsicFamily::Json, "ParseIsKind") => "subscript_rt_json_parse_is_kind",
        (l::IntrinsicFamily::Json, "ParseNumberFits") => "subscript_rt_json_parse_number_fits",
        (l::IntrinsicFamily::Json, "ParseNumber") => "subscript_rt_json_parse_number",
        (l::IntrinsicFamily::Json, "ParseInteger") => "subscript_rt_json_parse_integer",
        (l::IntrinsicFamily::Json, "ParseBool") => "subscript_rt_json_parse_bool",
        (l::IntrinsicFamily::Json, "ParseString") => "subscript_rt_json_parse_string",
        (l::IntrinsicFamily::Json, "ParseArrayLen") => "subscript_rt_json_parse_array_len",
        (l::IntrinsicFamily::Json, "ParseArrayGet") => "subscript_rt_json_parse_array_get",
        (l::IntrinsicFamily::Json, "ParseObjectGet") => "subscript_rt_json_parse_object_get",
        (l::IntrinsicFamily::String, "Slice") => "subscript_rt_str_slice",
        (l::IntrinsicFamily::String, "IndexOf") => "subscript_rt_str_index_of",
        (l::IntrinsicFamily::String, "LastIndexOf") => "subscript_rt_str_last_index_of",
        (l::IntrinsicFamily::String, "Includes") => "subscript_rt_str_includes",
        (l::IntrinsicFamily::String, "StartsWith") => "subscript_rt_str_starts_with",
        (l::IntrinsicFamily::String, "EndsWith") => "subscript_rt_str_ends_with",
        (l::IntrinsicFamily::String, "CharCodeAt") => "subscript_rt_str_char_code_at",
        (l::IntrinsicFamily::String, "Split") => "subscript_rt_str_split",
        (l::IntrinsicFamily::String, "Trim") => "subscript_rt_str_trim",
        (l::IntrinsicFamily::String, "TrimStart") => "subscript_rt_str_trim_start",
        (l::IntrinsicFamily::String, "TrimEnd") => "subscript_rt_str_trim_end",
        (l::IntrinsicFamily::String, "Repeat") => "subscript_rt_str_repeat",
        (l::IntrinsicFamily::String, "PadStart") => "subscript_rt_str_pad_start",
        (l::IntrinsicFamily::String, "PadEnd") => "subscript_rt_str_pad_end",
        (l::IntrinsicFamily::String, "ToUpperCase") => "subscript_rt_str_to_upper",
        (l::IntrinsicFamily::String, "ToLowerCase") => "subscript_rt_str_to_lower",
        (l::IntrinsicFamily::String, "Replace") => "subscript_rt_str_replace",
        (l::IntrinsicFamily::String, "ReplaceAll") => "subscript_rt_str_replace_all",
        (l::IntrinsicFamily::String, "Substring") => "subscript_rt_str_substring",
        (l::IntrinsicFamily::String, "Substr") => "subscript_rt_str_substr",
        (l::IntrinsicFamily::String, "CharAt") => "subscript_rt_str_char_at",
        (l::IntrinsicFamily::String, "CodePointAt") => "subscript_rt_str_code_point_at",
        (l::IntrinsicFamily::String, "Concat") => "subscript_rt_str_concat",
        (l::IntrinsicFamily::Regex, "New") => "subscript_rt_regex_new",
        (l::IntrinsicFamily::Regex, "Test") => "subscript_rt_regex_test",
        (l::IntrinsicFamily::Regex, "Source") => "subscript_rt_regex_source",
        (l::IntrinsicFamily::Regex, "Flags") => "subscript_rt_regex_flags",
        (l::IntrinsicFamily::Regex, "Search") => "subscript_rt_regex_search",
        (l::IntrinsicFamily::Regex, "Replace") => "subscript_rt_regex_replace",
        (l::IntrinsicFamily::Regex, "ReplaceAll") => "subscript_rt_regex_replace_all",
        (l::IntrinsicFamily::Regex, "Split") => "subscript_rt_regex_split",
        (l::IntrinsicFamily::Regex, "MatchStart") => "subscript_rt_regex_match_start",
        (l::IntrinsicFamily::Regex, "MatchEnd") => "subscript_rt_regex_match_end",
        (l::IntrinsicFamily::Set, "Union") => "subscript_rt_set_union",
        (l::IntrinsicFamily::Set, "Intersection") => "subscript_rt_set_intersection",
        (l::IntrinsicFamily::Set, "Difference") => "subscript_rt_set_difference",
        (l::IntrinsicFamily::Set, "SymmetricDifference") => "subscript_rt_set_symmetric_difference",
        (l::IntrinsicFamily::Set, "IsSubsetOf") => "subscript_rt_set_is_subset_of",
        (l::IntrinsicFamily::Set, "IsSupersetOf") => "subscript_rt_set_is_superset_of",
        (l::IntrinsicFamily::Set, "IsDisjointFrom") => "subscript_rt_set_is_disjoint_from",
        (l::IntrinsicFamily::Worker, "Post") => "subscript_rt_worker_post",
        (l::IntrinsicFamily::Worker, "Poll") => "subscript_rt_worker_poll",
        (l::IntrinsicFamily::Worker, "Close") => "subscript_rt_worker_close",
        (l::IntrinsicFamily::Worker, "Join") => "subscript_rt_worker_join",
        (l::IntrinsicFamily::Worker, "InboxWait") => "subscript_rt_worker_inbox_wait",
        (l::IntrinsicFamily::Worker, "InboxPoll") => "subscript_rt_worker_inbox_poll",
        (l::IntrinsicFamily::Worker, "OutboxPost") => "subscript_rt_worker_outbox_post",
        _ => return None,
    })
}

fn lower_operation_signature(signature: &hir::OperationSignature) -> l::CallSignature {
    let intrinsic = |family, operation, type_argument, worker_entry| {
        l::CallSignatureTarget::Intrinsic(l::Intrinsic {
            family,
            operation,
            type_argument,
            worker_entry,
        })
    };
    let target = match &signature.target {
        hir::OperationSignatureTarget::Ambient(function) => intrinsic(
            l::IntrinsicFamily::Ambient,
            intrinsic_index(&hir::AmbientFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::ContextBytes(function, ty) => intrinsic(
            l::IntrinsicFamily::ContextBytes,
            intrinsic_index(&hir::ContextBytesFn::ALL, function),
            Some(ty.clone()),
            None,
        ),
        hir::OperationSignatureTarget::Math(function) => intrinsic(
            l::IntrinsicFamily::Math,
            intrinsic_index(&hir::MathFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Num(function) => intrinsic(
            l::IntrinsicFamily::Number,
            intrinsic_index(&hir::NumFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Date(function) => intrinsic(
            l::IntrinsicFamily::Date,
            intrinsic_index(&hir::DateFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Json(function) => intrinsic(
            l::IntrinsicFamily::Json,
            intrinsic_index(&hir::JsonFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Str(function) => intrinsic(
            l::IntrinsicFamily::String,
            intrinsic_index(&hir::StrFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Regex(function) => intrinsic(
            l::IntrinsicFamily::Regex,
            intrinsic_index(&hir::RegexFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Arr(function) => intrinsic(
            l::IntrinsicFamily::Array,
            intrinsic_index(&hir::ArrFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Map(function) => intrinsic(
            l::IntrinsicFamily::Map,
            intrinsic_index(&hir::MapFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Set(function) => intrinsic(
            l::IntrinsicFamily::Set,
            intrinsic_index(&hir::SetFn::ALL, function),
            None,
            None,
        ),
        hir::OperationSignatureTarget::Worker(function) => intrinsic(
            l::IntrinsicFamily::Worker,
            intrinsic_index(&hir::WorkerFn::ALL, &function.intrinsic_identity()),
            None,
            match function {
                hir::WorkerFn::Spawn(index) => Some(*index as u32),
                _ => None,
            },
        ),
        hir::OperationSignatureTarget::BuiltinMethod(method) => {
            l::CallSignatureTarget::BuiltinMethod(lower_builtin_method(*method))
        }
    };
    l::CallSignature {
        target,
        parameter_types: signature
            .parameter_types
            .iter()
            .cloned()
            .map(l::ValueType::Data)
            .collect(),
        return_type: signature.return_type.clone().map(l::ValueType::Data),
    }
}

fn lower_builtin_method(method: hir::BuiltinMethod) -> l::BuiltinMethod {
    match method {
        hir::BuiltinMethod::ArrayPush => l::BuiltinMethod::ArrayPush,
        hir::BuiltinMethod::ArrayPop => l::BuiltinMethod::ArrayPop,
        hir::BuiltinMethod::StringSlice => l::BuiltinMethod::StringSlice,
        hir::BuiltinMethod::GeneratorNext => l::BuiltinMethod::GeneratorNext,
    }
}

fn convert_provenance(value: &hir::ForeignTypeProvenance) -> l::ForeignTypeProvenance {
    match value {
        hir::ForeignTypeProvenance::Descriptor {
            aggregate,
            element,
            element_const,
        } => l::ForeignTypeProvenance::Descriptor {
            aggregate: aggregate.clone(),
            element: element.clone(),
            element_const: *element_const,
        },
        hir::ForeignTypeProvenance::ScalarPair {
            element,
            element_const,
        } => l::ForeignTypeProvenance::ScalarPair {
            element: element.clone(),
            element_const: *element_const,
        },
        hir::ForeignTypeProvenance::StringView { aggregate } => {
            l::ForeignTypeProvenance::StringView {
                aggregate: aggregate.clone(),
            }
        }
        hir::ForeignTypeProvenance::Callback { typedef_name } => {
            l::ForeignTypeProvenance::Callback {
                typedef_name: typedef_name.clone(),
            }
        }
        _ => unreachable!("new foreign provenance requires an explicit LIR form"),
    }
}

fn convert_traps(sites: &[hir::TrapSite]) -> Vec<l::Trap> {
    sites
        .iter()
        .map(|site| l::Trap {
            kind: match site {
                hir::TrapSite::Allocation { .. } => l::TrapKind::Allocation,
                hir::TrapSite::Call { .. } => l::TrapKind::Call,
                hir::TrapSite::Unreachable { .. } => l::TrapKind::Unreachable,
                hir::TrapSite::DivisionByZero { .. } => l::TrapKind::DivisionByZero,
                hir::TrapSite::IndexRead { .. } => l::TrapKind::IndexRead,
                hir::TrapSite::IndexWrite { .. } => l::TrapKind::IndexWrite,
                hir::TrapSite::JsonResultValue { .. } => {
                    l::TrapKind::JsonResultValue(l::FieldId(u32::MAX))
                }
                hir::TrapSite::NullNarrowing { .. } => l::TrapKind::NullNarrowing,
                hir::TrapSite::ClassMismatch { class, .. } => l::TrapKind::ClassMismatch(*class),
                hir::TrapSite::DevOnlyLifetime { .. } => l::TrapKind::DevOnlyLifetime,
                hir::TrapSite::DevReloadOnlyStaleCoroutine { .. } => {
                    l::TrapKind::DevReloadOnlyStaleCoroutine
                }
                hir::TrapSite::WireEnumValue { alias, .. } => l::TrapKind::WireEnumValue(*alias),
            },
            pos: site.pos().clone(),
        })
        .collect()
}

#[derive(Clone)]
struct BlockDraft {
    id: l::BlockId,
    source_name: Option<String>,
    parameters: Vec<l::ValueId>,
    state_bindings: Vec<BindingId>,
    instructions: Vec<l::Instruction>,
    terminator: Option<l::Terminator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct BindingId(usize);

#[derive(Clone)]
struct Binding {
    source_name: String,
    ty: l::ValueType,
    mutable: bool,
    storage: Option<l::LocalId>,
    value: Option<l::Operand>,
}

fn is_async_owner_type(ty: &l::ValueType) -> bool {
    matches!(ty, l::ValueType::Data(Type::AsyncHandle(_)))
        || matches!(ty, l::ValueType::Data(Type::Array(element)) if matches!(&**element, Type::AsyncHandle(_)))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BindingSite {
    file: String,
    line: u32,
    col: u32,
    source_name: String,
}

impl BindingSite {
    fn new(source_name: &str, pos: &Pos) -> Self {
        Self {
            file: pos.file.clone(),
            line: pos.line,
            col: pos.col,
            source_name: source_name.to_string(),
        }
    }
}

#[derive(Clone)]
struct Control {
    break_target: l::BlockId,
    continue_target: Option<l::BlockId>,
    scope_depth: usize,
}

fn is_place_expr(expr: &hir::Expr) -> bool {
    matches!(
        expr.kind,
        hir::ExprKind::Local(_)
            | hir::ExprKind::Global(_)
            | hir::ExprKind::Field { .. }
            | hir::ExprKind::Index { .. }
    )
}

fn is_value_class(module: &hir::Module, ty: &Type) -> bool {
    matches!(ty, Type::Class(id) if module.classes.get(id.0).is_some_and(|class| class.is_value))
}

fn is_stored_aggregate(module: &hir::Module, ty: &Type) -> bool {
    matches!(ty, Type::FixedArray(..) | Type::IterResult(_)) || is_value_class(module, ty)
}

#[derive(Clone)]
struct PreparedPlace {
    kind: PreparedPlaceKind,
    traps: Vec<l::Trap>,
}

#[derive(Clone)]
enum PreparedPlaceKind {
    ExistingAddress(l::Operand, Type),
    BoxedBoundary(l::Operand, Type),
    Local(l::LocalId, Type),
    Global(l::GlobalId, Type),
    Field {
        base: PreparedBase,
        field: l::FieldRef,
        ty: Type,
    },
    Index {
        base: PreparedBase,
        index: l::Operand,
        checked: bool,
        ty: Type,
    },
}

#[derive(Clone)]
enum PreparedBase {
    Value(l::Operand),
    Place(Box<PreparedPlace>),
}

#[derive(Clone, Copy)]
enum OwnerStoreAction {
    Acquire(hir::AsyncCopySite),
    Move,
}

struct StoredOperand {
    index: usize,
    ty: l::ValueType,
    action: OwnerStoreAction,
    pos: Pos,
}

fn collect_place_traps(place: &PreparedPlace, traps: &mut Vec<l::Trap>) {
    traps.extend(place.traps.iter().cloned());
    let base = match &place.kind {
        PreparedPlaceKind::Field { base, .. } | PreparedPlaceKind::Index { base, .. } => base,
        PreparedPlaceKind::ExistingAddress(..)
        | PreparedPlaceKind::BoxedBoundary(..)
        | PreparedPlaceKind::Local(..)
        | PreparedPlaceKind::Global(..) => return,
    };
    if let PreparedBase::Place(base) = base {
        collect_place_traps(base, traps);
    }
}

fn prepare_place_after_checked_read(place: &mut PreparedPlace) {
    place.traps.clear();
    let base = match &mut place.kind {
        PreparedPlaceKind::Index { base, checked, .. } => {
            *checked = false;
            base
        }
        PreparedPlaceKind::Field { base, .. } => base,
        PreparedPlaceKind::ExistingAddress(..)
        | PreparedPlaceKind::BoxedBoundary(..)
        | PreparedPlaceKind::Local(..)
        | PreparedPlaceKind::Global(..) => return,
    };
    if let PreparedBase::Place(base) = base {
        prepare_place_after_checked_read(base);
    }
}

fn prepare_direct_index_store(place: &mut PreparedPlace, assignment_traps: &[l::Trap]) {
    let PreparedPlaceKind::Index { base, checked, .. } = &mut place.kind else {
        return;
    };
    if let PreparedBase::Place(base) = base {
        prepare_place_after_checked_read(base);
    }
    place.traps = assignment_traps
        .iter()
        .filter(|trap| trap.kind == l::TrapKind::IndexWrite)
        .cloned()
        .collect();
    if place.traps.is_empty() {
        *checked = false;
    }
}

fn prepare_direct_index_assignment(place: &mut PreparedPlace, assignment_traps: &[l::Trap]) {
    if !matches!(place.kind, PreparedPlaceKind::Index { .. }) {
        return;
    }
    place.traps = assignment_traps
        .iter()
        .filter(|trap| {
            matches!(
                trap.kind,
                l::TrapKind::DevOnlyLifetime | l::TrapKind::IndexWrite
            )
        })
        .cloned()
        .collect();
}

struct FunctionBuilder<'a, 'm> {
    lowering: &'a mut Lowering<'m>,
    function: FunctionInput,
    id: l::FunctionId,
    kind: l::FunctionKind,
    parameters: Vec<l::Parameter>,
    locals: Vec<l::Local>,
    values: Vec<l::Value>,
    blocks: Vec<BlockDraft>,
    entry: l::BlockId,
    current: Option<l::BlockId>,
    scopes: Vec<HashMap<String, BindingId>>,
    bindings: Vec<Binding>,
    address_taken: HashSet<BindingSite>,
    substitutions: Vec<HashMap<String, l::Operand>>,
    this_value: Option<l::Operand>,
    controls: Vec<Control>,
    array_values: Vec<l::ValueId>,
    moved_async_owners: HashSet<l::ValueId>,
}

type CallResolution = (
    l::CallTargetKind,
    Vec<l::Operand>,
    Vec<CallParam>,
    Option<PreparedBase>,
);

fn intrinsic_resolution(
    family: l::IntrinsicFamily,
    operation: u16,
    type_argument: Option<Type>,
    worker_entry: Option<u32>,
) -> CallResolution {
    (
        l::CallTargetKind::Intrinsic(l::Intrinsic {
            family,
            operation,
            type_argument,
            worker_entry,
        }),
        Vec::new(),
        Vec::new(),
        None,
    )
}

fn intrinsic_index<T: PartialEq>(values: &[T], value: &T) -> u16 {
    values
        .iter()
        .position(|candidate| candidate == value)
        .expect("HIR intrinsic belongs to its ALL table") as u16
}

fn convert_unary(value: hir::UnOp) -> l::UnaryOp {
    match value {
        hir::UnOp::Neg => l::UnaryOp::Neg,
        hir::UnOp::Not => l::UnaryOp::Not,
        hir::UnOp::BitNot => l::UnaryOp::BitNot,
        _ => unreachable!("unknown checked unary operator"),
    }
}

fn convert_binary(value: hir::BinOp) -> Result<l::BinaryOp, LowerError> {
    Ok(match value {
        hir::BinOp::Add => l::BinaryOp::Add,
        hir::BinOp::Sub => l::BinaryOp::Sub,
        hir::BinOp::Mul => l::BinaryOp::Mul,
        hir::BinOp::Div => l::BinaryOp::Div,
        hir::BinOp::Rem => l::BinaryOp::Rem,
        hir::BinOp::Eq => l::BinaryOp::Eq,
        hir::BinOp::Ne => l::BinaryOp::Ne,
        hir::BinOp::Lt => l::BinaryOp::Lt,
        hir::BinOp::Le => l::BinaryOp::Le,
        hir::BinOp::Gt => l::BinaryOp::Gt,
        hir::BinOp::Ge => l::BinaryOp::Ge,
        hir::BinOp::BitAnd => l::BinaryOp::BitAnd,
        hir::BinOp::BitOr => l::BinaryOp::BitOr,
        hir::BinOp::BitXor => l::BinaryOp::BitXor,
        hir::BinOp::Shl => l::BinaryOp::Shl,
        hir::BinOp::Shr => l::BinaryOp::Shr,
        hir::BinOp::UShr => l::BinaryOp::UShr,
        hir::BinOp::And | hir::BinOp::Or => {
            return Err(LowerError {
                pos: Pos::new("<operator>", 1, 1),
                message: "short-circuit operator reached scalar instruction lowering".to_string(),
            });
        }
        _ => {
            return Err(LowerError {
                pos: Pos::new("<operator>", 1, 1),
                message: format!("unrecognized binary operator {value:?}"),
            });
        }
    })
}

fn convert_for_of(value: hir::ForOfKind) -> l::ForOfKind {
    match value {
        hir::ForOfKind::ArrayValues => l::ForOfKind::ArrayValues,
        hir::ForOfKind::ArrayKeys => l::ForOfKind::ArrayKeys,
        hir::ForOfKind::FixedArrayValues => l::ForOfKind::FixedArrayValues,
        hir::ForOfKind::MapKeys => l::ForOfKind::MapKeys,
        hir::ForOfKind::MapValues => l::ForOfKind::MapValues,
        hir::ForOfKind::SetValues => l::ForOfKind::SetValues,
        hir::ForOfKind::StringCodePoints => l::ForOfKind::StringCodePoints,
        _ => unreachable!("unknown checked for-of kind"),
    }
}

fn convert_spread(value: hir::SpreadKind) -> l::SpreadKind {
    match value {
        hir::SpreadKind::Array => l::SpreadKind::Array,
        hir::SpreadKind::FixedArray => l::SpreadKind::FixedArray,
        hir::SpreadKind::SetValues => l::SpreadKind::SetValues,
        hir::SpreadKind::StringCodePoints => l::SpreadKind::StringCodePoints,
        _ => unreachable!("unknown checked spread kind"),
    }
}

fn constant_expr(expr: &hir::Expr) -> Option<l::Constant> {
    Some(match &expr.kind {
        hir::ExprKind::Int(value) => l::Constant {
            ty: expr.ty.clone(),
            kind: l::ConstantKind::Integer(*value),
        },
        hir::ExprKind::Bool(value) => l::Constant {
            ty: Type::Bool,
            kind: l::ConstantKind::Boolean(*value),
        },
        hir::ExprKind::EnumMember { value, .. } => l::Constant {
            ty: expr.ty.clone(),
            kind: l::ConstantKind::Integer(*value),
        },
        _ => return None,
    })
}

fn stmt_pos(statement: &hir::Stmt) -> Pos {
    match statement {
        hir::Stmt::Let { pos, .. }
        | hir::Stmt::Return { pos, .. }
        | hir::Stmt::If { pos, .. }
        | hir::Stmt::While { pos, .. }
        | hir::Stmt::For { pos, .. }
        | hir::Stmt::ForOf { pos, .. }
        | hir::Stmt::Switch { pos, .. }
        | hir::Stmt::Break(pos)
        | hir::Stmt::Continue(pos) => pos.clone(),
        hir::Stmt::Expr(expr) => expr.pos.clone(),
        hir::Stmt::Block(statements) => statements
            .first()
            .map(stmt_pos)
            .unwrap_or_else(|| Pos::new("<block>", 1, 1)),
    }
}

fn target(block: l::BlockId, arguments: Vec<l::Operand>) -> l::BlockTarget {
    l::BlockTarget { block, arguments }
}

fn i32_constant(value: i32) -> l::Operand {
    l::Operand::Constant(l::Constant {
        ty: Type::I32,
        kind: l::ConstantKind::Integer(i64::from(value)),
    })
}

fn bool_constant(value: bool) -> l::Operand {
    l::Operand::Constant(l::Constant {
        ty: Type::Bool,
        kind: l::ConstantKind::Boolean(value),
    })
}

fn branch(block: l::BlockId) -> l::Terminator {
    l::Terminator::Branch(target(block, Vec::new()))
}

fn address_base(ty: &l::ValueType) -> Option<l::ValueId> {
    match ty {
        l::ValueType::Address(address) => address.array_base,
        _ => None,
    }
}

#[cfg(test)]
mod verifier_tests {
    use super::*;

    fn pos() -> Pos {
        Pos::new("verifier.ts", 1, 1)
    }

    fn base_module() -> l::Module {
        let array_type = Type::Array(Box::new(Type::I32));
        let address_type = l::ValueType::Address(l::AddressType {
            pointee: Type::I32,
            array_base: Some(l::ValueId(0)),
        });
        let function = l::Function {
            id: l::FunctionId(0),
            source_name: "verify".to_string(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: vec![l::Parameter {
                storage: Some(l::LocalId(0)),
                value: l::ValueId(0),
                source_name: "array".to_string(),
                kind: l::ParameterKind::Explicit,
                pos: pos(),
            }],
            return_type: Type::Void,
            locals: vec![l::Local {
                id: l::LocalId(0),
                source_name: "array".to_string(),
                ty: l::ValueType::Data(array_type.clone()),
                storage: l::LocalStorageClass::Activation,
                mutable: true,
                pos: pos(),
            }],
            values: vec![
                l::Value {
                    id: l::ValueId(0),
                    ty: l::ValueType::Data(array_type.clone()),
                    fresh_owner: false,
                    source_name: Some("array".to_string()),
                },
                l::Value {
                    id: l::ValueId(1),
                    ty: address_type.clone(),
                    fresh_owner: false,
                    source_name: None,
                },
                l::Value {
                    id: l::ValueId(2),
                    ty: l::ValueType::Data(Type::I32),
                    fresh_owner: false,
                    source_name: None,
                },
            ],
            liveness: l::Liveness::default(),
            blocks: vec![l::BasicBlock {
                id: l::BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: vec![
                    l::Instruction {
                        result: Some(l::ValueId(1)),
                        kind: l::InstructionKind::AddressOfIndex { checked: true },
                        operands: vec![
                            l::Operand::Value(l::ValueId(0)),
                            l::Operand::Constant(l::Constant {
                                ty: Type::I32,
                                kind: l::ConstantKind::Integer(0),
                            }),
                        ],
                        invalidates: Vec::new(),
                        traps: vec![l::Trap {
                            kind: l::TrapKind::IndexRead,
                            pos: pos(),
                        }],
                        pos: pos(),
                    },
                    l::Instruction {
                        result: Some(l::ValueId(2)),
                        kind: l::InstructionKind::LoadAddress,
                        operands: vec![l::Operand::Value(l::ValueId(1))],
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                        pos: pos(),
                    },
                ],
                terminator: l::Terminator::Return {
                    value: None,
                    pos: pos(),
                },
            }],
            entry: l::BlockId(0),
            pos: pos(),
        };
        l::Module {
            entry: Some(l::FunctionId(0)),
            async_roots: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            string_aliases: Vec::new(),
            globals: Vec::new(),
            foreign_functions: Vec::new(),
            functions: vec![function],
            worker_entries: Vec::new(),
            intrinsic_operations: intrinsic_operations(),
            initializer: None,
        }
    }

    fn wrong_declared_call_module(kind: l::CallTargetKind) -> l::Module {
        let callee = l::Function {
            id: l::FunctionId(0),
            source_name: "declared".to_string(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: vec![l::Parameter {
                storage: None,
                value: l::ValueId(0),
                source_name: "value".to_string(),
                kind: l::ParameterKind::Explicit,
                pos: pos(),
            }],
            return_type: Type::I32,
            locals: Vec::new(),
            values: vec![l::Value {
                id: l::ValueId(0),
                ty: l::ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("value".to_string()),
            }],
            liveness: l::Liveness::default(),
            blocks: vec![l::BasicBlock {
                id: l::BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: l::Terminator::Return {
                    value: Some(l::Operand::Value(l::ValueId(0))),
                    pos: pos(),
                },
            }],
            entry: l::BlockId(0),
            pos: pos(),
        };
        let caller = l::Function {
            id: l::FunctionId(1),
            source_name: "caller".to_string(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: Vec::new(),
            return_type: Type::Void,
            locals: Vec::new(),
            values: vec![
                l::Value {
                    id: l::ValueId(0),
                    ty: l::ValueType::Data(Type::Str),
                    fresh_owner: false,
                    source_name: None,
                },
                l::Value {
                    id: l::ValueId(1),
                    ty: l::ValueType::Data(Type::Bool),
                    fresh_owner: false,
                    source_name: None,
                },
            ],
            liveness: l::Liveness::default(),
            blocks: vec![l::BasicBlock {
                id: l::BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: vec![
                    l::Instruction {
                        result: Some(l::ValueId(0)),
                        kind: l::InstructionKind::StringLiteral("wrong".to_string()),
                        operands: Vec::new(),
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                        pos: pos(),
                    },
                    l::Instruction {
                        result: Some(l::ValueId(1)),
                        kind: l::InstructionKind::Call(l::CallTarget {
                            kind,
                            parameter_types: vec![
                                l::ValueType::Data(Type::Str),
                                l::ValueType::Data(Type::I32),
                                l::ValueType::Data(Type::I32),
                            ],
                            return_type: Some(l::ValueType::Data(Type::Bool)),
                        }),
                        operands: vec![
                            l::Operand::Value(l::ValueId(0)),
                            l::Operand::Constant(l::Constant {
                                ty: Type::I32,
                                kind: l::ConstantKind::Integer(1),
                            }),
                            l::Operand::Constant(l::Constant {
                                ty: Type::I32,
                                kind: l::ConstantKind::Integer(2),
                            }),
                        ],
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                        pos: pos(),
                    },
                ],
                terminator: l::Terminator::Return {
                    value: None,
                    pos: pos(),
                },
            }],
            entry: l::BlockId(0),
            pos: pos(),
        };
        l::Module {
            entry: Some(l::FunctionId(0)),
            async_roots: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            string_aliases: Vec::new(),
            globals: Vec::new(),
            foreign_functions: Vec::new(),
            functions: vec![callee, caller],
            worker_entries: Vec::new(),
            intrinsic_operations: intrinsic_operations(),
            initializer: None,
        }
    }

    fn hand_built_call_module(
        kind: l::CallTargetKind,
        declared_parameters: Vec<l::ValueType>,
        declared_return: Option<l::ValueType>,
        actual_parameters: Vec<l::ValueType>,
        actual_return: Option<l::ValueType>,
    ) -> l::Module {
        let operation_target = match &kind {
            l::CallTargetKind::Intrinsic(intrinsic) => {
                Some(l::CallSignatureTarget::Intrinsic(intrinsic.clone()))
            }
            l::CallTargetKind::BuiltinMethod(method) => {
                Some(l::CallSignatureTarget::BuiltinMethod(*method))
            }
            l::CallTargetKind::Function(_)
            | l::CallTargetKind::StaticClosure(_)
            | l::CallTargetKind::Method(_)
            | l::CallTargetKind::Foreign(_)
            | l::CallTargetKind::Indirect => None,
        };
        let target_parameters = if operation_target.is_some() {
            Vec::new()
        } else {
            declared_parameters.clone()
        };
        let parameters = actual_parameters
            .iter()
            .enumerate()
            .map(|(index, _)| l::Parameter {
                storage: None,
                value: l::ValueId(index as u32),
                source_name: format!("arg{index}"),
                kind: l::ParameterKind::Explicit,
                pos: pos(),
            })
            .collect::<Vec<_>>();
        let mut values = actual_parameters
            .into_iter()
            .enumerate()
            .map(|(index, ty)| l::Value {
                id: l::ValueId(index as u32),
                ty,
                fresh_owner: false,
                source_name: Some(format!("arg{index}")),
            })
            .collect::<Vec<_>>();
        let result = actual_return.map(|ty| {
            let id = l::ValueId(values.len() as u32);
            values.push(l::Value {
                id,
                ty,
                fresh_owner: false,
                source_name: None,
            });
            id
        });
        let operands = parameters
            .iter()
            .map(|parameter| l::Operand::Value(parameter.value))
            .collect();
        let function = l::Function {
            id: l::FunctionId(0),
            source_name: "hand-built-caller".to_string(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters,
            return_type: Type::Void,
            locals: Vec::new(),
            values,
            liveness: l::Liveness::default(),
            blocks: vec![l::BasicBlock {
                id: l::BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: vec![l::Instruction {
                    result,
                    kind: l::InstructionKind::Call(l::CallTarget {
                        kind,
                        parameter_types: target_parameters,
                        return_type: declared_return.clone(),
                    }),
                    operands,
                    invalidates: Vec::new(),
                    traps: Vec::new(),
                    pos: pos(),
                }],
                terminator: l::Terminator::Return {
                    value: None,
                    pos: pos(),
                },
            }],
            entry: l::BlockId(0),
            pos: pos(),
        };
        let mut intrinsic_operations = intrinsic_operations();
        if let (Some(row), Some(target)) = (intrinsic_operations.first_mut(), operation_target) {
            row.signatures.push(l::CallSignature {
                target,
                parameter_types: declared_parameters,
                return_type: declared_return,
            });
        }
        l::Module {
            entry: Some(l::FunctionId(0)),
            async_roots: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            string_aliases: Vec::new(),
            globals: Vec::new(),
            foreign_functions: Vec::new(),
            functions: vec![function],
            worker_entries: Vec::new(),
            intrinsic_operations,
            initializer: None,
        }
    }

    #[test]
    fn valid_address_graph_passes() {
        verify_module(&base_module()).expect("valid graph");
    }

    #[test]
    fn checked_index_without_bounds_trap_is_rejected() {
        let mut module = base_module();
        module.functions[0].blocks[0].instructions[0].traps.clear();
        let errors = verify_module(&module).expect_err("missing bounds trap must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("index address check disagrees with its bounds trap")));
    }

    #[test]
    fn missing_module_entry_is_rejected() {
        let mut module = base_module();
        module.entry = Some(l::FunctionId(99));
        let errors = verify_module(&module).expect_err("missing module entry must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("module entry function 99 is missing")));
    }

    #[test]
    fn entryless_module_is_valid() {
        let mut module = base_module();
        module.entry = None;
        verify_module(&module).expect("host-callable entryless module is valid");
    }

    #[test]
    fn invalid_async_root_is_rejected() {
        let mut module = base_module();
        module.async_roots.push(l::FunctionId(0));
        let errors = verify_module(&module).expect_err("invalid async root must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("is not an exported zero-parameter non-entry async function")));
    }

    #[test]
    fn address_use_after_invalidation_is_rejected() {
        let mut module = base_module();
        let function = &mut module.functions[0];
        function.values.push(l::Value {
            id: l::ValueId(3),
            ty: l::ValueType::Data(Type::Array(Box::new(Type::I32))),
            fresh_owner: false,
            source_name: None,
        });
        function.blocks[0].instructions.insert(
            1,
            l::Instruction {
                result: Some(l::ValueId(3)),
                kind: l::InstructionKind::Copy,
                operands: vec![l::Operand::Value(l::ValueId(0))],
                invalidates: vec![l::ValueId(0)],
                traps: Vec::new(),
                pos: pos(),
            },
        );
        let errors = verify_module(&module).expect_err("crossing address must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("after array value 0 was invalidated")));
    }

    #[test]
    fn use_before_definition_is_rejected() {
        let mut module = base_module();
        module.functions[0].blocks[0].instructions.swap(0, 1);
        let errors = verify_module(&module).expect_err("dominance must fail");
        assert!(errors
            .iter()
            .any(|error| error.message.contains("not dominated")));
    }

    #[test]
    fn duplicate_and_missing_definitions_are_rejected() {
        let mut module = base_module();
        module.functions[0].blocks[0].instructions[1].result = Some(l::ValueId(1));
        let errors = verify_module(&module).expect_err("definition count must fail");
        assert!(errors
            .iter()
            .any(|error| error.message.contains("value 1 has 2 definitions")));
        assert!(errors
            .iter()
            .any(|error| error.message.contains("value 2 has 0 definitions")));
    }

    #[test]
    fn operand_type_mismatch_is_rejected() {
        let mut module = base_module();
        module.functions[0].blocks[0].instructions[1].operands =
            vec![l::Operand::Value(l::ValueId(0))];
        let errors = verify_module(&module).expect_err("operand type must fail");
        assert!(errors
            .iter()
            .any(|error| error.message.contains("address load signature is invalid")));
    }

    #[test]
    fn suspend_argument_arity_is_checked_against_live_in_parameters() {
        let mut module = base_module();
        let function = &mut module.functions[0];
        function.is_async = true;
        function.locals.clear();
        function.parameters[0].storage = None;
        function.values = vec![
            l::Value {
                id: l::ValueId(0),
                ty: l::ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("before".to_string()),
            },
            l::Value {
                id: l::ValueId(1),
                ty: l::ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("after".to_string()),
            },
        ];
        function.parameters[0].value = l::ValueId(0);
        function.blocks = vec![
            l::BasicBlock {
                id: l::BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: l::Terminator::Suspend {
                    kind: l::SuspendKind::Async,
                    pos: pos(),
                    successor: l::BlockId(1),
                    resume_value: None,
                    arguments: Vec::new(),
                    invalidates: Vec::new(),
                    traps: Vec::new(),
                },
            },
            l::BasicBlock {
                id: l::BlockId(1),
                source_name: Some("resume".to_string()),
                parameters: vec![l::ValueId(1)],
                instructions: Vec::new(),
                terminator: l::Terminator::Return {
                    value: None,
                    pos: pos(),
                },
            },
        ];
        let errors = verify_module(&module).expect_err("missing suspend argument must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("has 0 arguments for 1 live-in parameters")));
    }

    #[test]
    fn call_disagreeing_with_declared_callee_is_rejected() {
        let module = wrong_declared_call_module(l::CallTargetKind::Function(l::FunctionId(0)));
        let errors = verify_module(&module).expect_err("declared call mismatch must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("call signature disagrees with the target declaration")));
    }

    #[test]
    fn foreign_array_call_without_snapshot_operands_is_rejected() {
        let array = Type::Array(Box::new(Type::I32));
        let mut module = hand_built_call_module(
            l::CallTargetKind::Foreign(l::ForeignFunctionId(0)),
            vec![l::ValueType::Data(array.clone())],
            None,
            vec![l::ValueType::Data(array.clone())],
            None,
        );
        module.foreign_functions.push(l::ForeignFunction {
            id: l::ForeignFunctionId(0),
            source_name: "consume".to_string(),
            parameters: vec![l::ForeignParameter {
                source_name: "values".to_string(),
                ty: array,
                foreign_provenance: Some(l::ForeignTypeProvenance::Descriptor {
                    aggregate: "Values".to_string(),
                    element: "int32_t".to_string(),
                    element_const: true,
                }),
                pos: pos(),
            }],
            return_type: Type::Void,
            include: "probe.h".to_string(),
            pos: pos(),
        });
        let errors = verify_module(&module).expect_err("missing snapshot operands must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("call signature disagrees with the target declaration")));
    }

    #[test]
    fn intrinsic_operation_missing_from_lir_table_is_rejected() {
        let module = wrong_declared_call_module(l::CallTargetKind::Intrinsic(l::Intrinsic {
            family: l::IntrinsicFamily::Math,
            operation: 60_000,
            type_argument: None,
            worker_entry: None,
        }));
        let errors = verify_module(&module).expect_err("unknown intrinsic operation must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("call target identity/signature is invalid")));
    }

    #[test]
    fn intrinsic_call_with_wrong_arity_operands_and_result_is_rejected() {
        let module = hand_built_call_module(
            l::CallTargetKind::Intrinsic(l::Intrinsic {
                family: l::IntrinsicFamily::Math,
                operation: 0,
                type_argument: None,
                worker_entry: None,
            }),
            vec![l::ValueType::Data(Type::F64)],
            Some(l::ValueType::Data(Type::F64)),
            vec![l::ValueType::Data(Type::Str); 3],
            Some(l::ValueType::Data(Type::Bool)),
        );
        let errors = verify_module(&module).expect_err("wrong intrinsic call must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("call disagrees with the signature table")));
    }

    #[test]
    fn builtin_method_call_with_wrong_receiver_arity_and_result_is_rejected() {
        let module = hand_built_call_module(
            l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPush),
            vec![
                l::ValueType::Data(Type::Array(Box::new(Type::I32))),
                l::ValueType::Data(Type::I32),
            ],
            None,
            vec![l::ValueType::Data(Type::Str)],
            Some(l::ValueType::Data(Type::Bool)),
        );
        let errors = verify_module(&module).expect_err("wrong built-in call must fail");
        assert!(errors.iter().any(|error| error
            .message
            .contains("call disagrees with the signature table")));
    }

    #[test]
    fn indirect_call_is_checked_against_the_callee_function_type() {
        let function_type = Type::Func(Box::new(subscript_compiler::types::FuncType {
            params: vec![Type::I32],
            ret: Type::I32,
        }));
        let module = hand_built_call_module(
            l::CallTargetKind::Indirect,
            vec![l::ValueType::Data(function_type.clone()); 3],
            Some(l::ValueType::Data(Type::Bool)),
            vec![l::ValueType::Data(function_type); 3],
            Some(l::ValueType::Data(Type::Bool)),
        );
        let errors = verify_module(&module).expect_err("wrong indirect call must fail");
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("call signature disagrees with the target declaration")
        }));
    }
}
