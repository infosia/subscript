//! Typed HIR-to-LIR lowering and the mandatory LIR verifier.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;

use subscript_compiler::hir;
use subscript_compiler::lir as l;
use subscript_compiler::{ClassId, Pos, Type};

use crate::lir_types::boundary_box_class;

mod unroll;

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

impl<'a> Lowering<'a> {
    fn new(module: &'a hir::Module) -> Result<Self, LowerError> {
        let fallback = || Pos::new("<module>", 1, 1);
        if !module.poisoned_imports.is_empty() {
            return Err(LowerError {
                pos: module
                    .poisoned_imports
                    .first()
                    .map_or_else(fallback, |poison| poison.pos.clone()),
                message: "discovery HIR with poisoned imports is not executable LIR".to_string(),
            });
        }

        let mut next_function = 0_u32;
        let mut next_method = 0_u32;
        let mut free_functions = HashMap::new();
        let mut methods = HashMap::new();
        for (class_index, class) in module.classes.iter().enumerate() {
            if class.ctor.is_some() {
                let record = FunctionRecord {
                    id: l::FunctionId(next_function),
                    method: Some(l::MethodId(next_method)),
                };
                next_function += 1;
                next_method += 1;
                methods.insert((class_index, "constructor".to_string()), record);
            }
            for method in &class.methods {
                let record = FunctionRecord {
                    id: l::FunctionId(next_function),
                    method: Some(l::MethodId(next_method)),
                };
                next_function += 1;
                next_method += 1;
                if methods
                    .insert((class_index, method.name.clone()), record)
                    .is_some()
                {
                    return Err(LowerError {
                        pos: method.pos.clone(),
                        message: format!(
                            "class `{}` has duplicate checked method `{}`",
                            class.name, method.name
                        ),
                    });
                }
            }
        }
        for function in &module.functions {
            let record = FunctionRecord {
                id: l::FunctionId(next_function),
                method: None,
            };
            next_function += 1;
            if free_functions
                .insert(function.name.clone(), record)
                .is_some()
            {
                return Err(LowerError {
                    pos: function.pos.clone(),
                    message: format!("duplicate checked function `{}`", function.name),
                });
            }
        }

        let globals = module
            .globals
            .iter()
            .enumerate()
            .map(|(index, global)| (global.name.clone(), l::GlobalId(index as u32)))
            .collect();
        let foreign_functions = module
            .foreign_fns
            .iter()
            .enumerate()
            .map(|(index, function)| (function.name.clone(), l::ForeignFunctionId(index as u32)))
            .collect();

        let mut fields = HashMap::new();
        let mut next_field = 0_u32;
        for (class_index, class) in module.classes.iter().enumerate() {
            for field in &class.fields {
                fields.insert((class_index, field.name.clone()), l::FieldId(next_field));
                next_field += 1;
            }
        }

        let mut lowering = Self {
            hir: module,
            free_functions,
            methods,
            foreign_functions,
            globals,
            fields,
            functions: vec![None; next_function as usize],
            next_function,
            classes: Vec::new(),
            foreign: Vec::new(),
        };
        lowering.classes = lowering.lower_classes()?;
        lowering.foreign = lowering.lower_foreign()?;
        Ok(lowering)
    }

    fn run(mut self) -> Result<l::Module, LowerError> {
        for (class_index, class) in self.hir.classes.iter().cloned().enumerate() {
            if let Some(constructor) = class.ctor {
                let record = self.method_record(class_index, "constructor", &constructor.pos)?;
                self.lower_function(
                    record.id,
                    constructor,
                    l::FunctionKind::Constructor {
                        class: ClassId(class_index),
                        method: record.method.expect("constructor method id"),
                    },
                    Some(ClassId(class_index)),
                    Vec::new(),
                )?;
            }
            for method in class.methods {
                let record = self.method_record(class_index, &method.name, &method.pos)?;
                self.lower_function(
                    record.id,
                    method,
                    l::FunctionKind::Method {
                        class: ClassId(class_index),
                        method: record.method.expect("method id"),
                    },
                    Some(ClassId(class_index)),
                    Vec::new(),
                )?;
            }
        }
        for function in self.hir.functions.iter().cloned() {
            let record = self
                .free_functions
                .get(&function.name)
                .cloned()
                .ok_or_else(|| LowerError {
                    pos: function.pos.clone(),
                    message: format!("missing id for function `{}`", function.name),
                })?;
            self.lower_function(record.id, function, l::FunctionKind::Free, None, Vec::new())?;
        }

        let initializer = if self.hir.globals.is_empty() && self.hir.top_level.is_empty() {
            None
        } else {
            let id = self.allocate_function_id();
            let pos = self
                .hir
                .globals
                .first()
                .map(|global| global.pos.clone())
                .or_else(|| self.hir.top_level.first().map(stmt_pos))
                .unwrap_or_else(|| Pos::new("<module>", 1, 1));
            let function = FunctionInput {
                name: "<module initializer>".to_string(),
                exported: false,
                is_generator: false,
                is_async: false,
                creation_traps: Vec::new(),
                host_entry_traps: None,
                params: Vec::new(),
                ret: Type::Void,
                body: self.hir.top_level.clone(),
                pos: pos.clone(),
            };
            let globals = self.hir.globals.clone();
            let mut builder = FunctionBuilder::new(
                &mut self,
                id,
                function,
                l::FunctionKind::ModuleInitializer,
                None,
                Vec::new(),
            )?;
            let top_level = builder.function.body.clone();
            if let Some(global) = globals
                .iter()
                .find(|global| global.initializer_index > top_level.len())
            {
                return Err(builder.error(
                    &global.pos,
                    "global initializer position is after the module body",
                ));
            }
            for initializer_index in 0..=top_level.len() {
                if builder.current.is_none() {
                    break;
                }
                for global in globals
                    .iter()
                    .filter(|global| global.initializer_index == initializer_index)
                {
                    if builder.current.is_none() {
                        break;
                    }
                    let value =
                        builder.lower_stored_expr_at(&global.ty, &global.init, &global.pos)?;
                    let global_id = builder
                        .lowering
                        .globals
                        .get(&global.name)
                        .copied()
                        .ok_or_else(|| builder.error(&global.pos, "global id is missing"))?;
                    builder.emit_store_instruction(
                        l::InstructionKind::StoreGlobal(global_id),
                        vec![value],
                        vec![StoredOperand {
                            index: 0,
                            ty: l::ValueType::Data(global.ty.clone()),
                            action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Binding),
                            pos: global.pos.clone(),
                        }],
                        (None, false),
                        Vec::new(),
                        global.pos.clone(),
                    )?;
                }
                if let Some(statement) = top_level.get(initializer_index) {
                    builder.lower_statements(std::slice::from_ref(statement))?;
                }
            }
            let lowered = builder.finish()?;
            self.set_function(id, lowered)?;
            Some(id)
        };

        let functions = self
            .functions
            .into_iter()
            .enumerate()
            .map(|(index, function)| {
                function.ok_or_else(|| LowerError {
                    pos: Pos::new("<module>", 1, 1),
                    message: format!("function id {index} was allocated but not lowered"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let worker_entries = self
            .hir
            .worker_entries
            .iter()
            .map(|entry| {
                let function = self
                    .free_functions
                    .get(&entry.function)
                    .map(|record| record.id)
                    .ok_or_else(|| LowerError {
                        pos: Pos::new("<worker entry>", 1, 1),
                        message: format!(
                            "worker entry names unresolved function `{}`",
                            entry.function
                        ),
                    })?;
                Ok(l::WorkerEntry {
                    function,
                    input: entry.input,
                    output: entry.output,
                })
            })
            .collect::<Result<Vec<_>, LowerError>>()?;

        let entry = self
            .hir
            .functions
            .iter()
            .find(|function| function.exported && function.name == "main")
            .and_then(|function| self.free_functions.get(&function.name))
            .map(|record| record.id);
        let async_roots = self
            .hir
            .functions
            .iter()
            .filter(|function| {
                function.exported
                    && function.is_async
                    && function.name != "main"
                    && function.params.is_empty()
            })
            .map(|function| {
                self.free_functions
                    .get(&function.name)
                    .map(|record| record.id)
                    .ok_or_else(|| LowerError {
                        pos: function.pos.clone(),
                        message: "async root has no function id".to_string(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut intrinsic_operations = intrinsic_operations();
        if let Some(row) = intrinsic_operations.first_mut() {
            let mut signatures = self
                .hir
                .operation_signatures
                .iter()
                .map(lower_operation_signature)
                .collect::<Vec<_>>();
            for operation in &self.hir.operation_signatures {
                if !matches!(
                    operation.target,
                    hir::OperationSignatureTarget::Arr(hir::ArrFn::Map | hir::ArrFn::Filter)
                ) {
                    continue;
                }
                let Some(Type::Array(element)) = &operation.return_type else {
                    continue;
                };
                let signature = l::CallSignature {
                    target: l::CallSignatureTarget::BuiltinMethod(l::BuiltinMethod::ArrayPush),
                    parameter_types: vec![
                        l::ValueType::Data(Type::Array(element.clone())),
                        l::ValueType::Data((**element).clone()),
                    ],
                    return_type: Some(l::ValueType::Data(Type::I32)),
                };
                if !signatures.contains(&signature) {
                    signatures.push(signature);
                }
            }
            row.signatures.extend(signatures);
        }
        Ok(l::Module {
            entry,
            async_roots,
            classes: self.classes,
            enums: self
                .hir
                .enums
                .iter()
                .enumerate()
                .map(|(index, definition)| l::Enum {
                    id: subscript_compiler::EnumId(index),
                    source_name: definition.name.clone(),
                    members: definition.members.clone(),
                    pos: definition.pos.clone(),
                })
                .collect(),
            string_aliases: self
                .hir
                .string_aliases
                .iter()
                .enumerate()
                .map(|(index, definition)| l::StringAlias {
                    id: subscript_compiler::StringAliasId(index),
                    source_name: definition.name.clone(),
                    members: definition.members.clone(),
                    wire_values: definition.wire_values.clone(),
                    absence_discriminant: definition.absence_discriminant(),
                    pos: definition.pos.clone(),
                })
                .collect(),
            globals: self
                .hir
                .globals
                .iter()
                .enumerate()
                .map(|(index, global)| l::Global {
                    id: l::GlobalId(index as u32),
                    source_name: global.name.clone(),
                    ty: global.ty.clone(),
                    mutable: global.mutable,
                    pos: global.pos.clone(),
                })
                .collect(),
            foreign_functions: self.foreign,
            functions,
            worker_entries,
            intrinsic_operations,
            initializer,
        })
    }

    fn lower_classes(&self) -> Result<Vec<l::Class>, LowerError> {
        self.hir
            .classes
            .iter()
            .enumerate()
            .map(|(class_index, class)| {
                let constructor = class
                    .ctor
                    .as_ref()
                    .map(|constructor| {
                        self.method_record(class_index, "constructor", &constructor.pos)
                            .map(|record| l::Method {
                                id: record.method.expect("constructor method id"),
                                function: record.id,
                                source_name: "constructor".to_string(),
                            })
                    })
                    .transpose()?;
                let methods = class
                    .methods
                    .iter()
                    .map(|method| {
                        let record = self.method_record(class_index, &method.name, &method.pos)?;
                        Ok(l::Method {
                            id: record.method.expect("method id"),
                            function: record.id,
                            source_name: method.name.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, LowerError>>()?;
                let fields = class
                    .fields
                    .iter()
                    .map(|field| {
                        let id = self
                            .fields
                            .get(&(class_index, field.name.clone()))
                            .copied()
                            .ok_or_else(|| LowerError {
                                pos: field.pos.clone(),
                                message: format!("missing id for field `{}`", field.name),
                            })?;
                        Ok(l::Field {
                            id,
                            source_name: field.name.clone(),
                            ty: field.ty.clone(),
                            is_defaulted: field.is_defaulted,
                            is_absence_capable: field.is_absence_capable,
                            foreign_provenance: field
                                .foreign_provenance
                                .as_ref()
                                .map(convert_provenance),
                            pos: field.pos.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, LowerError>>()?;
                Ok(l::Class {
                    id: ClassId(class_index),
                    source_name: class.name.clone(),
                    is_value: class.is_value,
                    is_descriptor: class.is_descriptor,
                    is_boundary: class.is_boundary,
                    is_embedded_header: boundary_class_is_embedded_header(
                        self.hir,
                        ClassId(class_index),
                    ),
                    alignment: class.alignment_override.as_ref().map(|value| value.value),
                    fields,
                    constructor,
                    methods,
                    index_signature: class.index_signature.as_ref().map(|signature| {
                        l::IndexSignature {
                            index_type: signature.index_ty.clone(),
                            element_type: signature.element_ty.clone(),
                            readonly: signature.readonly,
                        }
                    }),
                    pos: class.pos.clone(),
                })
            })
            .collect()
    }

    fn lower_foreign(&self) -> Result<Vec<l::ForeignFunction>, LowerError> {
        self.hir
            .foreign_fns
            .iter()
            .enumerate()
            .map(|(index, function)| {
                let include = self
                    .hir
                    .foreign_mirrors
                    .get(function.mirror.0)
                    .map(|mirror| mirror.include.clone())
                    .ok_or_else(|| LowerError {
                        pos: function.pos.clone(),
                        message: format!(
                            "foreign function `{}` has an invalid mirror id",
                            function.name
                        ),
                    })?;
                Ok(l::ForeignFunction {
                    id: l::ForeignFunctionId(index as u32),
                    source_name: function.name.clone(),
                    parameters: function
                        .params
                        .iter()
                        .map(|parameter| l::ForeignParameter {
                            source_name: parameter.name.clone(),
                            ty: parameter.ty.clone(),
                            foreign_provenance: parameter
                                .foreign_provenance
                                .as_ref()
                                .map(convert_provenance),
                            pos: parameter.pos.clone(),
                        })
                        .collect(),
                    return_type: function.ret.clone(),
                    include,
                    pos: function.pos.clone(),
                })
            })
            .collect()
    }

    fn method_record(
        &self,
        class: usize,
        name: &str,
        pos: &Pos,
    ) -> Result<FunctionRecord, LowerError> {
        self.methods
            .get(&(class, name.to_string()))
            .cloned()
            .ok_or_else(|| LowerError {
                pos: pos.clone(),
                message: format!("missing method id for class #{class} `{name}`"),
            })
    }

    fn allocate_function_id(&mut self) -> l::FunctionId {
        let id = l::FunctionId(self.next_function);
        self.next_function += 1;
        self.functions.push(None);
        id
    }

    fn set_function(&mut self, id: l::FunctionId, function: l::Function) -> Result<(), LowerError> {
        let slot = self
            .functions
            .get_mut(id.0 as usize)
            .ok_or_else(|| LowerError {
                pos: function.pos.clone(),
                message: format!("function id {} is outside the module table", id.0),
            })?;
        if slot.is_some() {
            return Err(LowerError {
                pos: function.pos.clone(),
                message: format!("function id {} has two bodies", id.0),
            });
        }
        *slot = Some(function);
        Ok(())
    }

    fn lower_function(
        &mut self,
        id: l::FunctionId,
        function: hir::Function,
        kind: l::FunctionKind,
        receiver: Option<ClassId>,
        captures: Vec<hir::Capture>,
    ) -> Result<(), LowerError> {
        let host_entry_traps = (kind == l::FunctionKind::Free)
            .then(|| function.host_entry_trap_sites(self.hir))
            .flatten();
        let mut input = FunctionInput::from(function);
        input.host_entry_traps = host_entry_traps;
        self.lower_function_input(id, input, kind, receiver, captures)
    }

    fn lower_function_input(
        &mut self,
        id: l::FunctionId,
        function: FunctionInput,
        kind: l::FunctionKind,
        receiver: Option<ClassId>,
        captures: Vec<hir::Capture>,
    ) -> Result<(), LowerError> {
        let mut builder = FunctionBuilder::new(self, id, function, kind, receiver, captures)?;
        builder.lower_statements(&builder.function.body.clone())?;
        let lowered = builder.finish()?;
        self.set_function(id, lowered)
    }
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
            let method = match method {
                hir::BuiltinMethod::ArrayPush => l::BuiltinMethod::ArrayPush,
                hir::BuiltinMethod::ArrayPop => l::BuiltinMethod::ArrayPop,
                hir::BuiltinMethod::StringSlice => l::BuiltinMethod::StringSlice,
                hir::BuiltinMethod::GeneratorNext => l::BuiltinMethod::GeneratorNext,
            };
            l::CallSignatureTarget::BuiltinMethod(method)
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

struct AddressTaken<'a> {
    module: &'a hir::Module,
    classes: &'a [l::Class],
    scopes: Vec<HashMap<String, BindingSite>>,
    taken: HashSet<BindingSite>,
}

fn address_taken_bindings(
    module: &hir::Module,
    classes: &[l::Class],
    function: &FunctionInput,
    captures: &[hir::Capture],
) -> HashSet<BindingSite> {
    let mut analysis = AddressTaken {
        module,
        classes,
        scopes: vec![HashMap::new()],
        taken: HashSet::new(),
    };
    for capture in captures {
        analysis.declare(&capture.name, &function.pos);
    }
    for parameter in &function.params {
        analysis.declare(&parameter.name, &parameter.pos);
    }
    analysis.statements(&function.body);
    analysis.taken
}

impl AddressTaken<'_> {
    fn declare(&mut self, name: &str, pos: &Pos) {
        self.scopes
            .last_mut()
            .expect("one address-analysis scope")
            .insert(name.to_string(), BindingSite::new(name, pos));
    }

    fn mark(&mut self, name: &str) {
        if let Some(site) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
        {
            self.taken.insert(site);
        }
    }

    fn scoped(&mut self, statements: &[hir::Stmt]) {
        self.scopes.push(HashMap::new());
        self.statements(statements);
        self.scopes.pop();
    }

    fn statements(&mut self, statements: &[hir::Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &hir::Stmt) {
        match statement {
            hir::Stmt::Let {
                name, init, pos, ..
            } => {
                self.expr(init);
                self.declare(name, pos);
            }
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                self.expr(cond);
                self.scoped(then);
                if let Some(els) = els {
                    self.scoped(els);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.scoped(body);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                self.scopes.push(HashMap::new());
                if let Some(init) = init {
                    self.statement(init);
                }
                if let Some(cond) = cond {
                    self.expr(cond);
                }
                self.scoped(body);
                if let Some(step) = step {
                    self.expr(step);
                }
                self.scopes.pop();
            }
            hir::Stmt::ForOf {
                name,
                subject,
                body,
                pos,
                ..
            } => {
                self.expr(subject);
                self.scopes.push(HashMap::new());
                self.declare(name, pos);
                self.scoped(body);
                self.scopes.pop();
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                self.expr(disc);
                for case in cases {
                    if let Some(test) = &case.test {
                        self.expr(test);
                    }
                    self.scoped(&case.body);
                }
            }
            hir::Stmt::Block(statements) => self.scoped(statements),
            _ => {
                for child in statement.children() {
                    match child {
                        hir::HirChild::Expr(expr) => self.expr(expr),
                        hir::HirChild::Stmt(statement) => self.statement(statement),
                    }
                }
            }
        }
    }

    fn expr(&mut self, expr: &hir::Expr) {
        use hir::ExprKind as K;
        match &expr.kind {
            K::Assign { target, value, .. } => {
                match target.kind {
                    K::Local(_) | K::Global(_) => {}
                    _ => self.place(target),
                }
                self.expr(value);
                return;
            }
            K::Call { callee, args } => {
                match callee {
                    hir::Callee::Value(value) => self.expr(value),
                    hir::Callee::Method { recv, .. } if is_value_class(self.module, &recv.ty) => {
                        self.place(recv);
                    }
                    hir::Callee::Method { recv, .. } => self.expr(recv),
                    _ => {}
                }
                let parameter_types = match callee {
                    hir::Callee::Func(name) => self
                        .module
                        .functions
                        .iter()
                        .find(|function| function.name == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    hir::Callee::Foreign(name) => self
                        .module
                        .foreign_fns
                        .iter()
                        .find(|function| function.name == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    hir::Callee::Value(value) => match &value.ty {
                        Type::Func(signature) => Some(signature.params.clone()),
                        _ => None,
                    },
                    hir::Callee::Method { recv, name } => match recv.ty {
                        Type::Class(class) => self
                            .module
                            .classes
                            .get(class.0)
                            .and_then(|definition| {
                                definition
                                    .methods
                                    .iter()
                                    .find(|method| method.name == *name)
                            })
                            .map(|method| {
                                method
                                    .params
                                    .iter()
                                    .map(|parameter| parameter.ty.clone())
                                    .collect::<Vec<_>>()
                            }),
                        _ => None,
                    },
                    _ => None,
                };
                for (index, argument) in args.iter().enumerate() {
                    if matches!(callee, hir::Callee::Foreign(_))
                        && parameter_types
                            .as_ref()
                            .and_then(|params| params.get(index))
                            .is_some_and(|parameter| {
                                self.is_boundary_struct_pointer(parameter)
                                    && matches!(argument.ty, Type::Class(_))
                                    && is_place_expr(argument)
                                    && !self.is_embedded_header_store(parameter, argument)
                            })
                    {
                        self.place(argument);
                    } else {
                        self.expr(argument);
                    }
                }
                return;
            }
            K::Index { .. } => {
                self.place(expr);
                return;
            }
            K::Lambda { .. } => return,
            _ => {}
        }
        for child in expr.children() {
            match child {
                hir::HirChild::Expr(expr) => self.expr(expr),
                hir::HirChild::Stmt(statement) => self.statement(statement),
            }
        }
    }

    fn place(&mut self, expr: &hir::Expr) {
        match &expr.kind {
            hir::ExprKind::Local(name) => self.mark(name),
            hir::ExprKind::Field { obj, .. } => {
                if is_stored_aggregate(self.module, &obj.ty) && is_place_expr(obj) {
                    self.place(obj);
                } else {
                    self.expr(obj);
                }
            }
            hir::ExprKind::Index { obj, index, .. } => {
                if matches!(obj.ty, Type::FixedArray(..)) && is_place_expr(obj) {
                    self.place(obj);
                } else {
                    self.expr(obj);
                }
                self.expr(index);
            }
            hir::ExprKind::Global(_) | hir::ExprKind::This => {}
            _ => self.expr(expr),
        }
    }

    fn is_boundary_struct_pointer(&self, ty: &Type) -> bool {
        boundary_box_class(self.module, ty).is_some()
    }

    fn is_embedded_header_store(&self, expected: &Type, expr: &hir::Expr) -> bool {
        let Some(header) = boundary_box_class(self.module, expected) else {
            return false;
        };
        let hir::ExprKind::Field { obj, name } = &expr.kind else {
            return false;
        };
        let Type::Class(extension) = obj.ty else {
            return false;
        };
        self.classes
            .get(extension.0)
            .filter(|class| class.is_value && class.is_boundary)
            .and_then(|class| class.fields.first())
            .is_some_and(|field| {
                field.source_name == *name
                    && field.ty == Type::Class(header)
                    && self
                        .classes
                        .get(header.0)
                        .is_some_and(|header| header.is_embedded_header)
            })
    }
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

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    fn new(
        lowering: &'a mut Lowering<'m>,
        id: l::FunctionId,
        function: FunctionInput,
        kind: l::FunctionKind,
        receiver: Option<ClassId>,
        captures: Vec<hir::Capture>,
    ) -> Result<Self, LowerError> {
        let address_taken =
            address_taken_bindings(lowering.hir, &lowering.classes, &function, &captures);
        let mut builder = Self {
            lowering,
            function,
            id,
            kind,
            parameters: Vec::new(),
            locals: Vec::new(),
            values: Vec::new(),
            blocks: Vec::new(),
            entry: l::BlockId(0),
            current: None,
            scopes: vec![HashMap::new()],
            bindings: Vec::new(),
            address_taken,
            substitutions: Vec::new(),
            this_value: None,
            controls: Vec::new(),
            array_values: Vec::new(),
            moved_async_owners: HashSet::new(),
        };
        let entry = builder.new_block(Vec::new(), Some("entry".to_string()));
        builder.entry = entry;
        builder.current = Some(entry);

        if let Some(class_id) = receiver {
            let class = builder
                .lowering
                .hir
                .classes
                .get(class_id.0)
                .ok_or_else(|| builder.error(&builder.function.pos, "receiver class is missing"))?;
            let ty = if class.is_value {
                l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(class_id),
                    array_base: None,
                })
            } else {
                l::ValueType::Data(Type::Class(class_id))
            };
            let operand = builder.add_parameter(
                "this".to_string(),
                ty,
                l::ParameterKind::Receiver,
                builder.function.pos.clone(),
            )?;
            builder.this_value = Some(operand);
        }
        for capture in captures {
            builder.add_parameter(
                capture.name,
                l::ValueType::Data(capture.ty),
                l::ParameterKind::Capture,
                builder.function.pos.clone(),
            )?;
        }
        for parameter in builder.function.params.clone() {
            builder.add_parameter(
                parameter.name,
                l::ValueType::Data(parameter.ty),
                l::ParameterKind::Explicit,
                parameter.pos,
            )?;
        }
        Ok(builder)
    }

    fn finish(mut self) -> Result<l::Function, LowerError> {
        if let Some(block) = self.current {
            if self.blocks[block.0 as usize].terminator.is_none() {
                if self.function.ret == Type::Void || self.function.is_generator {
                    let pos = self.function.pos.clone();
                    self.release_scopes_from(0, &pos)?;
                    self.blocks[block.0 as usize].terminator = Some(l::Terminator::Return {
                        value: None,
                        pos: self.function.pos.clone(),
                    });
                } else {
                    return Err(self.error(
                        &self.function.pos,
                        "non-void function has a reachable fallthrough",
                    ));
                }
            }
        }
        for block in &mut self.blocks {
            if block.terminator.is_none() {
                block.terminator = Some(l::Terminator::Unreachable {
                    pos: self.function.pos.clone(),
                });
            }
        }
        let function = l::Function {
            id: self.id,
            source_name: self.function.name,
            kind: self.kind,
            exported: self.function.exported,
            is_generator: self.function.is_generator,
            is_async: self.function.is_async,
            creation_traps: convert_traps(&self.function.creation_traps),
            host_entry_traps: self.function.host_entry_traps.as_deref().map(convert_traps),
            parameters: self.parameters,
            return_type: self.function.ret,
            locals: self.locals,
            values: self.values,
            liveness: l::Liveness::default(),
            blocks: self
                .blocks
                .into_iter()
                .map(|block| l::BasicBlock {
                    id: block.id,
                    source_name: block.source_name,
                    parameters: block.parameters,
                    instructions: block.instructions,
                    terminator: block.terminator.expect("terminator filled"),
                })
                .collect(),
            entry: self.entry,
            pos: self.function.pos,
        };
        Ok(function)
    }

    fn error(&self, pos: &Pos, message: impl Into<String>) -> LowerError {
        LowerError {
            pos: pos.clone(),
            message: message.into(),
        }
    }

    fn new_block(
        &mut self,
        parameter_types: Vec<l::ValueType>,
        source_name: Option<String>,
    ) -> l::BlockId {
        let id = l::BlockId(self.blocks.len() as u32);
        let parameters = parameter_types
            .into_iter()
            .map(|ty| self.new_value(ty, None))
            .collect();
        self.blocks.push(BlockDraft {
            id,
            source_name,
            parameters,
            state_bindings: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    fn new_state_block(
        &mut self,
        prefix_types: Vec<l::ValueType>,
        source_name: Option<String>,
        forced: &[BindingId],
    ) -> l::BlockId {
        let mut state_bindings = self.visible_mutable_bindings();
        state_bindings.extend(forced.iter().copied());
        state_bindings.sort_unstable();
        state_bindings.dedup();
        state_bindings.retain(|binding| {
            self.bindings
                .get(binding.0)
                .is_some_and(|binding| binding.storage.is_none())
        });
        let mut parameter_types = prefix_types;
        parameter_types.extend(
            state_bindings
                .iter()
                .map(|binding| self.bindings[binding.0].ty.clone()),
        );
        let block = self.new_block(parameter_types, source_name);
        self.blocks[block.0 as usize].state_bindings = state_bindings;
        block
    }

    fn new_value(&mut self, ty: l::ValueType, source_name: Option<String>) -> l::ValueId {
        let id = l::ValueId(self.values.len() as u32);
        if matches!(&ty, l::ValueType::Data(Type::Array(_))) {
            self.array_values.push(id);
        }
        self.values.push(l::Value {
            id,
            ty,
            fresh_owner: false,
            source_name,
        });
        id
    }

    fn add_local(
        &mut self,
        source_name: String,
        ty: l::ValueType,
        mutable: bool,
        pos: Pos,
    ) -> Result<l::LocalId, LowerError> {
        let id = l::LocalId(self.locals.len() as u32);
        self.locals.push(l::Local {
            id,
            source_name: source_name.clone(),
            ty,
            mutable,
            storage: l::LocalStorageClass::Activation,
            pos: pos.clone(),
        });
        Ok(id)
    }

    fn declare_binding(
        &mut self,
        source_name: String,
        ty: l::ValueType,
        mutable: bool,
        value: l::Operand,
        pos: Pos,
        copy_site: Option<hir::AsyncCopySite>,
    ) -> Result<(BindingId, Option<l::LocalId>), LowerError> {
        let storage = if self
            .address_taken
            .contains(&BindingSite::new(&source_name, &pos))
        {
            Some(self.add_local(source_name.clone(), ty.clone(), mutable, pos.clone())?)
        } else {
            None
        };
        if let Some(local) = storage {
            self.emit_store_instruction(
                l::InstructionKind::StoreLocal(local),
                vec![value.clone()],
                vec![StoredOperand {
                    index: 0,
                    ty: ty.clone(),
                    action: copy_site.map_or(OwnerStoreAction::Move, OwnerStoreAction::Acquire),
                    pos: pos.clone(),
                }],
                (None, false),
                Vec::new(),
                pos.clone(),
            )?;
        } else if let Some(copy_site) = copy_site {
            self.acquire_owner(copy_site, &value, &ty, &pos)?;
        }
        let id = BindingId(self.bindings.len());
        self.bindings.push(Binding {
            source_name: source_name.clone(),
            ty,
            mutable,
            storage,
            value: storage.is_none().then_some(value),
        });
        let scope = self.scopes.last_mut().expect("one binding scope");
        if scope.insert(source_name.clone(), id).is_some() {
            return Err(self.error(
                &pos,
                format!("duplicate local `{source_name}` in one scope"),
            ));
        }
        Ok((id, storage))
    }

    fn declare_hidden_binding(
        &mut self,
        source_name: &str,
        ty: l::ValueType,
        value: l::Operand,
    ) -> BindingId {
        let id = BindingId(self.bindings.len());
        self.bindings.push(Binding {
            source_name: source_name.to_string(),
            ty,
            mutable: true,
            storage: None,
            value: Some(value),
        });
        id
    }

    fn add_parameter(
        &mut self,
        source_name: String,
        ty: l::ValueType,
        kind: l::ParameterKind,
        pos: Pos,
    ) -> Result<l::Operand, LowerError> {
        let value = self.new_value(ty.clone(), Some(source_name.clone()));
        let operand = l::Operand::Value(value);
        let (_, storage) = self.declare_binding(
            source_name.clone(),
            ty.clone(),
            true,
            operand.clone(),
            pos.clone(),
            None,
        )?;
        self.parameters.push(l::Parameter {
            storage,
            value,
            source_name,
            kind,
            pos,
        });
        Ok(operand)
    }

    fn emit(
        &mut self,
        kind: l::InstructionKind,
        operands: Vec<l::Operand>,
        result_type: Option<l::ValueType>,
        invalidates_arrays: bool,
        traps: Vec<l::Trap>,
        pos: Pos,
    ) -> Result<Option<l::Operand>, LowerError> {
        let block = self.current.ok_or_else(|| {
            self.error(&pos, "attempted to emit an instruction after a terminator")
        })?;
        let result = result_type
            .as_ref()
            .map(|ty| self.new_value(ty.clone(), None));
        if kind.produces_fresh_async_owner()
            && result_type.as_ref().is_some_and(is_async_owner_type)
        {
            if let Some(value) = result {
                self.values[value.0 as usize].fresh_owner = true;
            }
        }
        let invalidates = if invalidates_arrays {
            self.array_values.clone()
        } else {
            Vec::new()
        };
        self.blocks[block.0 as usize]
            .instructions
            .push(l::Instruction {
                result,
                kind,
                operands,
                invalidates,
                traps,
                pos,
            });
        Ok(result.map(l::Operand::Value))
    }

    fn emit_store_instruction(
        &mut self,
        kind: l::InstructionKind,
        operands: Vec<l::Operand>,
        stored: Vec<StoredOperand>,
        result: (Option<l::ValueType>, bool),
        traps: Vec<l::Trap>,
        pos: Pos,
    ) -> Result<Option<l::Operand>, LowerError> {
        self.acquire_stored_operands(&operands, stored)?;
        let (result_type, invalidates_arrays) = result;
        self.emit(kind, operands, result_type, invalidates_arrays, traps, pos)
    }

    fn acquire_stored_operands(
        &mut self,
        operands: &[l::Operand],
        stored: Vec<StoredOperand>,
    ) -> Result<(), LowerError> {
        for store in stored {
            let value = operands.get(store.index).ok_or_else(|| {
                self.error(
                    &store.pos,
                    format!("store operand {} is missing", store.index),
                )
            })?;
            match store.action {
                OwnerStoreAction::Acquire(site) => {
                    self.acquire_owner(site, value, &store.ty, &store.pos)?;
                }
                OwnerStoreAction::Move => {}
            }
        }
        Ok(())
    }

    fn terminate(&mut self, terminator: l::Terminator, pos: &Pos) -> Result<(), LowerError> {
        let block = self
            .current
            .take()
            .ok_or_else(|| self.error(pos, "block already has a terminator"))?;
        let draft = &mut self.blocks[block.0 as usize];
        if draft.terminator.replace(terminator).is_some() {
            return Err(self.error(pos, format!("block {} has two terminators", block.0)));
        }
        Ok(())
    }

    fn terminate_return(
        &mut self,
        value: Option<l::Operand>,
        ty: l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        if let Some(value) = &value {
            self.acquire_owner(hir::AsyncCopySite::Return, value, &ty, pos)?;
        }
        self.release_scopes_from(0, pos)?;
        self.terminate(
            l::Terminator::Return {
                value,
                pos: pos.clone(),
            },
            pos,
        )
    }

    fn operand_type(&self, operand: &l::Operand, pos: &Pos) -> Result<l::ValueType, LowerError> {
        match operand {
            l::Operand::Value(id) => self
                .values
                .get(id.0 as usize)
                .map(|value| value.ty.clone())
                .ok_or_else(|| self.error(pos, format!("value {} is not declared", id.0))),
            l::Operand::Constant(constant) => Ok(l::ValueType::Data(constant.ty.clone())),
        }
    }

    fn acquire_owner(
        &mut self,
        site: hir::AsyncCopySite,
        value: &l::Operand,
        ty: &l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        match site {
            hir::AsyncCopySite::Binding
            | hir::AsyncCopySite::Assignment
            | hir::AsyncCopySite::ArrayElement
            | hir::AsyncCopySite::SpreadElement
            | hir::AsyncCopySite::CallArgument
            | hir::AsyncCopySite::Return
            | hir::AsyncCopySite::ForOfBinding => {}
            hir::AsyncCopySite::ConditionalResult | hir::AsyncCopySite::DiscardedResult => {
                return Err(self.error(pos, "the copy site does not acquire an owner"));
            }
        }
        if matches!(value, l::Operand::Value(value)
            if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner))
        {
            if let l::Operand::Value(value) = value {
                if self.moved_async_owners.insert(*value) {
                    return Ok(());
                }
            }
        }
        let kind = match ty {
            l::ValueType::Data(Type::AsyncHandle(_)) => l::InstructionKind::AsyncHandleRetain,
            l::ValueType::Data(Type::Array(element))
                if matches!(&**element, Type::AsyncHandle(_)) =>
            {
                l::InstructionKind::AsyncHandleArrayRetain
            }
            _ => return Ok(()),
        };
        self.emit(
            kind,
            vec![value.clone()],
            None,
            false,
            Vec::new(),
            pos.clone(),
        )?;
        Ok(())
    }

    fn release_owner(
        &mut self,
        value: l::Operand,
        ty: &l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let kind = match ty {
            l::ValueType::Data(Type::AsyncHandle(_)) => l::InstructionKind::AsyncHandleRelease,
            l::ValueType::Data(Type::Array(element))
                if matches!(&**element, Type::AsyncHandle(_)) =>
            {
                l::InstructionKind::AsyncHandleArrayRelease
            }
            _ => return Ok(()),
        };
        self.emit(kind, vec![value], None, false, Vec::new(), pos.clone())?;
        Ok(())
    }

    fn discard_owner(
        &mut self,
        site: hir::AsyncCopySite,
        value: l::Operand,
        ty: &l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        match site {
            hir::AsyncCopySite::DiscardedResult => self.release_owner(value, ty, pos),
            hir::AsyncCopySite::Binding
            | hir::AsyncCopySite::Assignment
            | hir::AsyncCopySite::ArrayElement
            | hir::AsyncCopySite::SpreadElement
            | hir::AsyncCopySite::CallArgument
            | hir::AsyncCopySite::Return
            | hir::AsyncCopySite::ForOfBinding
            | hir::AsyncCopySite::ConditionalResult => {
                Err(self.error(pos, "the copy site does not discard an owner"))
            }
        }
    }

    fn release_scopes_from(&mut self, depth: usize, pos: &Pos) -> Result<(), LowerError> {
        if self.current.is_none() {
            return Ok(());
        }
        let mut bindings = self
            .scopes
            .iter()
            .skip(depth)
            .flat_map(|scope| scope.values().copied())
            .collect::<Vec<_>>();
        bindings.sort_unstable();
        bindings.dedup();
        bindings.reverse();
        for binding in bindings {
            let entry = self.bindings[binding.0].clone();
            if is_async_owner_type(&entry.ty) {
                let value = self.read_binding(binding, pos)?;
                self.release_owner(value, &entry.ty, pos)?;
            }
        }
        Ok(())
    }

    fn lookup_binding(&self, name: &str, pos: &Pos) -> Result<BindingId, LowerError> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .ok_or_else(|| self.error(pos, format!("unknown local `{name}`")))
    }

    fn read_binding(&mut self, binding: BindingId, pos: &Pos) -> Result<l::Operand, LowerError> {
        let entry = self
            .bindings
            .get(binding.0)
            .cloned()
            .ok_or_else(|| self.error(pos, format!("binding {} is missing", binding.0)))?;
        if let Some(local) = entry.storage {
            self.load_local(local, pos)
        } else {
            entry.value.ok_or_else(|| {
                self.error(
                    pos,
                    format!("binding `{}` has no current SSA value", entry.source_name),
                )
            })
        }
    }

    fn write_binding(
        &mut self,
        binding: BindingId,
        value: l::Operand,
        pos: &Pos,
        traps: Vec<l::Trap>,
    ) -> Result<(), LowerError> {
        let entry = self
            .bindings
            .get(binding.0)
            .cloned()
            .ok_or_else(|| self.error(pos, format!("binding {} is missing", binding.0)))?;
        let value = self.coerce_operand(value, entry.ty.clone(), pos)?;
        let old_owner = if is_async_owner_type(&entry.ty) {
            Some(self.read_binding(binding, pos)?)
        } else {
            None
        };
        if let Some(local) = entry.storage {
            self.emit_store_instruction(
                l::InstructionKind::StoreLocal(local),
                vec![value],
                vec![StoredOperand {
                    index: 0,
                    ty: entry.ty.clone(),
                    action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                    pos: pos.clone(),
                }],
                (None, false),
                traps,
                pos.clone(),
            )?;
            if let Some(old_owner) = old_owner {
                self.release_owner(old_owner, &entry.ty, pos)?;
            }
        } else {
            self.acquire_owner(hir::AsyncCopySite::Assignment, &value, &entry.ty, pos)?;
            if let Some(old_owner) = old_owner {
                self.release_owner(old_owner, &entry.ty, pos)?;
            }
            self.bindings[binding.0].value = Some(value);
        }
        Ok(())
    }

    fn binding_snapshot(&self) -> Vec<Option<l::Operand>> {
        self.bindings
            .iter()
            .map(|binding| binding.value.clone())
            .collect()
    }

    fn restore_bindings(&mut self, snapshot: &[Option<l::Operand>]) {
        for (binding, value) in self.bindings.iter_mut().zip(snapshot) {
            if binding.storage.is_none() {
                binding.value = value.clone();
            }
        }
    }

    fn visible_mutable_bindings(&self) -> Vec<BindingId> {
        let mut visible = BTreeSet::new();
        for scope in &self.scopes {
            for binding in scope.values() {
                if self.bindings[binding.0].mutable && self.bindings[binding.0].storage.is_none() {
                    visible.insert(*binding);
                }
            }
        }
        visible.into_iter().collect()
    }

    fn block_target(
        &self,
        block: l::BlockId,
        mut prefix: Vec<l::Operand>,
    ) -> Result<l::BlockTarget, LowerError> {
        let draft = self.blocks.get(block.0 as usize).ok_or_else(|| {
            self.error(
                &self.function.pos,
                format!("branch target block {} is missing", block.0),
            )
        })?;
        for binding in &draft.state_bindings {
            let entry = &self.bindings[binding.0];
            let value = entry.value.clone().ok_or_else(|| {
                self.error(
                    &self.function.pos,
                    format!(
                        "binding `{}` has no value for block {}",
                        entry.source_name, block.0
                    ),
                )
            })?;
            prefix.push(value);
        }
        Ok(target(block, prefix))
    }

    fn enter_block(&mut self, block: l::BlockId) -> Result<(), LowerError> {
        let draft = self.blocks.get(block.0 as usize).ok_or_else(|| {
            self.error(
                &self.function.pos,
                format!("entered block {} is missing", block.0),
            )
        })?;
        let state_start = draft.parameters.len() - draft.state_bindings.len();
        let updates = draft
            .state_bindings
            .iter()
            .copied()
            .zip(draft.parameters[state_start..].iter().copied())
            .collect::<Vec<_>>();
        for (binding, value) in updates {
            self.bindings[binding.0].value = Some(l::Operand::Value(value));
        }
        self.current = Some(block);
        Ok(())
    }

    fn require_expr(&mut self, expr: &hir::Expr) -> Result<l::Operand, LowerError> {
        self.lower_expr(expr)?.ok_or_else(|| {
            self.error(
                &expr.pos,
                format!("void expression used where `{}` is required", expr.ty),
            )
        })
    }

    fn lower_statements(&mut self, statements: &[hir::Stmt]) -> Result<(), LowerError> {
        for statement in statements {
            if self.current.is_none() {
                break;
            }
            self.lower_statement(statement)?;
        }
        Ok(())
    }

    fn lower_scoped(&mut self, statements: &[hir::Stmt]) -> Result<(), LowerError> {
        self.scopes.push(HashMap::new());
        let result = self.lower_statements(statements);
        if result.is_ok() && self.current.is_some() {
            let pos = statements
                .last()
                .map(stmt_pos)
                .unwrap_or_else(|| self.function.pos.clone());
            self.release_scopes_from(self.scopes.len() - 1, &pos)?;
        }
        self.scopes.pop();
        result
    }

    fn lower_statement(&mut self, statement: &hir::Stmt) -> Result<(), LowerError> {
        match statement {
            hir::Stmt::Let {
                name,
                ty,
                mutable,
                dispose: _,
                init,
                pos,
            } => {
                let value = self.lower_stored_expr_at(ty, init, pos)?;
                self.declare_binding(
                    name.clone(),
                    l::ValueType::Data(ty.clone()),
                    *mutable,
                    value,
                    pos.clone(),
                    Some(hir::AsyncCopySite::Binding),
                )?;
            }
            hir::Stmt::Expr(expr) => {
                let result = self.lower_expr(expr)?;
                if expr.kind.produces_fresh_async_owner()
                    && is_async_owner_type(&l::ValueType::Data(expr.ty.clone()))
                {
                    if let Some(value) = result {
                        self.discard_owner(
                            hir::AsyncCopySite::DiscardedResult,
                            value,
                            &l::ValueType::Data(expr.ty.clone()),
                            &expr.pos,
                        )?;
                    }
                }
            }
            hir::Stmt::Return { value, pos } => {
                let value = value
                    .as_ref()
                    .map(|value| self.lower_stored_expr_at(&self.function.ret.clone(), value, pos))
                    .transpose()?;
                self.terminate_return(value, l::ValueType::Data(self.function.ret.clone()), pos)?;
            }
            hir::Stmt::If {
                cond,
                then,
                els,
                pos,
            } => self.lower_if(cond, then, els.as_deref().unwrap_or(&[]), pos)?,
            hir::Stmt::While {
                cond, body, pos, ..
            } => self.lower_while(cond, body, pos)?,
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                pos,
            } => self.lower_for(init.as_deref(), cond.as_ref(), step.as_ref(), body, pos)?,
            hir::Stmt::ForOf {
                name,
                ty,
                subject,
                kind,
                body,
                pos,
            } => self.lower_for_of(name, ty, subject, *kind, body, pos)?,
            hir::Stmt::Switch { disc, cases, pos } => {
                self.lower_switch(disc, cases, pos)?;
            }
            hir::Stmt::Break(pos) => {
                let block = self
                    .controls
                    .last()
                    .map(|control| (control.break_target, control.scope_depth))
                    .ok_or_else(|| self.error(pos, "break has no enclosing target"))?;
                self.release_scopes_from(block.1, pos)?;
                let edge = self.block_target(block.0, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
            hir::Stmt::Continue(pos) => {
                let control = self
                    .controls
                    .iter()
                    .rev()
                    .find_map(|control| {
                        control
                            .continue_target
                            .map(|target| (target, control.scope_depth))
                    })
                    .ok_or_else(|| self.error(pos, "continue has no enclosing loop"))?;
                self.release_scopes_from(control.1, pos)?;
                let edge = self.block_target(control.0, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
            hir::Stmt::Block(statements) => self.lower_scoped(statements)?,
        }
        Ok(())
    }

    fn lower_if(
        &mut self,
        cond: &hir::Expr,
        then: &[hir::Stmt],
        els: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let condition = self.require_expr(cond)?;
        let branch_state = self.binding_snapshot();
        let then_block = self.new_block(Vec::new(), Some("if.then".to_string()));
        let else_block = self.new_block(Vec::new(), Some("if.else".to_string()));
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition,
                then_target: target(then_block, Vec::new()),
                else_target: target(else_block, Vec::new()),
            },
            pos,
        )?;
        self.current = Some(then_block);
        self.lower_scoped(then)?;
        let then_end = self.current;
        let then_state = self.binding_snapshot();
        self.restore_bindings(&branch_state);
        self.current = Some(else_block);
        self.lower_scoped(els)?;
        let else_end = self.current;
        let else_state = self.binding_snapshot();
        if then_end.is_none() && else_end.is_none() {
            self.current = None;
            return Ok(());
        }
        let join = self.new_state_block(Vec::new(), Some("if.join".to_string()), &[]);
        for (end, state) in [(then_end, then_state), (else_end, else_state)] {
            let Some(end) = end else { continue };
            self.restore_bindings(&state);
            self.current = Some(end);
            let edge = self.block_target(join, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        self.enter_block(join)?;
        Ok(())
    }

    fn lower_while(
        &mut self,
        cond: &hir::Expr,
        body: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let header = self.new_state_block(Vec::new(), Some("while.cond".to_string()), &[]);
        let body_block = self.new_block(Vec::new(), Some("while.body".to_string()));
        let exit = self.new_state_block(Vec::new(), Some("while.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)?;
        self.enter_block(header)?;
        let condition = self.require_expr(cond)?;
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition,
                then_target: target(body_block, Vec::new()),
                else_target: exit_target,
            },
            pos,
        )?;
        self.controls.push(Control {
            break_target: exit,
            continue_target: Some(header),
            scope_depth: self.scopes.len(),
        });
        self.current = Some(body_block);
        self.lower_scoped(body)?;
        if self.current.is_some() {
            let edge = self.block_target(header, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        self.controls.pop();
        self.enter_block(exit)?;
        Ok(())
    }

    fn lower_for(
        &mut self,
        init: Option<&hir::Stmt>,
        cond: Option<&hir::Expr>,
        step: Option<&hir::Expr>,
        body: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        self.scopes.push(HashMap::new());
        if let Some(init) = init {
            self.lower_statement(init)?;
        }
        let header = self.new_state_block(Vec::new(), Some("for.cond".to_string()), &[]);
        let body_block = self.new_block(Vec::new(), Some("for.body".to_string()));
        let step_block = self.new_state_block(Vec::new(), Some("for.step".to_string()), &[]);
        let exit = self.new_state_block(Vec::new(), Some("for.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)?;
        self.enter_block(header)?;
        if let Some(cond) = cond {
            let condition = self.require_expr(cond)?;
            let exit_target = self.block_target(exit, Vec::new())?;
            self.terminate(
                l::Terminator::ConditionalBranch {
                    condition,
                    then_target: target(body_block, Vec::new()),
                    else_target: exit_target,
                },
                pos,
            )?;
        } else {
            self.terminate(branch(body_block), pos)?;
        }
        self.controls.push(Control {
            break_target: exit,
            continue_target: Some(step_block),
            scope_depth: self.scopes.len(),
        });
        self.current = Some(body_block);
        self.lower_scoped(body)?;
        if self.current.is_some() {
            let edge = self.block_target(step_block, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        let step_reachable = self.blocks.iter().any(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| successors(terminator).contains(&step_block))
        });
        if step_reachable {
            self.enter_block(step_block)?;
            if let Some(step) = step {
                self.lower_expr(step)?;
            }
            if self.current.is_some() {
                let edge = self.block_target(header, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
        } else {
            self.current = None;
        }
        self.controls.pop();
        self.enter_block(exit)?;
        self.release_scopes_from(self.scopes.len() - 1, pos)?;
        self.scopes.pop();
        Ok(())
    }

    fn lower_for_of(
        &mut self,
        name: &str,
        ty: &Type,
        subject: &hir::Expr,
        kind: hir::ForOfKind,
        body: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let subject_value = self.require_expr(subject)?;
        let kind = convert_for_of(kind);
        let iterator_type = l::ValueType::Iterator(l::IteratorType {
            kind,
            element: ty.clone(),
        });
        let iterator = self
            .emit(
                l::InstructionKind::IteratorCreate {
                    kind,
                    bound: l::IteratorBoundKind::Live,
                },
                vec![subject_value],
                Some(iterator_type.clone()),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator result");
        let bound = self
            .emit(
                l::InstructionKind::IteratorBound,
                vec![iterator.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator bound");
        let index = l::Operand::Constant(l::Constant {
            ty: Type::I32,
            kind: l::ConstantKind::Integer(0),
        });
        self.scopes.push(HashMap::new());
        let cursor_binding =
            self.declare_hidden_binding("<for-of cursor>", iterator_type.clone(), iterator);
        let index_binding =
            self.declare_hidden_binding("<for-of index>", l::ValueType::Data(Type::I32), index);
        let bound_binding =
            self.declare_hidden_binding("<for-of bound>", l::ValueType::Data(Type::I32), bound);
        let traversal = [cursor_binding, index_binding, bound_binding];
        let header = self.new_state_block(Vec::new(), Some("for-of.cond".to_string()), &traversal);
        let body_block = self.new_block(Vec::new(), Some("for-of.body".to_string()));
        let step_block =
            self.new_state_block(Vec::new(), Some("for-of.step".to_string()), &traversal);
        let exit = self.new_state_block(Vec::new(), Some("for-of.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)?;
        self.enter_block(header)?;
        let cursor = self.read_binding(cursor_binding, pos)?;
        let index = self.read_binding(index_binding, pos)?;
        let bound = self.read_binding(bound_binding, pos)?;
        let more = self
            .emit(
                l::InstructionKind::IteratorHasNext,
                vec![cursor, index, bound],
                Some(l::ValueType::Data(Type::Bool)),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator condition");
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: more,
                then_target: target(body_block, Vec::new()),
                else_target: exit_target,
            },
            pos,
        )?;
        self.controls.push(Control {
            break_target: exit,
            continue_target: Some(step_block),
            scope_depth: self.scopes.len(),
        });
        self.current = Some(body_block);
        let cursor = self.read_binding(cursor_binding, pos)?;
        let index = self.read_binding(index_binding, pos)?;
        let bound = self.read_binding(bound_binding, pos)?;
        let value = self
            .emit(
                l::InstructionKind::IteratorValue,
                vec![cursor, index, bound],
                Some(l::ValueType::Data(ty.clone())),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator value");
        self.scopes.push(HashMap::new());
        self.declare_binding(
            name.to_string(),
            l::ValueType::Data(ty.clone()),
            true,
            value,
            pos.clone(),
            Some(hir::AsyncCopySite::ForOfBinding),
        )?;
        self.lower_scoped(body)?;
        if self.current.is_some() {
            self.release_scopes_from(self.scopes.len() - 1, pos)?;
        }
        self.scopes.pop();
        if self.current.is_some() {
            let edge = self.block_target(step_block, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        let step_reachable = self.blocks.iter().any(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| successors(terminator).contains(&step_block))
        });
        if step_reachable {
            self.enter_block(step_block)?;
            let cursor = self.read_binding(cursor_binding, pos)?;
            let index = self.read_binding(index_binding, pos)?;
            let bound = self.read_binding(bound_binding, pos)?;
            let advanced = self
                .emit(
                    l::InstructionKind::IteratorAdvance,
                    vec![cursor, index.clone(), bound],
                    Some(iterator_type),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .expect("advanced iterator");
            let next_index = self
                .emit(
                    l::InstructionKind::Binary(l::BinaryOp::Add),
                    vec![
                        index,
                        l::Operand::Constant(l::Constant {
                            ty: Type::I32,
                            kind: l::ConstantKind::Integer(1),
                        }),
                    ],
                    Some(l::ValueType::Data(Type::I32)),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .expect("advanced iterator index");
            self.bindings[cursor_binding.0].value = Some(advanced);
            self.bindings[index_binding.0].value = Some(next_index);
            let edge = self.block_target(header, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        self.controls.pop();
        self.enter_block(exit)?;
        self.release_scopes_from(self.scopes.len() - 1, pos)?;
        self.scopes.pop();
        Ok(())
    }

    fn create_iterator(
        &mut self,
        subject: l::Operand,
        kind: l::ForOfKind,
        element: Type,
        bound: l::IteratorBoundKind,
        pos: &Pos,
    ) -> Result<(l::ValueType, l::Operand), LowerError> {
        let iterator_type = l::ValueType::Iterator(l::IteratorType { kind, element });
        let iterator = self
            .emit(
                l::InstructionKind::IteratorCreate { kind, bound },
                vec![subject],
                Some(iterator_type.clone()),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator result");
        Ok((iterator_type, iterator))
    }

    fn lower_static_callback(
        &mut self,
        callback: &hir::Expr,
    ) -> Result<StaticCallback, LowerError> {
        let Type::Func(_) = &callback.ty else {
            return Err(self.error(&callback.pos, "static callback is not function-typed"));
        };
        match &callback.kind {
            hir::ExprKind::FuncRef(name) => {
                let function = self
                    .lowering
                    .free_functions
                    .get(name)
                    .map(|record| record.id)
                    .ok_or_else(|| {
                        self.error(&callback.pos, format!("unknown callback function `{name}`"))
                    })?;
                let _ = self.require_expr(callback)?;
                Ok(StaticCallback {
                    target: l::CallTargetKind::Function(function),
                    callable: None,
                    ty: callback.ty.clone(),
                })
            }
            hir::ExprKind::Lambda {
                params,
                ret,
                body,
                captures,
            } => {
                let (function, callable) =
                    self.lower_lambda_with_id(params, ret, body, captures, callback)?;
                Ok(StaticCallback {
                    target: l::CallTargetKind::StaticClosure(function),
                    callable: Some(callable),
                    ty: callback.ty.clone(),
                })
            }
            _ => Err(self.error(&callback.pos, "callback is not a known function")),
        }
    }

    fn emit_static_callback_call(
        &mut self,
        callback: &StaticCallback,
        callable: Option<l::Operand>,
        arguments: Vec<l::Operand>,
        traps: Vec<l::Trap>,
        pos: &Pos,
    ) -> Result<Option<l::Operand>, LowerError> {
        let Type::Func(signature) = &callback.ty else {
            return Err(self.error(pos, "static callback type is not callable"));
        };
        let mut operands = Vec::with_capacity(arguments.len() + usize::from(callable.is_some()));
        let mut parameter_types = Vec::with_capacity(operands.capacity());
        if let Some(callable) = callable {
            operands.push(callable);
            parameter_types.push(l::ValueType::Data(callback.ty.clone()));
        }
        operands.extend(arguments);
        parameter_types.extend(signature.params.iter().cloned().map(l::ValueType::Data));
        let return_type =
            (signature.ret != Type::Void).then(|| l::ValueType::Data(signature.ret.clone()));
        self.emit(
            l::InstructionKind::Call(l::CallTarget {
                kind: callback.target.clone(),
                parameter_types,
                return_type: return_type.clone(),
            }),
            operands,
            return_type,
            true,
            traps,
            pos.clone(),
        )
    }

    fn emit_static_array_push(
        &mut self,
        array: l::Operand,
        value: l::Operand,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        self.emit(
            l::InstructionKind::Call(l::CallTarget {
                kind: l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPush),
                parameter_types: Vec::new(),
                return_type: Some(l::ValueType::Data(Type::I32)),
            }),
            vec![array, value],
            Some(l::ValueType::Data(Type::I32)),
            true,
            Vec::new(),
            pos.clone(),
        )?;
        Ok(())
    }

    fn lower_static_array_callback(
        &mut self,
        operation: hir::ArrFn,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let Some(subject) = args.first() else {
            return Err(self.error(&expr.pos, "static Array callback has no receiver"));
        };
        let Some(callback_expr) = args.get(1) else {
            return Err(self.error(&expr.pos, "static Array callback has no callback"));
        };
        let Type::Array(element) = &subject.ty else {
            return Err(self.error(
                &subject.pos,
                "static Array callback receiver is not dynamic",
            ));
        };
        let element = (**element).clone();
        let subject_value = self.require_expr(subject)?;
        let callback = self.lower_static_callback(callback_expr)?;
        let initial = if matches!(operation, hir::ArrFn::Reduce | hir::ArrFn::ReduceRight) {
            Some(
                args.get(2)
                    .ok_or_else(|| self.error(&expr.pos, "reduce has no initial value"))
                    .and_then(|initial| self.require_expr(initial))?,
            )
        } else {
            None
        };
        let Type::Func(callback_type) = &callback.ty else {
            unreachable!("static callback type was checked")
        };
        let indexed_arity = operation.callback_index_arity();
        let indexed = indexed_arity == Some(callback_type.params.len());
        if !indexed && indexed_arity.is_some_and(|arity| callback_type.params.len() + 1 != arity) {
            return Err(self.error(&callback_expr.pos, "static callback arity is invalid"));
        }

        let reverse = operation == hir::ArrFn::ReduceRight;
        let kind = if reverse {
            l::ForOfKind::ArrayValuesReverse
        } else {
            l::ForOfKind::ArrayValues
        };
        let (iterator_type, iterator) = self.create_iterator(
            subject_value.clone(),
            kind,
            element.clone(),
            l::IteratorBoundKind::Fixed,
            &expr.pos,
        )?;
        let reverse_index_iterator = if reverse && indexed {
            Some(self.create_iterator(
                subject_value,
                l::ForOfKind::ArrayKeysReverse,
                Type::I32,
                l::IteratorBoundKind::Fixed,
                &expr.pos,
            )?)
        } else {
            None
        };
        let bound = self
            .emit(
                l::InstructionKind::IteratorBound,
                vec![iterator.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("static callback iterator bound");
        let call_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
            .into_iter()
            .filter(|trap| trap.kind == l::TrapKind::Call)
            .collect::<Vec<_>>();

        let output = if matches!(operation, hir::ArrFn::Map | hir::ArrFn::Filter) {
            let Type::Array(output_element) = &expr.ty else {
                return Err(self.error(&expr.pos, "Array producer result is not an array"));
            };
            Some(
                self.emit(
                    l::InstructionKind::ArrayWithCapacity,
                    vec![bound.clone()],
                    Some(l::ValueType::Data(Type::Array(output_element.clone()))),
                    false,
                    call_traps.clone(),
                    expr.pos.clone(),
                )?
                .expect("capacity array result"),
            )
        } else {
            None
        };
        let initial_result = match operation {
            hir::ArrFn::Map | hir::ArrFn::Filter => output.clone(),
            hir::ArrFn::Reduce | hir::ArrFn::ReduceRight => initial,
            hir::ArrFn::Some => Some(bool_constant(false)),
            hir::ArrFn::Every => Some(bool_constant(true)),
            hir::ArrFn::FindIndex => Some(i32_constant(-1)),
            hir::ArrFn::ForEach => None,
            _ => return Err(self.error(&expr.pos, "unsupported static Array callback operation")),
        };

        self.scopes.push(HashMap::new());
        let cursor_binding =
            self.declare_hidden_binding("<array callback cursor>", iterator_type.clone(), iterator);
        let reverse_index_binding = reverse_index_iterator.map(|(ty, iterator)| {
            self.declare_hidden_binding("<array callback reverse index>", ty, iterator)
        });
        let callable_binding = callback.callable.clone().map(|callable| {
            self.declare_hidden_binding(
                "<array callback callable>",
                l::ValueType::Data(callback.ty.clone()),
                callable,
            )
        });
        let index_binding = self.declare_hidden_binding(
            "<array callback step>",
            l::ValueType::Data(Type::I32),
            i32_constant(0),
        );
        let bound_binding = self.declare_hidden_binding(
            "<array callback bound>",
            l::ValueType::Data(Type::I32),
            bound,
        );
        let result_binding = initial_result.map(|value| {
            self.declare_hidden_binding(
                "<array callback result>",
                l::ValueType::Data(expr.ty.clone()),
                value,
            )
        });
        let mut traversal = vec![cursor_binding, index_binding, bound_binding];
        traversal.extend(reverse_index_binding);
        traversal.extend(callable_binding);
        traversal.extend(result_binding);

        let header = self.new_state_block(
            Vec::new(),
            Some("array-callback.cond".to_string()),
            &traversal,
        );
        let body = self.new_block(Vec::new(), Some("array-callback.body".to_string()));
        let step = self.new_state_block(
            Vec::new(),
            Some("array-callback.step".to_string()),
            &traversal,
        );
        let exit_forced = result_binding.into_iter().collect::<Vec<_>>();
        let exit = self.new_state_block(
            Vec::new(),
            Some("array-callback.exit".to_string()),
            &exit_forced,
        );
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(header)?;
        let header_cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let header_index = self.read_binding(index_binding, &expr.pos)?;
        let header_bound = self.read_binding(bound_binding, &expr.pos)?;
        let more = self
            .emit(
                l::InstructionKind::IteratorHasNext,
                vec![header_cursor, header_index, header_bound],
                Some(l::ValueType::Data(Type::Bool)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("static callback iterator condition");
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: more,
                then_target: target(body, Vec::new()),
                else_target: exit_target,
            },
            &expr.pos,
        )?;

        self.current = Some(body);
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let step_index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let value = self
            .emit(
                l::InstructionKind::IteratorValue,
                vec![cursor, step_index.clone(), captured_bound.clone()],
                Some(l::ValueType::Data(element.clone())),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("static callback iterator value");
        let callback_index = if let Some(binding) = reverse_index_binding {
            let reverse_cursor = self.read_binding(binding, &expr.pos)?;
            self.emit(
                l::InstructionKind::IteratorValue,
                vec![reverse_cursor, step_index.clone(), captured_bound.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("reverse callback index")
        } else {
            step_index.clone()
        };
        let mut arguments = if matches!(operation, hir::ArrFn::Reduce | hir::ArrFn::ReduceRight) {
            vec![
                self.read_binding(result_binding.expect("reduce result binding"), &expr.pos)?,
                value.clone(),
            ]
        } else {
            vec![value.clone()]
        };
        if indexed {
            arguments.push(callback_index.clone());
        }
        let callable = callable_binding
            .map(|binding| self.read_binding(binding, &expr.pos))
            .transpose()?;
        let callback_result =
            self.emit_static_callback_call(&callback, callable, arguments, call_traps, &expr.pos)?;

        let mut branch_to_step = true;
        match operation {
            hir::ArrFn::Map => {
                let output =
                    self.read_binding(result_binding.expect("map result binding"), &expr.pos)?;
                self.emit_static_array_push(
                    output,
                    callback_result.expect("map callback result"),
                    &expr.pos,
                )?;
            }
            hir::ArrFn::Filter => {
                let push = self.new_block(Vec::new(), Some("array-callback.push".to_string()));
                let step_target = self.block_target(step, Vec::new())?;
                self.terminate(
                    l::Terminator::ConditionalBranch {
                        condition: callback_result.expect("filter callback result"),
                        then_target: target(push, Vec::new()),
                        else_target: step_target,
                    },
                    &expr.pos,
                )?;
                self.current = Some(push);
                let output =
                    self.read_binding(result_binding.expect("filter result binding"), &expr.pos)?;
                self.emit_static_array_push(output, value, &expr.pos)?;
            }
            hir::ArrFn::Reduce | hir::ArrFn::ReduceRight => {
                self.bindings[result_binding.expect("reduce result binding").0].value =
                    Some(callback_result.expect("reduce callback result"));
            }
            hir::ArrFn::ForEach => {}
            hir::ArrFn::Some | hir::ArrFn::Every | hir::ArrFn::FindIndex => {
                let matched = callback_result.expect("predicate callback result");
                let (finish_on_true, finished_value) = match operation {
                    hir::ArrFn::Some => (true, bool_constant(true)),
                    hir::ArrFn::Every => (false, bool_constant(false)),
                    hir::ArrFn::FindIndex => (true, callback_index),
                    _ => unreachable!(),
                };
                let finish = self.new_block(Vec::new(), Some("array-callback.finish".to_string()));
                let step_target = self.block_target(step, Vec::new())?;
                let (then_target, else_target) = if finish_on_true {
                    (target(finish, Vec::new()), step_target)
                } else {
                    (step_target, target(finish, Vec::new()))
                };
                self.terminate(
                    l::Terminator::ConditionalBranch {
                        condition: matched,
                        then_target,
                        else_target,
                    },
                    &expr.pos,
                )?;
                self.current = Some(finish);
                self.bindings[result_binding.expect("predicate result binding").0].value =
                    Some(finished_value);
                let exit_target = self.block_target(exit, Vec::new())?;
                self.terminate(l::Terminator::Branch(exit_target), &expr.pos)?;
                branch_to_step = false;
            }
            _ => unreachable!(),
        }
        if branch_to_step {
            let edge = self.block_target(step, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), &expr.pos)?;
        }

        self.enter_block(step)?;
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let advanced = self
            .emit(
                l::InstructionKind::IteratorAdvance,
                vec![cursor, index.clone(), captured_bound.clone()],
                Some(iterator_type),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced static callback iterator");
        if reverse {
            self.bindings[cursor_binding.0].value = Some(advanced);
        }
        if let Some(binding) = reverse_index_binding {
            let cursor = self.read_binding(binding, &expr.pos)?;
            let iterator_type = self.bindings[binding.0].ty.clone();
            let advanced = self
                .emit(
                    l::InstructionKind::IteratorAdvance,
                    vec![cursor, index.clone(), captured_bound],
                    Some(iterator_type),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .expect("advanced reverse index iterator");
            self.bindings[binding.0].value = Some(advanced);
        }
        let next_index = self
            .emit(
                l::InstructionKind::Binary(l::BinaryOp::Add),
                vec![index, i32_constant(1)],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced static callback step");
        self.bindings[index_binding.0].value = Some(next_index);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(exit)?;
        let result = result_binding
            .map(|binding| self.read_binding(binding, &expr.pos))
            .transpose()?;
        self.release_scopes_from(self.scopes.len() - 1, &expr.pos)?;
        self.scopes.pop();
        Ok(result)
    }

    fn lower_for_each(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let [subject, callback] = args else {
            return Err(self.error(
                &expr.pos,
                format!("forEach lowering expected 2 operands, got {}", args.len()),
            ));
        };
        let subject_value = self.require_expr(subject)?;
        let callback_value = self.require_expr(callback)?;
        let Type::Func(callback_type) = &callback.ty else {
            return Err(self.error(&callback.pos, "forEach callback is not function-typed"));
        };
        if callback_type.ret != Type::Void {
            return Err(self.error(&callback.pos, "forEach callback does not return void"));
        }

        let (kind, element, secondary, bound) = match (callee, &subject.ty) {
            (hir::Callee::Arr(hir::ArrFn::ForEach), Type::Array(element)) => (
                l::ForOfKind::ArrayValues,
                (**element).clone(),
                None,
                l::IteratorBoundKind::Fixed,
            ),
            (hir::Callee::Arr(hir::ArrFn::ForEach), Type::FixedArray(element, _)) => (
                l::ForOfKind::FixedArrayValues,
                (**element).clone(),
                None,
                l::IteratorBoundKind::Live,
            ),
            (hir::Callee::Map(hir::MapFn::ForEach), Type::Map(key, value)) => (
                l::ForOfKind::MapValues,
                (**value).clone(),
                Some((l::ForOfKind::MapKeys, (**key).clone())),
                l::IteratorBoundKind::Live,
            ),
            (hir::Callee::Set(hir::SetFn::ForEach), Type::Set(key)) => (
                l::ForOfKind::SetValues,
                (**key).clone(),
                None,
                l::IteratorBoundKind::Live,
            ),
            _ => {
                return Err(self.error(&subject.pos, "forEach spelling and receiver type disagree"));
            }
        };

        let (iterator_type, iterator) = self.create_iterator(
            subject_value.clone(),
            kind,
            element.clone(),
            bound,
            &expr.pos,
        )?;
        let secondary_iterator = secondary
            .map(|(kind, element)| {
                self.create_iterator(subject_value.clone(), kind, element, bound, &expr.pos)
            })
            .transpose()?;
        let captured_bound = self
            .emit(
                l::InstructionKind::IteratorBound,
                vec![iterator.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("iterator bound");
        let index = l::Operand::Constant(l::Constant {
            ty: Type::I32,
            kind: l::ConstantKind::Integer(0),
        });

        self.scopes.push(HashMap::new());
        let cursor_binding =
            self.declare_hidden_binding("<for-each cursor>", iterator_type.clone(), iterator);
        let secondary_binding = secondary_iterator.map(|(ty, iterator)| {
            self.declare_hidden_binding("<for-each secondary cursor>", ty, iterator)
        });
        let callback_binding = self.declare_hidden_binding(
            "<for-each callback>",
            l::ValueType::Data(callback.ty.clone()),
            callback_value,
        );
        let index_binding =
            self.declare_hidden_binding("<for-each index>", l::ValueType::Data(Type::I32), index);
        let bound_binding = self.declare_hidden_binding(
            "<for-each bound>",
            l::ValueType::Data(Type::I32),
            captured_bound,
        );
        let mut traversal = vec![
            cursor_binding,
            callback_binding,
            index_binding,
            bound_binding,
        ];
        traversal.extend(secondary_binding);

        let header =
            self.new_state_block(Vec::new(), Some("for-each.cond".to_string()), &traversal);
        let body = self.new_block(Vec::new(), Some("for-each.body".to_string()));
        let step = self.new_state_block(Vec::new(), Some("for-each.step".to_string()), &traversal);
        let exit = self.new_state_block(Vec::new(), Some("for-each.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(header)?;
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let more = self
            .emit(
                l::InstructionKind::IteratorHasNext,
                vec![cursor, index, captured_bound],
                Some(l::ValueType::Data(Type::Bool)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("iterator condition");
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: more,
                then_target: target(body, Vec::new()),
                else_target: exit_target,
            },
            &expr.pos,
        )?;

        self.current = Some(body);
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let value = self
            .emit(
                l::InstructionKind::IteratorValue,
                vec![cursor, index.clone(), captured_bound.clone()],
                Some(l::ValueType::Data(element)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("iterator value");
        let mut call_operands = vec![self.read_binding(callback_binding, &expr.pos)?, value];
        if let Some(secondary_binding) = secondary_binding {
            let secondary_cursor = self.read_binding(secondary_binding, &expr.pos)?;
            let secondary_type = self.bindings[secondary_binding.0].ty.clone();
            let l::ValueType::Iterator(secondary_type) = secondary_type else {
                unreachable!("secondary cursor binding has an iterator type")
            };
            let secondary_value = self
                .emit(
                    l::InstructionKind::IteratorValue,
                    vec![secondary_cursor, index.clone(), captured_bound],
                    Some(l::ValueType::Data(secondary_type.element)),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .expect("secondary iterator value");
            call_operands.push(secondary_value);
        } else if callback_type.params.len() == 2 {
            call_operands.push(index.clone());
        }
        self.emit(
            l::InstructionKind::Call(l::CallTarget {
                kind: l::CallTargetKind::Indirect,
                parameter_types: std::iter::once(l::ValueType::Data(callback.ty.clone()))
                    .chain(callback_type.params.iter().cloned().map(l::ValueType::Data))
                    .collect(),
                return_type: None,
            }),
            call_operands,
            None,
            true,
            convert_traps(&expr.trap_sites(self.lowering.hir)),
            expr.pos.clone(),
        )?;
        let edge = self.block_target(step, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(step)?;
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let advanced = self
            .emit(
                l::InstructionKind::IteratorAdvance,
                vec![cursor, index.clone(), captured_bound.clone()],
                Some(iterator_type),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced iterator");
        self.bindings[cursor_binding.0].value = Some(advanced);
        if let Some(secondary_binding) = secondary_binding {
            let cursor = self.read_binding(secondary_binding, &expr.pos)?;
            let iterator_type = self.bindings[secondary_binding.0].ty.clone();
            let advanced = self
                .emit(
                    l::InstructionKind::IteratorAdvance,
                    vec![cursor, index.clone(), captured_bound],
                    Some(iterator_type),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .expect("advanced secondary iterator");
            self.bindings[secondary_binding.0].value = Some(advanced);
        }
        let next_index = self
            .emit(
                l::InstructionKind::Binary(l::BinaryOp::Add),
                vec![
                    index,
                    l::Operand::Constant(l::Constant {
                        ty: Type::I32,
                        kind: l::ConstantKind::Integer(1),
                    }),
                ],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced iterator index");
        self.bindings[index_binding.0].value = Some(next_index);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(exit)?;
        self.release_scopes_from(self.scopes.len() - 1, &expr.pos)?;
        self.scopes.pop();
        Ok(None)
    }

    fn lower_switch(
        &mut self,
        disc: &hir::Expr,
        cases: &[hir::SwitchCase],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let value = self.require_expr(disc)?;
        let exit = self.new_state_block(Vec::new(), Some("switch.exit".to_string()), &[]);
        let exhaustive_alias =
            matches!(disc.ty, Type::StringAlias(_)) && cases.iter().all(|case| case.test.is_some());
        let no_match = if exhaustive_alias {
            let block = self.new_block(Vec::new(), Some("switch.exhaustive".to_string()));
            self.blocks[block.0 as usize].terminator =
                Some(l::Terminator::Unreachable { pos: pos.clone() });
            block
        } else {
            exit
        };
        let case_blocks = cases
            .iter()
            .enumerate()
            .map(|(index, _)| {
                self.new_state_block(Vec::new(), Some(format!("switch.case.{index}")), &[])
            })
            .collect::<Vec<_>>();
        let mut default = no_match;
        for (case, block) in cases.iter().zip(&case_blocks) {
            if case.test.is_none() {
                default = *block;
            }
        }
        let all_constant = cases
            .iter()
            .filter_map(|case| case.test.as_ref())
            .all(|test| constant_expr(test).is_some());
        if all_constant {
            let arms = cases
                .iter()
                .zip(&case_blocks)
                .filter_map(|(case, block)| {
                    case.test.as_ref().map(|test| {
                        Ok(l::SwitchArm {
                            value: constant_expr(test).expect("constant switch case"),
                            target: self.block_target(*block, Vec::new())?,
                        })
                    })
                })
                .collect::<Result<Vec<_>, LowerError>>()?;
            let default = self.block_target(default, Vec::new())?;
            self.terminate(
                l::Terminator::Switch {
                    value,
                    arms,
                    default,
                },
                pos,
            )?;
        } else {
            let tests = cases
                .iter()
                .enumerate()
                .filter_map(|(index, case)| case.test.as_ref().map(|test| (index, test)))
                .collect::<Vec<_>>();
            if tests.is_empty() {
                let edge = self.block_target(default, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            } else {
                for (test_index, (case_index, test)) in tests.iter().enumerate() {
                    let test_value = self.require_expr(test)?;
                    let test_value = self.coerce_operand(
                        test_value,
                        l::ValueType::Data(disc.ty.clone()),
                        &test.pos,
                    )?;
                    let equal = self
                        .emit(
                            l::InstructionKind::Binary(l::BinaryOp::Eq),
                            vec![value.clone(), test_value],
                            Some(l::ValueType::Data(Type::Bool)),
                            false,
                            Vec::new(),
                            test.pos.clone(),
                        )?
                        .expect("switch test result");
                    let miss = if test_index + 1 == tests.len() {
                        default
                    } else {
                        self.new_state_block(
                            Vec::new(),
                            Some(format!("switch.test.{}", test_index + 1)),
                            &[],
                        )
                    };
                    let then_target = self.block_target(case_blocks[*case_index], Vec::new())?;
                    let else_target = self.block_target(miss, Vec::new())?;
                    self.terminate(
                        l::Terminator::ConditionalBranch {
                            condition: equal,
                            then_target,
                            else_target,
                        },
                        &test.pos,
                    )?;
                    if test_index + 1 != tests.len() {
                        self.enter_block(miss)?;
                    }
                }
            }
        }
        self.controls.push(Control {
            break_target: exit,
            continue_target: None,
            scope_depth: self.scopes.len(),
        });
        let mut previous_end = None;
        for (index, (case, block)) in cases.iter().zip(&case_blocks).enumerate() {
            if let Some(end) = previous_end {
                self.current = Some(end);
                let edge = self.block_target(*block, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), &case.pos)?;
            }
            self.enter_block(*block)?;
            self.lower_scoped(&case.body)?;
            previous_end = self.current;
            if index + 1 == cases.len() {
                if let Some(end) = previous_end.take() {
                    self.current = Some(end);
                    let edge = self.block_target(exit, Vec::new())?;
                    self.terminate(l::Terminator::Branch(edge), &case.pos)?;
                }
            }
        }
        self.controls.pop();
        let exit_reachable = self.blocks.iter().any(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| successors(terminator).contains(&exit))
        });
        if exit_reachable {
            self.enter_block(exit)?;
        } else {
            self.current = None;
        }
        Ok(())
    }

    fn lower_expr(&mut self, expr: &hir::Expr) -> Result<Option<l::Operand>, LowerError> {
        use hir::ExprKind as K;
        let result = match &expr.kind {
            K::Int(value) => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::Integer(*value),
            })),
            K::Float(value) => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::FloatBits(if expr.ty == Type::F32 {
                    u64::from((*value as f32).to_bits())
                } else {
                    value.to_bits()
                }),
            })),
            K::Bool(value) => Some(l::Operand::Constant(l::Constant {
                ty: Type::Bool,
                kind: l::ConstantKind::Boolean(*value),
            })),
            K::Null => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::Null,
            })),
            K::Str(value) => self.emit(
                l::InstructionKind::StringLiteral(value.clone()),
                Vec::new(),
                Some(l::ValueType::Data(expr.ty.clone())),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?,
            K::This => Some(
                self.this_value
                    .clone()
                    .ok_or_else(|| self.error(&expr.pos, "`this` has no receiver parameter"))?,
            ),
            K::Local(name) => {
                if let Some(value) = self.lookup_substitution(name) {
                    Some(self.coerce_operand(
                        value,
                        l::ValueType::Data(expr.ty.clone()),
                        &expr.pos,
                    )?)
                } else {
                    let binding = self.lookup_binding(name, &expr.pos)?;
                    let value = self.read_binding(binding, &expr.pos)?;
                    Some(self.coerce_operand(
                        value,
                        l::ValueType::Data(expr.ty.clone()),
                        &expr.pos,
                    )?)
                }
            }
            K::Global(name) => {
                let global = self
                    .lowering
                    .globals
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown global `{name}`")))?;
                let stored_type = self
                    .lowering
                    .hir
                    .globals
                    .get(global.0 as usize)
                    .map(|global| global.ty.clone())
                    .ok_or_else(|| self.error(&expr.pos, "global declaration is missing"))?;
                let value = self
                    .emit(
                        l::InstructionKind::LoadGlobal(global),
                        Vec::new(),
                        Some(l::ValueType::Data(stored_type)),
                        false,
                        Vec::new(),
                        expr.pos.clone(),
                    )?
                    .expect("global load");
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::FuncRef(name) => {
                let function = self
                    .lowering
                    .free_functions
                    .get(name)
                    .map(|record| record.id)
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown function `{name}`")))?;
                self.emit(
                    l::InstructionKind::FunctionRef(function),
                    Vec::new(),
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
            }
            K::EnumMember { value, .. } => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::Integer(*value),
            })),
            K::Unary { op, operand } => {
                let operand = self.require_expr(operand)?;
                self.emit(
                    l::InstructionKind::Unary(convert_unary(*op)),
                    vec![operand],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Binary {
                op: hir::BinOp::And,
                left,
                right,
            } => Some(self.lower_short_circuit(left, right, false, expr)?),
            K::Binary {
                op: hir::BinOp::Or,
                left,
                right,
            } => Some(self.lower_short_circuit(left, right, true, expr)?),
            K::Binary { op, left, right } => {
                let left = self.require_expr(left)?;
                let right = self.require_expr(right)?;
                self.emit(
                    l::InstructionKind::Binary(convert_binary(*op)?),
                    vec![left, right],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::AbsenceTest { value, negated } => {
                let Type::StringAlias(alias) = value.ty else {
                    return Err(self.error(&value.pos, "absence test value is not a string alias"));
                };
                let discriminant = self
                    .lowering
                    .hir
                    .string_aliases
                    .get(alias.0)
                    .map(hir::StringAliasDef::absence_discriminant)
                    .ok_or_else(|| self.error(&value.pos, "absence alias is missing"))?;
                let value = self.require_expr(value)?;
                let absent = l::Operand::Constant(l::Constant {
                    ty: Type::StringAlias(alias),
                    kind: l::ConstantKind::Integer(discriminant),
                });
                self.emit(
                    l::InstructionKind::Binary(if *negated {
                        l::BinaryOp::Ne
                    } else {
                        l::BinaryOp::Eq
                    }),
                    vec![value, absent],
                    Some(l::ValueType::Data(Type::Bool)),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Assign { op, target, value } => {
                Some(self.lower_assignment(*op, target, value, expr)?)
            }
            K::Cast(value) => {
                let value = self.require_expr(value)?;
                self.emit(
                    l::InstructionKind::Cast,
                    vec![value],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Call { callee, args } => self.lower_call(callee, args, expr)?,
            K::New { class, args } => Some(self.lower_new(*class, args, expr)?),
            K::DescriptorLit { class, fields } => {
                Some(self.lower_descriptor(*class, fields, expr)?)
            }
            K::Zero => self.emit(
                l::InstructionKind::Zero,
                Vec::new(),
                Some(l::ValueType::Data(expr.ty.clone())),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?,
            K::RawNew { class } => self.emit(
                l::InstructionKind::AllocateClass(*class),
                Vec::new(),
                Some(self.allocated_type(*class, &expr.pos)?),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?,
            K::Field { obj, name } => {
                let object = self.require_expr(obj)?;
                let field = self.resolve_field(&obj.ty, name, &expr.pos)?;
                let stored_type = self.resolved_field_type(field, &obj.ty, &expr.pos)?;
                let value = self
                    .emit(
                        l::InstructionKind::LoadField(field),
                        vec![object],
                        Some(l::ValueType::Data(stored_type)),
                        false,
                        convert_traps(&expr.trap_sites(self.lowering.hir)),
                        expr.pos.clone(),
                    )?
                    .expect("field load");
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::JsonResultValue(obj) => {
                let object = self.require_expr(obj)?;
                let field = self.resolve_field(&obj.ty, "value", &expr.pos)?;
                let ok_field = match self.resolve_field(&obj.ty, "ok", &expr.pos)? {
                    l::FieldRef::Class(field) => field,
                    _ => {
                        return Err(
                            self.error(&expr.pos, "JSON result ok field is not a class field")
                        );
                    }
                };
                let stored_type = self.resolved_field_type(field, &obj.ty, &expr.pos)?;
                let traps = convert_traps(&expr.trap_sites(self.lowering.hir))
                    .into_iter()
                    .map(|mut trap| {
                        if matches!(trap.kind, l::TrapKind::JsonResultValue(_)) {
                            trap.kind = l::TrapKind::JsonResultValue(ok_field);
                        }
                        trap
                    })
                    .collect();
                let value = self
                    .emit(
                        l::InstructionKind::LoadField(field),
                        vec![object],
                        Some(l::ValueType::Data(stored_type)),
                        false,
                        traps,
                        expr.pos.clone(),
                    )?
                    .expect("JSON result field load");
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::Length(value) => {
                let value = self.require_expr(value)?;
                self.emit(
                    l::InstructionKind::Length,
                    vec![value],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Index { .. } => {
                let place = self.prepare_place(expr)?;
                let value = self.load_place(&place, &expr.pos)?;
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::ArrayLit(elements) => {
                let element_type = match &expr.ty {
                    Type::Array(element) | Type::FixedArray(element, _) => (**element).clone(),
                    _ => {
                        return Err(
                            self.error(&expr.pos, "array literal result is not array-typed")
                        );
                    }
                };
                let operands = elements
                    .iter()
                    .map(|element| self.lower_stored_expr(&element_type, element))
                    .collect::<Result<Vec<_>, _>>()?;
                let stored = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| StoredOperand {
                        index,
                        ty: l::ValueType::Data(element_type.clone()),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::ArrayElement),
                        pos: element.pos.clone(),
                    })
                    .collect();
                self.emit_store_instruction(
                    l::InstructionKind::ArrayLiteral,
                    operands,
                    stored,
                    (Some(l::ValueType::Data(expr.ty.clone())), false),
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::ArraySpreadLit(elements) => {
                let Type::Array(element_type) = &expr.ty else {
                    return Err(
                        self.error(&expr.pos, "spread literal result is not a dynamic array")
                    );
                };
                let operands = elements
                    .iter()
                    .map(|element| {
                        if element.spread.is_none() {
                            self.lower_stored_expr(element_type, &element.expr)
                        } else {
                            self.require_expr(&element.expr)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let spreads = elements
                    .iter()
                    .map(|element| element.spread.map(convert_spread))
                    .collect();
                let stored = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| StoredOperand {
                        index,
                        ty: l::ValueType::Data(if element.spread.is_none() {
                            (**element_type).clone()
                        } else {
                            element.expr.ty.clone()
                        }),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::SpreadElement),
                        pos: element.expr.pos.clone(),
                    })
                    .collect();
                self.emit_store_instruction(
                    l::InstructionKind::ArraySpreadLiteral(spreads),
                    operands,
                    stored,
                    (Some(l::ValueType::Data(expr.ty.clone())), false),
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Template(parts) => {
                let mut operands = Vec::new();
                let mut lowered_parts = Vec::new();
                for part in parts {
                    match part {
                        hir::TplPart::Text(text) => {
                            lowered_parts.push(l::TemplatePart::Text(text.clone()));
                        }
                        hir::TplPart::Expr(value) => {
                            let index = operands.len() as u32;
                            operands.push(self.require_expr(value)?);
                            let format = match value.ty {
                                Type::I8 | Type::I16 | Type::I32 | Type::Enum(_) => {
                                    l::FormatKind::I32
                                }
                                Type::U8 | Type::U16 | Type::U32 => l::FormatKind::U32,
                                Type::I64 | Type::Date => l::FormatKind::I64,
                                Type::U64 => l::FormatKind::U64,
                                Type::F16 => l::FormatKind::F16,
                                Type::F32 => l::FormatKind::F32,
                                Type::F64 => l::FormatKind::F64,
                                Type::Bool => l::FormatKind::Bool,
                                Type::Str => l::FormatKind::Str,
                                Type::StringAlias(alias) => l::FormatKind::StringAlias(alias),
                                ref other => {
                                    return Err(self.error(
                                        &value.pos,
                                        format!("template operand {other:?} is not formattable"),
                                    ))
                                }
                            };
                            lowered_parts.push(l::TemplatePart::Operand { index, format });
                        }
                        other => {
                            return Err(self.error(
                                &expr.pos,
                                format!("unrecognized template part: {other:?}"),
                            ));
                        }
                    }
                }
                let traps = if lowered_parts.is_empty() {
                    Vec::new()
                } else {
                    convert_traps(&expr.trap_sites(self.lowering.hir))
                };
                self.emit(
                    l::InstructionKind::Template(lowered_parts),
                    operands,
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    traps,
                    expr.pos.clone(),
                )?
            }
            K::Lambda {
                params,
                ret,
                body,
                captures,
            } => Some(self.lower_lambda(params, ret, body, captures, expr)?),
            K::Yield(value) => {
                let value = value
                    .as_deref()
                    .map(|value| self.require_expr(value))
                    .transpose()?
                    .map(|value| self.terminator_value(value, &expr.pos))
                    .transpose()?;
                let successor = self.new_block(Vec::new(), Some("yield.resume".to_string()));
                self.terminate(
                    l::Terminator::Suspend {
                        kind: l::SuspendKind::Yield(value),
                        pos: expr.pos.clone(),
                        successor,
                        resume_value: None,
                        arguments: Vec::new(),
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                    },
                    &expr.pos,
                )?;
                self.current = Some(successor);
                None
            }
            K::AsyncSuspend => {
                let successor = self.new_block(Vec::new(), Some("async.resume".to_string()));
                self.terminate(
                    l::Terminator::Suspend {
                        kind: l::SuspendKind::Async,
                        pos: expr.pos.clone(),
                        successor,
                        resume_value: None,
                        arguments: Vec::new(),
                        invalidates: self.array_values.clone(),
                        traps: Vec::new(),
                    },
                    &expr.pos,
                )?;
                self.current = Some(successor);
                None
            }
            K::AsyncCall { callee, args } => self.lower_async_call(callee, args, expr)?,
            K::AsyncHandleCreate { callee, args, .. } => {
                Some(self.lower_async_handle_create(callee, args, expr)?)
            }
            K::AsyncHandleAwait(handle) => self.lower_async_handle_await(handle, expr)?,
            K::AsyncHandleTransfer { value, .. } => self.lower_expr(value)?,
            K::Cond { cond, then, els } => Some(self.lower_cond(cond, then, els, expr)?),
        };
        Ok(result)
    }

    fn lower_short_circuit(
        &mut self,
        left: &hir::Expr,
        right: &hir::Expr,
        short_value: bool,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let left = self.require_expr(left)?;
        let branch_state = self.binding_snapshot();
        let right_block = self.new_block(Vec::new(), Some("logic.rhs".to_string()));
        let merge = self.new_state_block(
            vec![l::ValueType::Data(expr.ty.clone())],
            Some("logic.merge".to_string()),
            &[],
        );
        let short = l::Operand::Constant(l::Constant {
            ty: Type::Bool,
            kind: l::ConstantKind::Boolean(short_value),
        });
        let short_target = self.block_target(merge, vec![short])?;
        let (then_target, else_target) = if short_value {
            (short_target, target(right_block, Vec::new()))
        } else {
            (target(right_block, Vec::new()), short_target)
        };
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: left,
                then_target,
                else_target,
            },
            &expr.pos,
        )?;
        self.current = Some(right_block);
        self.restore_bindings(&branch_state);
        let right = self.require_expr(right)?;
        let edge = self.block_target(merge, vec![right])?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;
        self.enter_block(merge)?;
        Ok(l::Operand::Value(
            self.blocks[merge.0 as usize].parameters[0],
        ))
    }

    fn lower_cond(
        &mut self,
        cond: &hir::Expr,
        then: &hir::Expr,
        els: &hir::Expr,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let condition = self.require_expr(cond)?;
        let branch_state = self.binding_snapshot();
        let then_block = self.new_block(Vec::new(), Some("cond.then".to_string()));
        let else_block = self.new_block(Vec::new(), Some("cond.else".to_string()));
        let merge = self.new_state_block(
            vec![l::ValueType::Data(expr.ty.clone())],
            Some("cond.merge".to_string()),
            &[],
        );
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition,
                then_target: target(then_block, Vec::new()),
                else_target: target(else_block, Vec::new()),
            },
            &expr.pos,
        )?;
        self.current = Some(then_block);
        let then_value = self.lower_stored_expr(&expr.ty, then)?;
        let result_type = l::ValueType::Data(expr.ty.clone());
        let then_is_fresh = matches!(&then_value, l::Operand::Value(value)
            if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner));
        let edge = self.block_target(merge, vec![then_value])?;
        self.terminate(l::Terminator::Branch(edge), &then.pos)?;
        self.restore_bindings(&branch_state);
        self.current = Some(else_block);
        let else_value = self.lower_stored_expr(&expr.ty, els)?;
        let else_is_fresh = matches!(&else_value, l::Operand::Value(value)
            if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner));
        let edge = self.block_target(merge, vec![else_value])?;
        self.terminate(l::Terminator::Branch(edge), &els.pos)?;
        self.enter_block(merge)?;
        let result = self.blocks[merge.0 as usize].parameters[0];
        if is_async_owner_type(&result_type) && then_is_fresh && else_is_fresh {
            let transfers_fresh_owner = match hir::AsyncCopySite::ConditionalResult {
                hir::AsyncCopySite::ConditionalResult => true,
                hir::AsyncCopySite::Binding
                | hir::AsyncCopySite::Assignment
                | hir::AsyncCopySite::ArrayElement
                | hir::AsyncCopySite::SpreadElement
                | hir::AsyncCopySite::CallArgument
                | hir::AsyncCopySite::Return
                | hir::AsyncCopySite::ForOfBinding
                | hir::AsyncCopySite::DiscardedResult => false,
            };
            if transfers_fresh_owner {
                self.values[result.0 as usize].fresh_owner = true;
            }
        }
        Ok(l::Operand::Value(result))
    }

    fn lower_assignment(
        &mut self,
        op: Option<hir::BinOp>,
        target_expr: &hir::Expr,
        value_expr: &hir::Expr,
        whole: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let traps = convert_traps(&whole.trap_sites(self.lowering.hir));
        let binary_traps = || {
            traps
                .iter()
                .filter(|trap| {
                    matches!(
                        trap.kind,
                        l::TrapKind::Allocation | l::TrapKind::DivisionByZero
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        if let hir::ExprKind::Local(name) = &target_expr.kind {
            let binding = self.lookup_binding(name, &target_expr.pos)?;
            let old = if op.is_some() {
                Some(self.read_binding(binding, &target_expr.pos)?)
            } else {
                None
            };
            let result = if let Some(op) = op {
                let value = self.require_expr(value_expr)?;
                self.emit(
                    l::InstructionKind::Binary(convert_binary(op)?),
                    vec![old.expect("compound old value"), value],
                    Some(l::ValueType::Data(target_expr.ty.clone())),
                    false,
                    binary_traps(),
                    target_expr.pos.clone(),
                )?
                .expect("compound result")
            } else {
                self.lower_stored_expr_at(&target_expr.ty, value_expr, &target_expr.pos)?
            };
            self.write_binding(binding, result.clone(), &target_expr.pos, Vec::new())?;
            return Ok(result);
        }
        let mut place = self.prepare_place(target_expr)?;
        let direct_index = matches!(place.kind, PreparedPlaceKind::Index { .. });
        let old = if op.is_some() {
            let old = self.load_place(&place, &target_expr.pos)?;
            if direct_index {
                prepare_direct_index_store(&mut place, &traps);
            } else {
                prepare_place_after_checked_read(&mut place);
            }
            Some(old)
        } else {
            if direct_index {
                prepare_direct_index_assignment(&mut place, &traps);
            }
            None
        };
        let result = if let Some(op) = op {
            let value = self.require_expr(value_expr)?;
            self.emit(
                l::InstructionKind::Binary(convert_binary(op)?),
                vec![old.expect("compound old value"), value],
                Some(l::ValueType::Data(target_expr.ty.clone())),
                false,
                binary_traps(),
                target_expr.pos.clone(),
            )?
            .expect("compound result")
        } else {
            self.lower_stored_expr_at(&target_expr.ty, value_expr, &target_expr.pos)?
        };
        self.store_place(&place, result.clone(), &target_expr.pos)?;
        Ok(result)
    }

    fn lower_call(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        if let hir::Callee::Arr(operation) = callee {
            let static_operation = matches!(
                operation,
                hir::ArrFn::Map
                    | hir::ArrFn::Filter
                    | hir::ArrFn::Reduce
                    | hir::ArrFn::ReduceRight
                    | hir::ArrFn::ForEach
                    | hir::ArrFn::Some
                    | hir::ArrFn::Every
                    | hir::ArrFn::FindIndex
            );
            let dynamic_receiver = matches!(
                args.first().map(|argument| &argument.ty),
                Some(Type::Array(_))
            );
            let known_callback = args.get(1).is_some_and(|callback| {
                matches!(
                    callback.kind,
                    hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. }
                )
            });
            if static_operation && dynamic_receiver && known_callback {
                return self.lower_static_array_callback(*operation, args, expr);
            }
        }
        if matches!(
            callee,
            hir::Callee::Map(hir::MapFn::ForEach) | hir::Callee::Set(hir::SetFn::ForEach)
        ) {
            return self.lower_for_each(callee, args, expr);
        }
        if matches!(callee, hir::Callee::Ambient(hir::AmbientFn::Unreachable)) {
            let trap = convert_traps(&expr.trap_sites(self.lowering.hir))
                .into_iter()
                .find(|trap| trap.kind == l::TrapKind::Unreachable)
                .unwrap_or(l::Trap {
                    kind: l::TrapKind::Unreachable,
                    pos: expr.pos.clone(),
                });
            self.terminate(l::Terminator::Trap(trap), &expr.pos)?;
            return Ok(None);
        }

        let (declared_parameter_types, return_type) =
            self.declared_hir_call_signature(callee, args, expr)?;
        let (kind, mut operands, mut params, receiver_for_defaults) =
            self.resolve_call(callee, expr)?;
        if params.is_empty()
            && matches!(
                kind,
                l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
            )
        {
            params = self
                .operation_call_params(&kind, &operands, args, return_type.as_ref())?
                .unwrap_or_else(|| {
                    args.iter()
                        .enumerate()
                        .map(|(index, argument)| CallParam {
                            name: format!("arg{index}"),
                            ty: argument.ty.clone(),
                            default: None,
                            pos: argument.pos.clone(),
                        })
                        .collect()
                });
        }
        let foreign = matches!(kind, l::CallTargetKind::Foreign(_));
        let explicit_offset = operands.len();
        let explicit =
            self.lower_call_arguments(&params, args, receiver_for_defaults.as_ref(), foreign)?;
        operands.extend(explicit);
        if matches!(kind, l::CallTargetKind::Method(_)) {
            if let Some(PreparedBase::Place(place)) = receiver_for_defaults {
                operands[0] = self.materialize_address_inner(&place, &expr.pos, false)?;
            }
        }
        let deleted_field_owners =
            self.owners_destroyed_by_unsafe_delete(callee, args, &operands, explicit_offset)?;
        let table_signature = matches!(
            kind,
            l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
        );
        let parameter_types = if foreign {
            operands
                .iter()
                .map(|operand| self.operand_type(operand, &expr.pos))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            declared_parameter_types
        };
        let target = l::CallTarget {
            kind,
            parameter_types: if table_signature {
                Vec::new()
            } else {
                parameter_types
            },
            return_type: return_type.clone(),
        };
        let stored = if foreign {
            Vec::new()
        } else {
            params
                .iter()
                .enumerate()
                .map(|(index, parameter)| StoredOperand {
                    index: explicit_offset + index,
                    ty: l::ValueType::Data(parameter.ty.clone()),
                    action: OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument),
                    pos: args
                        .get(index)
                        .map_or_else(|| parameter.pos.clone(), |argument| argument.pos.clone()),
                })
                .collect()
        };
        // Nullable boundary boxes are emitted while each argument is lowered;
        // keep their allocation sites on those instructions instead of also
        // attaching them to the eventual call.
        let call_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
            .into_iter()
            .filter(|trap| trap.kind != l::TrapKind::Allocation)
            .collect();
        let result = self.emit_store_instruction(
            l::InstructionKind::Call(target),
            operands,
            stored,
            (return_type, true),
            call_traps,
            expr.pos.clone(),
        )?;
        for (owner, ty, pos) in deleted_field_owners {
            self.release_owner(owner, &ty, &pos)?;
        }
        Ok(result)
    }

    fn operation_call_params(
        &self,
        kind: &l::CallTargetKind,
        prefix: &[l::Operand],
        arguments: &[hir::Expr],
        return_type: Option<&l::ValueType>,
    ) -> Result<Option<Vec<CallParam>>, LowerError> {
        let target = match kind {
            l::CallTargetKind::Intrinsic(intrinsic) => {
                l::CallSignatureTarget::Intrinsic(intrinsic.clone())
            }
            l::CallTargetKind::BuiltinMethod(method) => {
                l::CallSignatureTarget::BuiltinMethod(*method)
            }
            _ => return Ok(None),
        };
        let prefix_types = prefix
            .iter()
            .map(|operand| {
                self.operand_type(
                    operand,
                    arguments
                        .first()
                        .map_or(&self.function.pos, |argument| &argument.pos),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let signature = self
            .lowering
            .hir
            .operation_signatures
            .iter()
            .map(lower_operation_signature)
            .find(|signature| {
                signature.target == target
                    && signature.return_type.as_ref() == return_type
                    && signature.parameter_types.len() == prefix.len() + arguments.len()
                    && signature.parameter_types[..prefix.len()] == prefix_types
                    && signature.parameter_types[prefix.len()..]
                        .iter()
                        .zip(arguments)
                        .all(|(expected, argument)| match expected {
                            l::ValueType::Data(expected) => {
                                expected == &argument.ty
                                    || self.is_boundary_box_narrowing(expected, &argument.ty)
                                    || self.embedded_header_extension(expected, argument).is_some()
                            }
                            l::ValueType::Address(_) | l::ValueType::Iterator(_) => false,
                        })
            });
        Ok(signature.map(|signature| {
            signature.parameter_types[prefix.len()..]
                .iter()
                .zip(arguments)
                .enumerate()
                .map(|(index, (parameter, argument))| CallParam {
                    name: format!("arg{index}"),
                    ty: match parameter {
                        l::ValueType::Data(ty) => ty.clone(),
                        l::ValueType::Address(_) | l::ValueType::Iterator(_) => argument.ty.clone(),
                    },
                    default: None,
                    pos: argument.pos.clone(),
                })
                .collect()
        }))
    }

    fn owners_destroyed_by_unsafe_delete(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        operands: &[l::Operand],
        explicit_offset: usize,
    ) -> Result<Vec<(l::Operand, l::ValueType, Pos)>, LowerError> {
        if !matches!(callee, hir::Callee::Ambient(hir::AmbientFn::UnsafeDelete)) {
            return Ok(Vec::new());
        }
        let Some(argument) = args.first() else {
            return Ok(Vec::new());
        };
        let Type::Class(class_id) = argument.ty else {
            return Ok(Vec::new());
        };
        let class = self
            .lowering
            .hir
            .classes
            .get(class_id.0)
            .ok_or_else(|| self.error(&argument.pos, "deleted class is missing"))?;
        if class.is_value {
            return Ok(Vec::new());
        }
        let fields = class
            .fields
            .iter()
            .filter(|field| is_async_owner_type(&l::ValueType::Data(field.ty.clone())))
            .map(|field| (field.name.clone(), field.ty.clone(), field.pos.clone()))
            .collect::<Vec<_>>();
        let base = operands
            .get(explicit_offset)
            .cloned()
            .ok_or_else(|| self.error(&argument.pos, "deleted class operand is missing"))?;
        let mut owners = Vec::with_capacity(fields.len());
        for (name, field_type, pos) in fields {
            let field = self
                .lowering
                .fields
                .get(&(class_id.0, name))
                .copied()
                .ok_or_else(|| self.error(&pos, "deleted class field is missing"))?;
            let ty = l::ValueType::Data(field_type);
            let owner = self
                .emit(
                    l::InstructionKind::LoadField(l::FieldRef::Class(field)),
                    vec![base.clone()],
                    Some(ty.clone()),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(&pos, "deleted class field produced no owner"))?;
            owners.push((owner, ty, pos));
        }
        Ok(owners)
    }

    fn declared_hir_call_signature(
        &self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<(Vec<l::ValueType>, Option<l::ValueType>), LowerError> {
        let data_result = |ty: &Type| (*ty != Type::Void).then(|| l::ValueType::Data(ty.clone()));
        let data_params = |params: &[hir::Param]| {
            params
                .iter()
                .map(|parameter| l::ValueType::Data(parameter.ty.clone()))
                .collect::<Vec<_>>()
        };
        let foreign_params = |params: &[hir::Param]| {
            params
                .iter()
                .flat_map(|parameter| match &parameter.ty {
                    Type::Array(element) => vec![
                        l::ValueType::Address(l::AddressType {
                            pointee: (**element).clone(),
                            array_base: None,
                        }),
                        l::ValueType::Data(Type::I32),
                    ],
                    ty => vec![l::ValueType::Data(ty.clone())],
                })
                .collect::<Vec<_>>()
        };
        match callee {
            hir::Callee::Func(name) => {
                let function = self
                    .lowering
                    .hir
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .ok_or_else(|| self.error(&expr.pos, format!("missing body for `{name}`")))?;
                Ok((data_params(&function.params), data_result(&function.ret)))
            }
            hir::Callee::Foreign(name) => {
                let function = self
                    .lowering
                    .hir
                    .foreign_fns
                    .iter()
                    .find(|function| function.name == *name)
                    .ok_or_else(|| {
                        self.error(&expr.pos, format!("missing foreign declaration `{name}`"))
                    })?;
                Ok((foreign_params(&function.params), data_result(&function.ret)))
            }
            hir::Callee::Value(value) => {
                let Type::Func(signature) = &value.ty else {
                    return Err(self.error(&value.pos, "indirect callee is not function-typed"));
                };
                let mut parameters = vec![l::ValueType::Data(value.ty.clone())];
                parameters.extend(signature.params.iter().cloned().map(l::ValueType::Data));
                Ok((parameters, data_result(&signature.ret)))
            }
            hir::Callee::Method { recv, name } => {
                if let Type::Class(class_id) = recv.ty {
                    let class =
                        self.lowering.hir.classes.get(class_id.0).ok_or_else(|| {
                            self.error(&recv.pos, "method receiver class is missing")
                        })?;
                    let method = class
                        .methods
                        .iter()
                        .find(|method| method.name == *name)
                        .ok_or_else(|| self.error(&expr.pos, "method body is missing"))?;
                    let receiver = if class.is_value {
                        l::ValueType::Address(l::AddressType {
                            pointee: Type::Class(class_id),
                            array_base: None,
                        })
                    } else {
                        l::ValueType::Data(Type::Class(class_id))
                    };
                    let mut parameters = vec![receiver];
                    parameters.extend(data_params(&method.params));
                    return Ok((parameters, data_result(&method.ret)));
                }
                let mut parameters = vec![l::ValueType::Data(recv.ty.clone())];
                parameters.extend(
                    args.iter()
                        .map(|argument| l::ValueType::Data(argument.ty.clone())),
                );
                Ok((parameters, data_result(&expr.ty)))
            }
            _ => Ok((
                args.iter()
                    .map(|argument| l::ValueType::Data(argument.ty.clone()))
                    .collect(),
                data_result(&expr.ty),
            )),
        }
    }

    fn resolve_call(
        &mut self,
        callee: &hir::Callee,
        expr: &hir::Expr,
    ) -> Result<CallResolution, LowerError> {
        match callee {
            hir::Callee::Func(name) => {
                let record = self
                    .lowering
                    .free_functions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown function `{name}`")))?;
                let function = self
                    .lowering
                    .hir
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .cloned()
                    .ok_or_else(|| self.error(&expr.pos, format!("missing body for `{name}`")))?;
                Ok((
                    l::CallTargetKind::Function(record.id),
                    Vec::new(),
                    function
                        .params
                        .iter()
                        .map(CallParam::from)
                        .collect::<Vec<_>>(),
                    None,
                ))
            }
            hir::Callee::Foreign(name) => {
                let id = self
                    .lowering
                    .foreign_functions
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown foreign `{name}`")))?;
                let params = self
                    .lowering
                    .hir
                    .foreign_fns
                    .iter()
                    .find(|function| function.name == *name)
                    .map(|function| function.params.iter().map(CallParam::from).collect())
                    .ok_or_else(|| {
                        self.error(&expr.pos, format!("missing foreign declaration `{name}`"))
                    })?;
                Ok((l::CallTargetKind::Foreign(id), Vec::new(), params, None))
            }
            hir::Callee::Value(value) => {
                let callee_value = self.require_expr(value)?;
                let Type::Func(signature) = &value.ty else {
                    return Err(self.error(&value.pos, "indirect callee is not function-typed"));
                };
                let params = signature
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| CallParam {
                        name: format!("arg{index}"),
                        ty: ty.clone(),
                        default: None,
                        pos: value.pos.clone(),
                    })
                    .collect();
                Ok((
                    l::CallTargetKind::Indirect,
                    vec![callee_value],
                    params,
                    None,
                ))
            }
            hir::Callee::Method { recv, name } => self.resolve_method_call(recv, name, expr),
            hir::Callee::Ambient(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Ambient,
                intrinsic_index(&hir::AmbientFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::ContextBytes { function, ty } => Ok(intrinsic_resolution(
                l::IntrinsicFamily::ContextBytes,
                intrinsic_index(&hir::ContextBytesFn::ALL, function),
                Some(ty.clone()),
                None,
            )),
            hir::Callee::Math(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Math,
                intrinsic_index(&hir::MathFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Num(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Number,
                intrinsic_index(&hir::NumFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Date(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Date,
                intrinsic_index(&hir::DateFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Json(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Json,
                intrinsic_index(&hir::JsonFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Str(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::String,
                intrinsic_index(&hir::StrFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Regex(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Regex,
                intrinsic_index(&hir::RegexFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Arr(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Array,
                intrinsic_index(&hir::ArrFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Map(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Map,
                intrinsic_index(&hir::MapFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Set(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Set,
                intrinsic_index(&hir::SetFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Worker(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Worker,
                intrinsic_index(&hir::WorkerFn::ALL, &value.intrinsic_identity()),
                None,
                match value {
                    hir::WorkerFn::Spawn(index) => Some(*index as u32),
                    _ => None,
                },
            )),
        }
    }

    fn resolve_method_call(
        &mut self,
        recv: &hir::Expr,
        name: &str,
        expr: &hir::Expr,
    ) -> Result<CallResolution, LowerError> {
        if let Type::Class(class_id) = recv.ty {
            let class = self
                .lowering
                .hir
                .classes
                .get(class_id.0)
                .cloned()
                .ok_or_else(|| self.error(&recv.pos, "method receiver class is missing"))?;
            let record = self
                .lowering
                .methods
                .get(&(class_id.0, name.to_string()))
                .cloned()
                .ok_or_else(|| {
                    self.error(
                        &expr.pos,
                        format!("class `{}` has no resolved method `{name}`", class.name),
                    )
                })?;
            let method = class
                .methods
                .iter()
                .find(|method| method.name == name)
                .cloned()
                .ok_or_else(|| self.error(&expr.pos, "method body is missing"))?;
            let (receiver, prepared) = if class.is_value {
                if is_place_expr(recv) {
                    let place = self.prepare_place(recv)?;
                    let placeholder = self.materialize_address(&place, &recv.pos)?;
                    (placeholder, Some(PreparedBase::Place(Box::new(place))))
                } else {
                    let value = self.require_expr(recv)?;
                    let address = match self.operand_type(&value, &recv.pos)? {
                        l::ValueType::Address(_) => value,
                        l::ValueType::Data(Type::Class(id)) if id == class_id => self
                            .emit(
                                l::InstructionKind::AddressOfValue,
                                vec![value],
                                Some(l::ValueType::Address(l::AddressType {
                                    pointee: Type::Class(class_id),
                                    array_base: None,
                                })),
                                false,
                                Vec::new(),
                                recv.pos.clone(),
                            )?
                            .expect("temporary value-class address"),
                        other => {
                            return Err(self.error(
                                &recv.pos,
                                format!("value-class receiver has invalid LIR type {other:?}"),
                            ));
                        }
                    };
                    (address.clone(), Some(PreparedBase::Value(address)))
                }
            } else {
                (self.require_expr(recv)?, None)
            };
            return Ok((
                l::CallTargetKind::Method(record.method.expect("method id")),
                vec![receiver],
                method.params.iter().map(CallParam::from).collect(),
                prepared,
            ));
        }
        let builtin = match (&recv.ty, name) {
            (Type::Array(_), "push") => l::BuiltinMethod::ArrayPush,
            (Type::Array(_), "pop") => l::BuiltinMethod::ArrayPop,
            (Type::Str, "slice") => l::BuiltinMethod::StringSlice,
            (Type::Generator(_), "next") => l::BuiltinMethod::GeneratorNext,
            _ => {
                return Err(self.error(
                    &expr.pos,
                    format!("unrepresented built-in method `{name}` on `{}`", recv.ty),
                ));
            }
        };
        Ok((
            l::CallTargetKind::BuiltinMethod(builtin),
            vec![self.require_expr(recv)?],
            Vec::new(),
            None,
        ))
    }

    fn lower_call_arguments(
        &mut self,
        params: &[CallParam],
        args: &[hir::Expr],
        receiver: Option<&PreparedBase>,
        foreign: bool,
    ) -> Result<Vec<l::Operand>, LowerError> {
        let mut operand_groups = Vec::with_capacity(params.len());
        let mut delayed_array_snapshots = Vec::new();
        let mut substitutions = HashMap::new();
        for (index, parameter) in params.iter().enumerate() {
            let value = if let Some(argument) = args.get(index) {
                if foreign {
                    self.lower_foreign_argument_value(&parameter.ty, argument)?
                } else {
                    self.lower_argument_value(&parameter.ty, argument)?
                }
            } else {
                let default = parameter.default.as_ref().ok_or_else(|| {
                    self.error(
                        &parameter.pos,
                        format!("missing argument `{}` with no default", parameter.name),
                    )
                })?;
                self.substitutions.push(substitutions.clone());
                let saved_this = self.this_value.clone();
                if let Some(receiver) = receiver {
                    self.this_value = Some(match receiver {
                        PreparedBase::Value(value) => value.clone(),
                        PreparedBase::Place(place) => {
                            let address =
                                self.materialize_address_inner(place, &default.pos, false)?;
                            self.emit(
                                l::InstructionKind::LoadAddress,
                                vec![address],
                                Some(l::ValueType::Data(self.place_type(place).clone())),
                                false,
                                Vec::new(),
                                default.pos.clone(),
                            )?
                            .expect("default receiver load")
                        }
                    });
                }
                let lowered = self.lower_stored_expr_at(&parameter.ty, default, &parameter.pos);
                self.this_value = saved_this;
                self.substitutions.pop();
                lowered?
            };
            let actual = self.operand_type(&value, &parameter.pos)?;
            let expected = l::ValueType::Data(parameter.ty.clone());
            let value = if actual == expected
                || foreign && self.foreign_boundary_pointer_representation(&parameter.ty, &actual)
            {
                value
            } else {
                self.coerce_operand(value, expected, &parameter.pos)?
            };
            substitutions.insert(parameter.name.clone(), value.clone());
            if foreign {
                if let Type::Array(element) = &parameter.ty {
                    let pos = args
                        .get(index)
                        .map_or_else(|| parameter.pos.clone(), |argument| argument.pos.clone());
                    delayed_array_snapshots.push((index, value, (**element).clone(), pos));
                    operand_groups.push(Vec::new());
                    continue;
                }
            }
            operand_groups.push(vec![value]);
        }
        if args.len() > params.len() {
            return Err(self.error(
                &args[params.len()].pos,
                format!(
                    "call has {} checked arguments but target has {} parameters",
                    args.len(),
                    params.len()
                ),
            ));
        }
        for (index, value, element, pos) in delayed_array_snapshots {
            operand_groups[index] = self.foreign_array_snapshot(value, &element, pos)?.to_vec();
        }
        Ok(operand_groups.into_iter().flatten().collect())
    }

    fn foreign_array_snapshot(
        &mut self,
        value: l::Operand,
        element: &Type,
        pos: Pos,
    ) -> Result<[l::Operand; 2], LowerError> {
        let data = self
            .emit(
                l::InstructionKind::ForeignArrayData,
                vec![value.clone()],
                Some(l::ValueType::Address(l::AddressType {
                    pointee: element.clone(),
                    array_base: None,
                })),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("foreign array data snapshot");
        let count = self
            .emit(
                l::InstructionKind::Length,
                vec![value],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                pos,
            )?
            .expect("foreign array count snapshot");
        Ok([data, count])
    }

    fn lower_new(
        &mut self,
        class_id: ClassId,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let class = self
            .lowering
            .hir
            .classes
            .get(class_id.0)
            .cloned()
            .ok_or_else(|| self.error(&expr.pos, "constructed class id is missing"))?;
        let allocation_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
            .into_iter()
            .filter(|trap| trap.kind == l::TrapKind::Allocation && trap.pos == expr.pos)
            .collect();
        let allocated = self
            .emit(
                l::InstructionKind::AllocateClass(class_id),
                Vec::new(),
                Some(self.allocated_type(class_id, &expr.pos)?),
                false,
                allocation_traps,
                expr.pos.clone(),
            )?
            .expect("class allocation");

        let mut constructor_args = Vec::new();
        if let Some(constructor) = &class.ctor {
            let receiver = PreparedBase::Value(allocated.clone());
            let params = constructor
                .params
                .iter()
                .map(CallParam::from)
                .collect::<Vec<_>>();
            constructor_args = self.lower_call_arguments(&params, args, Some(&receiver), false)?;
        }
        for (index, field) in class.fields.iter().enumerate() {
            if let Some(initializer) = &field.init {
                let saved_this = self.this_value.replace(allocated.clone());
                let value = self.lower_stored_expr_at(&field.ty, initializer, &field.pos);
                self.this_value = saved_this;
                let value = value?;
                self.store_class_field(class_id, index, allocated.clone(), value, &field.pos)?;
            }
        }
        if class.is_boundary {
            if args.len() != class.fields.len() {
                return Err(self.error(
                    &expr.pos,
                    format!(
                        "boundary class `{}` has {} fields but {} constructor arguments",
                        class.name,
                        class.fields.len(),
                        args.len()
                    ),
                ));
            }
            for (index, (field, argument)) in class.fields.iter().zip(args).enumerate() {
                let value = self.lower_argument_value(&field.ty, argument)?;
                self.store_class_field(class_id, index, allocated.clone(), value, &argument.pos)?;
            }
        }
        if let Some(constructor) = &class.ctor {
            let record =
                self.lowering
                    .method_record(class_id.0, "constructor", &constructor.pos)?;
            let mut operands = vec![allocated.clone()];
            operands.extend(constructor_args);
            let receiver_type = if class.is_value {
                l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(class_id),
                    array_base: None,
                })
            } else {
                l::ValueType::Data(Type::Class(class_id))
            };
            let parameter_types = std::iter::once(receiver_type)
                .chain(
                    constructor
                        .params
                        .iter()
                        .map(|parameter| l::ValueType::Data(parameter.ty.clone())),
                )
                .collect();
            let call_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
                .into_iter()
                .filter(|trap| trap.kind == l::TrapKind::Call)
                .collect();
            let stored = constructor
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| StoredOperand {
                    index: index + 1,
                    ty: l::ValueType::Data(parameter.ty.clone()),
                    action: OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument),
                    pos: args
                        .get(index)
                        .map_or_else(|| parameter.pos.clone(), |argument| argument.pos.clone()),
                })
                .collect();
            self.emit_store_instruction(
                l::InstructionKind::Call(l::CallTarget {
                    kind: l::CallTargetKind::Method(record.method.expect("constructor method id")),
                    parameter_types,
                    return_type: None,
                }),
                operands,
                stored,
                (None, true),
                call_traps,
                expr.pos.clone(),
            )?;
        } else if !class.is_boundary && !args.is_empty() {
            return Err(self.error(
                &expr.pos,
                format!(
                    "class `{}` has no constructor but received {} arguments",
                    class.name,
                    args.len()
                ),
            ));
        }
        if class.is_value {
            self.emit(
                l::InstructionKind::LoadAddress,
                vec![allocated],
                Some(l::ValueType::Data(Type::Class(class_id))),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .ok_or_else(|| self.error(&expr.pos, "value construction produced no value"))
        } else {
            Ok(allocated)
        }
    }

    fn lower_descriptor(
        &mut self,
        class_id: ClassId,
        slots: &[Option<hir::Expr>],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let class = self
            .lowering
            .hir
            .classes
            .get(class_id.0)
            .cloned()
            .ok_or_else(|| self.error(&expr.pos, "descriptor class id is missing"))?;
        if !class.is_descriptor || class.is_value || slots.len() != class.fields.len() {
            return Err(self.error(
                &expr.pos,
                format!("invalid descriptor construction for `{}`", class.name),
            ));
        }
        let allocated = self
            .emit(
                l::InstructionKind::AllocateClass(class_id),
                Vec::new(),
                Some(l::ValueType::Data(Type::Class(class_id))),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?
            .expect("descriptor allocation");
        for (index, (slot, field)) in slots.iter().zip(&class.fields).enumerate() {
            let value = if let Some(value) = slot {
                self.lower_stored_expr(&field.ty, value)?
            } else if field.is_absence_capable {
                let Type::StringAlias(alias) = field.ty else {
                    return Err(
                        self.error(&field.pos, "absence-capable field is not a string alias")
                    );
                };
                let discriminant = self
                    .lowering
                    .hir
                    .string_aliases
                    .get(alias.0)
                    .map(hir::StringAliasDef::absence_discriminant)
                    .ok_or_else(|| self.error(&field.pos, "absence alias is missing"))?;
                l::Operand::Constant(l::Constant {
                    ty: field.ty.clone(),
                    kind: l::ConstantKind::Integer(discriminant),
                })
            } else {
                let default = field.init.as_ref().ok_or_else(|| {
                    self.error(
                        &field.pos,
                        format!("descriptor field `{}` has no value or default", field.name),
                    )
                })?;
                let saved_this = self.this_value.replace(allocated.clone());
                let value = self.lower_stored_expr(&field.ty, default);
                self.this_value = saved_this;
                value?
            };
            self.store_class_field(class_id, index, allocated.clone(), value, &field.pos)?;
        }
        Ok(allocated)
    }

    fn store_class_field(
        &mut self,
        class: ClassId,
        index: usize,
        object: l::Operand,
        value: l::Operand,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let definition = self
            .lowering
            .hir
            .classes
            .get(class.0)
            .and_then(|class| class.fields.get(index))
            .ok_or_else(|| self.error(pos, "class field index is missing"))?;
        let field = self
            .lowering
            .fields
            .get(&(class.0, definition.name.clone()))
            .copied()
            .ok_or_else(|| self.error(pos, "class field id is missing"))?;
        let value = self.coerce_operand(value, l::ValueType::Data(definition.ty.clone()), pos)?;
        let base_type = self.operand_type(&object, pos)?;
        let array_base = match base_type {
            l::ValueType::Address(address) => address.array_base,
            _ => None,
        };
        let address = self
            .emit(
                l::InstructionKind::AddressOfField(l::FieldRef::Class(field)),
                vec![object],
                Some(l::ValueType::Address(l::AddressType {
                    pointee: definition.ty.clone(),
                    array_base,
                })),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("field address");
        self.emit_store_instruction(
            l::InstructionKind::StoreAddress,
            vec![address, value],
            vec![StoredOperand {
                index: 1,
                ty: l::ValueType::Data(definition.ty.clone()),
                action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                pos: pos.clone(),
            }],
            (None, false),
            Vec::new(),
            pos.clone(),
        )?;
        Ok(())
    }

    fn lower_argument_value(
        &mut self,
        expected: &Type,
        argument: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        if self.embedded_header_extension(expected, argument).is_some() {
            self.lower_stored_expr(expected, argument)
        } else {
            self.require_expr(argument)
        }
    }

    fn lower_foreign_argument_value(
        &mut self,
        expected: &Type,
        argument: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        if self.embedded_header_extension(expected, argument).is_some() {
            return self.lower_stored_expr(expected, argument);
        }
        if !self.is_boundary_box_narrowing(expected, &argument.ty) {
            return self.require_expr(argument);
        }
        let value = if is_place_expr(argument) {
            let place = self.prepare_place(argument)?;
            self.materialize_address_inner(&place, &argument.pos, false)?
        } else {
            self.require_expr(argument)?
        };
        match self.operand_type(&value, &argument.pos)? {
            l::ValueType::Address(_) | l::ValueType::Data(Type::Nullable(_)) => Ok(value),
            l::ValueType::Data(Type::Class(class))
                if expected == &Type::Nullable(Box::new(Type::Class(class))) =>
            {
                self.emit(
                    l::InstructionKind::AddressOfValue,
                    vec![value],
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: Type::Class(class),
                        array_base: None,
                    })),
                    false,
                    Vec::new(),
                    argument.pos.clone(),
                )?
                .ok_or_else(|| self.error(&argument.pos, "foreign argument address is missing"))
            }
            other => Err(self.error(
                &argument.pos,
                format!("nullable boundary argument has invalid LIR type {other:?}"),
            )),
        }
    }

    fn lower_lambda(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[hir::Capture],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        self.lower_lambda_with_id(params, ret, body, captures, expr)
            .map(|(_, closure)| closure)
    }

    fn lower_lambda_with_id(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[hir::Capture],
        expr: &hir::Expr,
    ) -> Result<(l::FunctionId, l::Operand), LowerError> {
        let capture_values = captures
            .iter()
            .map(|capture| {
                let binding = self.lookup_binding(&capture.name, &expr.pos)?;
                self.read_binding(binding, &expr.pos)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let id = self.lowering.allocate_function_id();
        let function = FunctionInput {
            name: format!(
                "<lambda {}:{}:{}>",
                expr.pos.file, expr.pos.line, expr.pos.col
            ),
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            params: params.to_vec(),
            ret: ret.clone(),
            body: body.to_vec(),
            pos: expr.pos.clone(),
        };
        self.lowering.lower_function_input(
            id,
            function,
            l::FunctionKind::Lambda,
            None,
            captures.to_vec(),
        )?;
        let closure = self
            .emit(
                l::InstructionKind::MakeClosure(id),
                capture_values,
                Some(l::ValueType::Data(expr.ty.clone())),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?
            .ok_or_else(|| self.error(&expr.pos, "lambda produced no function value"))?;
        Ok((id, closure))
    }

    fn resolve_async_target(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        return_type: Option<l::ValueType>,
        pos: &Pos,
    ) -> Result<(l::CallTarget, Vec<l::Operand>, Vec<StoredOperand>), LowerError> {
        let (kind, mut operands, params) = match callee {
            hir::AsyncCallee::Function(name) => {
                let record = self
                    .lowering
                    .free_functions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| self.error(pos, format!("unknown async function `{name}`")))?;
                let function = self
                    .lowering
                    .hir
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .cloned()
                    .ok_or_else(|| self.error(pos, "async function body is missing"))?;
                (
                    l::CallTargetKind::Function(record.id),
                    Vec::new(),
                    function
                        .params
                        .iter()
                        .map(CallParam::from)
                        .collect::<Vec<_>>(),
                )
            }
            hir::AsyncCallee::Method {
                class,
                receiver,
                name,
            } => {
                let record = self.lowering.method_record(class.0, name, pos)?;
                let function = self
                    .lowering
                    .hir
                    .classes
                    .get(class.0)
                    .and_then(|class| class.methods.iter().find(|method| method.name == *name))
                    .cloned()
                    .ok_or_else(|| self.error(pos, "async method body is missing"))?;
                (
                    l::CallTargetKind::Method(record.method.expect("async method id")),
                    vec![self.require_expr(receiver)?],
                    function
                        .params
                        .iter()
                        .map(CallParam::from)
                        .collect::<Vec<_>>(),
                )
            }
        };
        let mut parameter_types = if let hir::AsyncCallee::Method { class, .. } = callee {
            vec![l::ValueType::Data(Type::Class(*class))]
        } else {
            Vec::new()
        };
        parameter_types.extend(
            params
                .iter()
                .map(|parameter| l::ValueType::Data(parameter.ty.clone())),
        );
        let explicit_offset = operands.len();
        operands.extend(self.lower_call_arguments(&params, args, None, false)?);
        let target = l::CallTarget {
            kind,
            parameter_types,
            return_type,
        };
        let stored = params
            .iter()
            .enumerate()
            .map(|(index, parameter)| StoredOperand {
                index: explicit_offset + index,
                ty: l::ValueType::Data(parameter.ty.clone()),
                action: OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument),
                pos: args
                    .get(index)
                    .map_or_else(|| parameter.pos.clone(), |argument| argument.pos.clone()),
            })
            .collect();
        Ok((target, operands, stored))
    }

    fn lower_async_call(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let return_type = (expr.ty != Type::Void).then(|| l::ValueType::Data(expr.ty.clone()));
        let (target, operands, stored) =
            self.resolve_async_target(callee, args, return_type.clone(), &expr.pos)?;
        self.acquire_stored_operands(&operands, stored)?;
        let typed_operands = operands
            .into_iter()
            .map(|operand| self.terminator_value(operand, &expr.pos))
            .collect::<Result<Vec<_>, _>>()?;
        let successor = self.new_block(
            return_type.clone().into_iter().collect(),
            Some("async-call.resume".to_string()),
        );
        let resume_value = return_type
            .as_ref()
            .map(|_| self.blocks[successor.0 as usize].parameters[0]);
        self.terminate(
            l::Terminator::Suspend {
                kind: l::SuspendKind::AsyncCall {
                    target,
                    operands: typed_operands,
                },
                pos: expr.pos.clone(),
                successor,
                resume_value,
                arguments: Vec::new(),
                invalidates: self.array_values.clone(),
                traps: convert_traps(&expr.trap_sites(self.lowering.hir)),
            },
            &expr.pos,
        )?;
        self.current = Some(successor);
        Ok(resume_value.map(l::Operand::Value))
    }

    fn lower_async_handle_create(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let Type::AsyncHandle(value) = &expr.ty else {
            return Err(self.error(&expr.pos, "async handle creation has a non-handle type"));
        };
        let return_type = (**value != Type::Void).then(|| l::ValueType::Data((**value).clone()));
        let (target, operands, stored) =
            self.resolve_async_target(callee, args, return_type, &expr.pos)?;
        self.emit_store_instruction(
            l::InstructionKind::AsyncHandleCreate(target),
            operands,
            stored,
            (Some(l::ValueType::Data(expr.ty.clone())), false),
            convert_traps(&expr.trap_sites(self.lowering.hir)),
            expr.pos.clone(),
        )?
        .ok_or_else(|| self.error(&expr.pos, "async handle creation produced no value"))
    }

    fn lower_async_handle_await(
        &mut self,
        handle: &hir::Expr,
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let handle = self.require_expr(handle)?;
        let handle = self.terminator_value(handle, &expr.pos)?;
        let return_type = (expr.ty != Type::Void).then(|| l::ValueType::Data(expr.ty.clone()));
        let successor = self.new_block(
            return_type.clone().into_iter().collect(),
            Some("async-handle.resume".to_string()),
        );
        let resume_value = return_type
            .as_ref()
            .map(|_| self.blocks[successor.0 as usize].parameters[0]);
        self.terminate(
            l::Terminator::Suspend {
                kind: l::SuspendKind::AsyncHandle { handle },
                pos: expr.pos.clone(),
                successor,
                resume_value,
                arguments: Vec::new(),
                invalidates: self.array_values.clone(),
                traps: convert_traps(&expr.trap_sites(self.lowering.hir)),
            },
            &expr.pos,
        )?;
        self.current = Some(successor);
        Ok(resume_value.map(l::Operand::Value))
    }

    fn terminator_value(
        &mut self,
        operand: l::Operand,
        pos: &Pos,
    ) -> Result<l::ValueId, LowerError> {
        let ty = self.operand_type(&operand, pos)?;
        let value = match operand {
            l::Operand::Value(value) => value,
            constant @ l::Operand::Constant(_) => {
                let materialized = self
                    .emit(
                        l::InstructionKind::Copy,
                        vec![constant],
                        Some(ty.clone()),
                        false,
                        Vec::new(),
                        pos.clone(),
                    )?
                    .expect("copy result");
                let l::Operand::Value(value) = materialized else {
                    unreachable!()
                };
                value
            }
        };
        Ok(value)
    }

    fn prepare_place(&mut self, expr: &hir::Expr) -> Result<PreparedPlace, LowerError> {
        self.prepare_place_with_traps(expr, None)
    }

    fn prepare_place_with_traps(
        &mut self,
        expr: &hir::Expr,
        traps: Option<Vec<l::Trap>>,
    ) -> Result<PreparedPlace, LowerError> {
        let mut traps = traps.unwrap_or_else(|| convert_traps(&expr.trap_sites(self.lowering.hir)));
        let kind = match &expr.kind {
            hir::ExprKind::Local(name) => {
                let binding = self.lookup_binding(name, &expr.pos)?;
                let ty = match &self.bindings[binding.0].ty {
                    l::ValueType::Data(ty) => ty.clone(),
                    other => {
                        return Err(self.error(
                            &expr.pos,
                            format!("local place has invalid LIR type {other:?}"),
                        ));
                    }
                };
                let local = self.bindings[binding.0].storage.ok_or_else(|| {
                    self.error(
                        &expr.pos,
                        format!("local `{name}` is used as an address but has no storage"),
                    )
                })?;
                PreparedPlaceKind::Local(local, ty)
            }
            hir::ExprKind::Global(name) => {
                let global = self
                    .lowering
                    .globals
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown global `{name}`")))?;
                let ty = self
                    .lowering
                    .hir
                    .globals
                    .iter()
                    .find(|definition| definition.name == *name)
                    .map(|definition| definition.ty.clone())
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown global `{name}`")))?;
                PreparedPlaceKind::Global(global, ty)
            }
            hir::ExprKind::Field { obj, name } => {
                let base = if is_stored_aggregate(self.lowering.hir, &obj.ty) && is_place_expr(obj)
                {
                    PreparedBase::Place(Box::new(self.prepare_place(obj)?))
                } else {
                    PreparedBase::Value(self.require_expr(obj)?)
                };
                let field = self.resolve_field(&obj.ty, name, &expr.pos)?;
                let ty = self.resolved_field_type(field, &obj.ty, &expr.pos)?;
                PreparedPlaceKind::Field { base, field, ty }
            }
            hir::ExprKind::Index {
                obj,
                index,
                checked,
            } => {
                let base = if matches!(obj.ty, Type::FixedArray(..)) && is_place_expr(obj) {
                    PreparedBase::Place(Box::new(self.prepare_place(obj)?))
                } else {
                    PreparedBase::Value(self.require_expr(obj)?)
                };
                let index = self.require_expr(index)?;
                PreparedPlaceKind::Index {
                    base,
                    index,
                    checked: *checked,
                    ty: self.indexed_element_type(&obj.ty, &expr.pos)?,
                }
            }
            hir::ExprKind::This if self.this_value.is_some() => {
                let value = self.this_value.clone().expect("this value");
                if matches!(
                    self.operand_type(&value, &expr.pos)?,
                    l::ValueType::Address(_)
                ) {
                    PreparedPlaceKind::ExistingAddress(value, expr.ty.clone())
                } else {
                    return Err(
                        self.error(&expr.pos, "reference `this` is not an addressable value")
                    );
                }
            }
            other => {
                return Err(self.error(
                    &expr.pos,
                    format!("assignment/mutable receiver is not an addressable form: {other:?}"),
                ));
            }
        };
        let mut nested = Vec::new();
        let base = match &kind {
            PreparedPlaceKind::Field { base, .. } | PreparedPlaceKind::Index { base, .. } => {
                Some(base)
            }
            PreparedPlaceKind::ExistingAddress(..)
            | PreparedPlaceKind::BoxedBoundary(..)
            | PreparedPlaceKind::Local(..)
            | PreparedPlaceKind::Global(..) => None,
        };
        if let Some(PreparedBase::Place(base)) = base {
            collect_place_traps(base, &mut nested);
        }
        for nested_trap in nested {
            if let Some(index) = traps.iter().position(|trap| *trap == nested_trap) {
                traps.remove(index);
            }
        }
        let place = PreparedPlace { kind, traps };
        let stored = self.place_type(&place).clone();
        if self.is_boundary_box_narrowing(&stored, &expr.ty) {
            let handle = self.load_place(&place, &expr.pos)?;
            return Ok(PreparedPlace {
                kind: PreparedPlaceKind::BoxedBoundary(handle, expr.ty.clone()),
                traps: Vec::new(),
            });
        }
        Ok(place)
    }

    fn materialize_address(
        &mut self,
        place: &PreparedPlace,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        self.materialize_address_inner(place, pos, true)
    }

    fn materialize_address_inner(
        &mut self,
        place: &PreparedPlace,
        pos: &Pos,
        include_traps: bool,
    ) -> Result<l::Operand, LowerError> {
        let traps = if include_traps {
            place.traps.clone()
        } else {
            Default::default()
        };
        match &place.kind {
            PreparedPlaceKind::ExistingAddress(address, _) => Ok(address.clone()),
            PreparedPlaceKind::BoxedBoundary(handle, _) => Ok(handle.clone()),
            PreparedPlaceKind::Local(local, ty) => self
                .emit(
                    l::InstructionKind::AddressOfLocal(*local),
                    Vec::new(),
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base: None,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "local address produced no value")),
            PreparedPlaceKind::Global(global, ty) => self
                .emit(
                    l::InstructionKind::AddressOfGlobal(*global),
                    Vec::new(),
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base: None,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "global address produced no value")),
            PreparedPlaceKind::Field { base, field, ty } => {
                let base = self.materialize_base(base, pos, include_traps)?;
                let array_base = address_base(&self.operand_type(&base, pos)?);
                self.emit(
                    l::InstructionKind::AddressOfField(*field),
                    vec![base],
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "field address produced no value"))
            }
            PreparedPlaceKind::Index {
                base,
                index,
                checked,
                ty,
            } => {
                let base = self.materialize_base(base, pos, include_traps)?;
                let base_type = self.operand_type(&base, pos)?;
                let array_base = match (&base, &base_type) {
                    (l::Operand::Value(value), l::ValueType::Data(Type::Array(_))) => Some(*value),
                    (_, l::ValueType::Address(address)) => address.array_base,
                    _ => None,
                };
                self.emit(
                    l::InstructionKind::AddressOfIndex {
                        checked: *checked && include_traps,
                    },
                    vec![base, index.clone()],
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "index address produced no value"))
            }
        }
    }

    fn materialize_base(
        &mut self,
        base: &PreparedBase,
        pos: &Pos,
        include_traps: bool,
    ) -> Result<l::Operand, LowerError> {
        match base {
            PreparedBase::Value(value) => Ok(value.clone()),
            PreparedBase::Place(place) => self.materialize_address_inner(place, pos, include_traps),
        }
    }

    fn load_place(&mut self, place: &PreparedPlace, pos: &Pos) -> Result<l::Operand, LowerError> {
        match &place.kind {
            PreparedPlaceKind::BoxedBoundary(handle, ty) => {
                self.coerce_operand(handle.clone(), l::ValueType::Data(ty.clone()), pos)
            }
            PreparedPlaceKind::Local(local, _) => self.load_local(*local, pos),
            PreparedPlaceKind::Global(global, ty) => self
                .emit(
                    l::InstructionKind::LoadGlobal(*global),
                    Vec::new(),
                    Some(l::ValueType::Data(ty.clone())),
                    false,
                    place.traps.clone(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "global load produced no value")),
            _ => {
                let ty = self.place_type(place).clone();
                let address = self.materialize_address(place, pos)?;
                self.emit(
                    l::InstructionKind::LoadAddress,
                    vec![address],
                    Some(l::ValueType::Data(ty)),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "address load produced no value"))
            }
        }
    }

    fn store_place(
        &mut self,
        place: &PreparedPlace,
        value: l::Operand,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let ty = l::ValueType::Data(self.place_type(place).clone());
        let value = self.coerce_operand(value, ty.clone(), pos)?;
        let counted = is_async_owner_type(&ty);
        match &place.kind {
            PreparedPlaceKind::Local(local, _) => {
                let old_owner = counted.then(|| self.load_local(*local, pos)).transpose()?;
                self.emit_store_instruction(
                    l::InstructionKind::StoreLocal(*local),
                    vec![value],
                    vec![StoredOperand {
                        index: 0,
                        ty: ty.clone(),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                        pos: pos.clone(),
                    }],
                    (None, false),
                    place.traps.clone(),
                    pos.clone(),
                )?;
                if let Some(old_owner) = old_owner {
                    self.release_owner(old_owner, &ty, pos)?;
                }
            }
            PreparedPlaceKind::Global(global, _) => {
                let old_owner = if counted {
                    self.emit(
                        l::InstructionKind::LoadGlobal(*global),
                        Vec::new(),
                        Some(ty.clone()),
                        false,
                        Vec::new(),
                        pos.clone(),
                    )?
                } else {
                    None
                };
                self.emit_store_instruction(
                    l::InstructionKind::StoreGlobal(*global),
                    vec![value],
                    vec![StoredOperand {
                        index: 0,
                        ty: ty.clone(),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                        pos: pos.clone(),
                    }],
                    (None, false),
                    place.traps.clone(),
                    pos.clone(),
                )?;
                if let Some(old_owner) = old_owner {
                    self.release_owner(old_owner, &ty, pos)?;
                }
            }
            _ => {
                let address = self.materialize_address(place, pos)?;
                let old_owner = if counted {
                    self.emit(
                        l::InstructionKind::LoadAddress,
                        vec![address.clone()],
                        Some(ty.clone()),
                        false,
                        Vec::new(),
                        pos.clone(),
                    )?
                } else {
                    None
                };
                self.emit_store_instruction(
                    l::InstructionKind::StoreAddress,
                    vec![address, value],
                    vec![StoredOperand {
                        index: 1,
                        ty: ty.clone(),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                        pos: pos.clone(),
                    }],
                    (None, false),
                    Vec::new(),
                    pos.clone(),
                )?;
                if let Some(old_owner) = old_owner {
                    self.release_owner(old_owner, &ty, pos)?;
                }
            }
        }
        Ok(())
    }

    fn place_type<'p>(&self, place: &'p PreparedPlace) -> &'p Type {
        match &place.kind {
            PreparedPlaceKind::ExistingAddress(_, ty)
            | PreparedPlaceKind::BoxedBoundary(_, ty)
            | PreparedPlaceKind::Local(_, ty)
            | PreparedPlaceKind::Global(_, ty)
            | PreparedPlaceKind::Field { ty, .. }
            | PreparedPlaceKind::Index { ty, .. } => ty,
        }
    }

    fn embedded_header_extension(
        &self,
        expected: &Type,
        expr: &hir::Expr,
    ) -> Option<(ClassId, ClassId)> {
        let header = boundary_box_class(self.lowering.hir, expected)?;
        let hir::ExprKind::Field { obj, name } = &expr.kind else {
            return None;
        };
        let Type::Class(extension) = obj.ty else {
            return None;
        };
        let definition = self.lowering.hir.classes.get(extension.0)?;
        let first = definition.fields.first()?;
        (definition.is_value
            && definition.is_boundary
            && first.name == *name
            && first.ty == Type::Class(header)
            && self
                .lowering
                .classes
                .get(header.0)
                .is_some_and(|header| header.is_embedded_header))
        .then_some((extension, header))
    }

    fn lower_stored_expr(
        &mut self,
        expected: &Type,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        self.lower_stored_expr_at(expected, expr, &expr.pos)
    }

    fn lower_stored_expr_at(
        &mut self,
        expected: &Type,
        expr: &hir::Expr,
        coercion_pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        if let Some((extension, header)) = self.embedded_header_extension(expected, expr) {
            let hir::ExprKind::Field { obj, .. } = &expr.kind else {
                unreachable!("embedded header projection is a field")
            };
            let value = self.require_expr(obj)?;
            let actual = self.operand_type(&value, &expr.pos)?;
            let expected_operand = l::ValueType::Data(Type::Class(extension));
            if actual != expected_operand {
                return Err(self.error(
                    &expr.pos,
                    format!(
                        "embedded header extension has LIR type {actual:?}, expected {expected_operand:?}"
                    ),
                ));
            }
            return self
                .emit(
                    l::InstructionKind::BoxBoundaryValue { payload: extension },
                    vec![value],
                    Some(l::ValueType::Data(Type::Nullable(Box::new(Type::Class(
                        header,
                    ))))),
                    false,
                    vec![l::Trap {
                        kind: l::TrapKind::Allocation,
                        pos: expr.pos.clone(),
                    }],
                    expr.pos.clone(),
                )?
                .ok_or_else(|| self.error(&expr.pos, "embedded header box produced no handle"));
        }
        let value = self.require_expr(expr)?;
        self.coerce_operand(value, l::ValueType::Data(expected.clone()), coercion_pos)
    }

    fn is_boundary_box_narrowing(&self, stored: &Type, narrowed: &Type) -> bool {
        boundary_box_class(self.lowering.hir, stored)
            .is_some_and(|class| narrowed == &Type::Class(class))
    }

    fn foreign_boundary_pointer_representation(
        &self,
        declared: &Type,
        actual: &l::ValueType,
    ) -> bool {
        let Some(class) = boundary_box_class(self.lowering.hir, declared) else {
            return false;
        };
        matches!(actual, l::ValueType::Address(address) if address.pointee == Type::Class(class))
    }

    fn allocated_type(&self, class: ClassId, pos: &Pos) -> Result<l::ValueType, LowerError> {
        let definition = self
            .lowering
            .hir
            .classes
            .get(class.0)
            .ok_or_else(|| self.error(pos, "allocated class id is missing"))?;
        Ok(if definition.is_value {
            l::ValueType::Address(l::AddressType {
                pointee: Type::Class(class),
                array_base: None,
            })
        } else {
            l::ValueType::Data(Type::Class(class))
        })
    }

    fn resolve_field(
        &self,
        object_type: &Type,
        name: &str,
        pos: &Pos,
    ) -> Result<l::FieldRef, LowerError> {
        match object_type {
            Type::Class(class) => self
                .lowering
                .fields
                .get(&(class.0, name.to_string()))
                .copied()
                .map(l::FieldRef::Class)
                .ok_or_else(|| {
                    self.error(
                        pos,
                        format!("class #{} has no resolved field `{name}`", class.0),
                    )
                }),
            Type::IterResult(_) if name == "done" => Ok(l::FieldRef::IterDone),
            Type::IterResult(_) if name == "value" => Ok(l::FieldRef::IterValue),
            _ => Err(self.error(
                pos,
                format!("field `{name}` on `{object_type}` has no LIR field id"),
            )),
        }
    }

    fn resolved_field_type(
        &self,
        field: l::FieldRef,
        object_type: &Type,
        pos: &Pos,
    ) -> Result<Type, LowerError> {
        match field {
            l::FieldRef::Class(field_id) => self
                .lowering
                .classes
                .iter()
                .flat_map(|class| &class.fields)
                .find(|field| field.id == field_id)
                .map(|field| field.ty.clone())
                .ok_or_else(|| self.error(pos, "resolved class field type is missing")),
            l::FieldRef::IterDone => Ok(Type::Bool),
            l::FieldRef::IterValue => match object_type {
                Type::IterResult(value) => Ok((**value).clone()),
                _ => Err(self.error(pos, "iterator value field has a non-iterator base")),
            },
        }
    }

    fn indexed_element_type(&self, ty: &Type, pos: &Pos) -> Result<Type, LowerError> {
        match ty {
            Type::Array(element) | Type::FixedArray(element, _) => Ok((**element).clone()),
            _ => Err(self.error(pos, format!("indexed base `{ty}` is not an array"))),
        }
    }

    fn lookup_substitution(&self, name: &str) -> Option<l::Operand> {
        self.substitutions
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn load_local(&mut self, local: l::LocalId, pos: &Pos) -> Result<l::Operand, LowerError> {
        let ty = self
            .locals
            .get(local.0 as usize)
            .map(|local| local.ty.clone())
            .ok_or_else(|| self.error(pos, format!("local {} is missing", local.0)))?;
        self.emit(
            l::InstructionKind::LoadLocal(local),
            Vec::new(),
            Some(ty),
            false,
            Vec::new(),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "local load produced no value"))
    }

    fn coerce_operand(
        &mut self,
        operand: l::Operand,
        expected: l::ValueType,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        let actual = self.operand_type(&operand, pos)?;
        if actual == expected {
            return Ok(operand);
        }
        if let l::ValueType::Data(Type::Nullable(target)) = &expected {
            if let Type::Class(target) = target.as_ref() {
                let boundary = self
                    .lowering
                    .hir
                    .classes
                    .get(target.0)
                    .is_some_and(|definition| definition.is_value && definition.is_boundary);
                if boundary {
                    let value = match &actual {
                        l::ValueType::Address(address)
                            if address.pointee == Type::Class(*target) =>
                        {
                            self.emit(
                                l::InstructionKind::LoadAddress,
                                vec![operand.clone()],
                                Some(l::ValueType::Data(Type::Class(*target))),
                                false,
                                Vec::new(),
                                pos.clone(),
                            )?
                            .expect("boundary value load")
                        }
                        l::ValueType::Data(Type::Class(source)) if source == target => {
                            operand.clone()
                        }
                        _ => {
                            return self
                                .emit(
                                    l::InstructionKind::Coerce,
                                    vec![operand],
                                    Some(expected),
                                    false,
                                    Vec::new(),
                                    pos.clone(),
                                )?
                                .ok_or_else(|| {
                                    self.error(pos, "implicit coercion produced no value")
                                });
                        }
                    };
                    if matches!(
                        self.operand_type(&value, pos)?,
                        l::ValueType::Data(Type::Class(source)) if source == *target
                    ) {
                        return self
                            .emit(
                                l::InstructionKind::BoxBoundaryValue { payload: *target },
                                vec![value],
                                Some(expected),
                                false,
                                vec![l::Trap {
                                    kind: l::TrapKind::Allocation,
                                    pos: pos.clone(),
                                }],
                                pos.clone(),
                            )?
                            .ok_or_else(|| {
                                self.error(pos, "boundary value box produced no handle")
                            });
                    }
                }
            }
        }
        let kind = match (&actual, &expected) {
            (l::ValueType::Address(address), l::ValueType::Data(result))
                if address.pointee == *result =>
            {
                l::InstructionKind::LoadAddress
            }
            _ => l::InstructionKind::Coerce,
        };
        self.emit(
            kind,
            vec![operand],
            Some(expected),
            false,
            Vec::new(),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "implicit coercion produced no value"))
    }
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
        hir::SpreadKind::MapKeys => l::SpreadKind::MapKeys,
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

/// Turns every value live across a suspend edge into an explicit successor
/// definition and repairs SSA at downstream joins. The input graph is already
/// ordinary SSA; a suspension is the additional definition boundary.
fn thread_suspension_live_ins(function: &mut l::Function) -> Result<(), LowerError> {
    let original_value_count = function.values.len();
    let live_in = lir_live_ins(function, original_value_count);
    for block in &mut function.blocks {
        let l::Terminator::Suspend {
            successor,
            invalidates,
            ..
        } = &mut block.terminator
        else {
            continue;
        };
        let successor_live_in = live_in
            .get(successor.0 as usize)
            .cloned()
            .unwrap_or_default();
        invalidates.retain(|value| successor_live_in.contains(value));
    }
    let mut value_origins = (0..original_value_count)
        .map(|index| l::ValueId(index as u32))
        .collect::<Vec<_>>();
    let block_count = function.blocks.len();
    let mut carried = vec![Vec::<(l::ValueId, l::ValueId)>::new(); block_count];
    let mut suspend_successors = BTreeSet::new();
    for block in &function.blocks {
        if let l::Terminator::Suspend { successor, .. } = block.terminator {
            suspend_successors.insert(successor);
        }
    }

    for successor in suspend_successors {
        let Some(destination) = function.blocks.get(successor.0 as usize) else {
            return Err(LowerError {
                pos: function.pos.clone(),
                message: format!("suspend successor block {} is missing", successor.0),
            });
        };
        let values = live_in
            .get(successor.0 as usize)
            .cloned()
            .unwrap_or_default();
        let destination_id = destination.id;
        for original in values {
            let definition = function
                .values
                .get(original.0 as usize)
                .cloned()
                .ok_or_else(|| LowerError {
                    pos: function.pos.clone(),
                    message: format!("live-in value {} is missing", original.0),
                })?;
            let parameter = l::ValueId(function.values.len() as u32);
            function.values.push(l::Value {
                id: parameter,
                ty: definition.ty,
                fresh_owner: definition.fresh_owner,
                source_name: definition.source_name,
            });
            value_origins.push(original);
            function.blocks[destination_id.0 as usize]
                .parameters
                .push(parameter);
            carried[destination_id.0 as usize].push((original, parameter));
        }
    }

    let origins = carried
        .iter()
        .flat_map(|values| values.iter().map(|(original, _)| *original))
        .collect::<BTreeSet<_>>();
    if origins.is_empty() {
        function.liveness = l::Liveness {
            live_ins: live_in
                .into_iter()
                .map(|values| values.into_iter().collect())
                .collect(),
            value_origins,
        };
        return Ok(());
    }

    let predecessors = predecessors(function);
    let reachable = reachable_blocks(function);
    let function_parameters = function
        .parameters
        .iter()
        .map(|parameter| parameter.value)
        .collect::<HashSet<_>>();

    for origin in origins {
        let mut special = vec![None; block_count];
        for (block_index, values) in carried.iter().enumerate() {
            special[block_index] = values
                .iter()
                .find_map(|(candidate, parameter)| (*candidate == origin).then_some(*parameter));
        }
        let mut merges = vec![None; block_count];
        let mut incoming = vec![None; block_count];
        let mut outgoing = vec![None; block_count];

        loop {
            let mut changed = false;
            for block_index in 0..block_count {
                if !reachable[block_index] {
                    continue;
                }
                let block_id = function.blocks[block_index].id;
                let block_parameter_definition = function.blocks[block_index]
                    .parameters
                    .iter()
                    .take_while(|value| (value.0 as usize) < original_value_count)
                    .any(|value| *value == origin);
                let function_parameter_definition =
                    block_id == function.entry && function_parameters.contains(&origin);
                let instruction_definition = function.blocks[block_index]
                    .instructions
                    .iter()
                    .any(|instruction| instruction.result == Some(origin));

                let next_in = if let Some(parameter) = special[block_index] {
                    Some(parameter)
                } else if block_parameter_definition || function_parameter_definition {
                    Some(origin)
                } else if let Some(parameter) = merges[block_index] {
                    Some(parameter)
                } else {
                    let pred_versions = predecessors[block_index]
                        .iter()
                        .filter(|predecessor| reachable[predecessor.0 as usize])
                        .map(|predecessor| outgoing[predecessor.0 as usize])
                        .collect::<Vec<_>>();
                    let versions = pred_versions.into_iter().flatten().collect::<BTreeSet<_>>();
                    if versions.is_empty() {
                        None
                    } else {
                        if versions.len() == 1 {
                            versions.first().copied()
                        } else if live_in[block_index].contains(&origin) {
                            let definition = function.values[origin.0 as usize].clone();
                            let parameter = l::ValueId(function.values.len() as u32);
                            function.values.push(l::Value {
                                id: parameter,
                                ty: definition.ty,
                                fresh_owner: definition.fresh_owner,
                                source_name: definition.source_name,
                            });
                            value_origins.push(origin);
                            function.blocks[block_index].parameters.push(parameter);
                            merges[block_index] = Some(parameter);
                            changed = true;
                            Some(parameter)
                        } else {
                            versions.first().copied()
                        }
                    }
                };
                let next_out = if block_parameter_definition
                    || function_parameter_definition
                    || instruction_definition
                {
                    Some(origin)
                } else {
                    next_in
                };
                if incoming[block_index] != next_in {
                    incoming[block_index] = next_in;
                    changed = true;
                }
                if outgoing[block_index] != next_out {
                    outgoing[block_index] = next_out;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for block_index in 0..block_count {
            if !reachable[block_index] {
                continue;
            }
            let block_id = function.blocks[block_index].id;
            let block_parameter_definition = function.blocks[block_index]
                .parameters
                .iter()
                .take_while(|value| (value.0 as usize) < original_value_count)
                .any(|value| *value == origin);
            let function_parameter_definition =
                block_id == function.entry && function_parameters.contains(&origin);
            let mut current = if block_parameter_definition || function_parameter_definition {
                Some(origin)
            } else {
                incoming[block_index]
            };

            let parameters = function.blocks[block_index].parameters.clone();
            for parameter in parameters {
                if parameter != origin {
                    replace_address_base(function, parameter, origin, current);
                }
            }
            let instruction_count = function.blocks[block_index].instructions.len();
            for instruction_index in 0..instruction_count {
                let instruction = &mut function.blocks[block_index].instructions[instruction_index];
                replace_operands(&mut instruction.operands, origin, current);
                replace_ids(&mut instruction.invalidates, origin, current);
                let result = instruction.result;
                if let Some(result) = result {
                    replace_address_base(function, result, origin, current);
                }
                if result == Some(origin) {
                    current = Some(origin);
                }
            }
            replace_terminator_uses(
                &mut function.blocks[block_index].terminator,
                origin,
                current,
            );
        }

        for (destination_index, merge) in merges.iter().enumerate().take(block_count) {
            let Some(_parameter) = merge else {
                continue;
            };
            let destination = function.blocks[destination_index].id;
            for (source_index, version) in outgoing.iter().copied().enumerate().take(block_count) {
                if !reachable[source_index] {
                    continue;
                }
                if let Some(version) = version {
                    append_normal_edge_argument(
                        &mut function.blocks[source_index].terminator,
                        destination,
                        l::Operand::Value(version),
                    );
                }
            }
        }

        for (source_index, version) in outgoing.iter().copied().enumerate().take(block_count) {
            let successor = match &function.blocks[source_index].terminator {
                l::Terminator::Suspend { successor, .. } => *successor,
                _ => continue,
            };
            if carried[successor.0 as usize]
                .iter()
                .any(|(candidate, _)| *candidate == origin)
            {
                let version = version.ok_or_else(|| LowerError {
                    pos: function.pos.clone(),
                    message: format!(
                        "value {} is live at suspend in block {} but has no reaching definition",
                        origin.0, function.blocks[source_index].id.0
                    ),
                })?;
                if let l::Terminator::Suspend { arguments, .. } =
                    &mut function.blocks[source_index].terminator
                {
                    arguments.push(l::Operand::Value(version));
                }
            }
        }
    }
    function.liveness = l::Liveness {
        live_ins: live_in
            .into_iter()
            .map(|values| values.into_iter().collect())
            .collect(),
        value_origins,
    };
    Ok(())
}

/// Computes the value live-ins with the lowering's single graph fixed point.
fn lir_live_ins(function: &l::Function, original_value_count: usize) -> Vec<BTreeSet<l::ValueId>> {
    let mut uses = vec![BTreeSet::new(); function.blocks.len()];
    let mut definitions = vec![BTreeSet::new(); function.blocks.len()];
    for block in &function.blocks {
        let index = block.id.0 as usize;
        definitions[index].extend(
            block
                .parameters
                .iter()
                .copied()
                .filter(|value| (value.0 as usize) < original_value_count),
        );
        for instruction in &block.instructions {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    if !definitions[index].contains(value) {
                        uses[index].insert(*value);
                    }
                }
            }
            if let Some(result) = instruction.result {
                definitions[index].insert(result);
            }
        }
        for value in terminator_values(&block.terminator) {
            if !definitions[index].contains(&value) {
                uses[index].insert(value);
            }
        }
    }

    let mut live_in = vec![BTreeSet::new(); function.blocks.len()];
    let mut live_out = vec![BTreeSet::new(); function.blocks.len()];
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let index = block.id.0 as usize;
            let next_out = successors(&block.terminator)
                .into_iter()
                .filter_map(|successor| live_in.get(successor.0 as usize))
                .flat_map(|values| values.iter().copied())
                .collect::<BTreeSet<_>>();
            let mut next_in = uses[index].clone();
            next_in.extend(
                next_out
                    .iter()
                    .filter(|value| !definitions[index].contains(value))
                    .copied(),
            );
            if live_out[index] != next_out {
                live_out[index] = next_out;
                changed = true;
            }
            if live_in[index] != next_in {
                live_in[index] = next_in;
                changed = true;
            }
        }
        if !changed {
            return live_in;
        }
    }
}

fn reachable_blocks(function: &l::Function) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    let mut queue = VecDeque::from([function.entry]);
    while let Some(block) = queue.pop_front() {
        let Some(mark) = reachable.get_mut(block.0 as usize) else {
            continue;
        };
        if *mark {
            continue;
        }
        *mark = true;
        if let Some(block) = function.blocks.get(block.0 as usize) {
            queue.extend(successors(&block.terminator));
        }
    }
    reachable
}

/// Marks storage that a resumed activation must read before any redefinition.
fn classify_local_storage(function: &mut l::Function) {
    let predecessors = predecessors(function);
    let dominators = dominators(function, &predecessors);
    let mut store_blocks = vec![BTreeSet::new(); function.locals.len()];
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                if let Some(stores) = store_blocks.get_mut(local.0 as usize) {
                    stores.insert(block.id);
                }
            }
        }
    }
    let required = function
        .locals
        .iter()
        .filter(|local| local_requires_frame(function, local.id, &store_blocks, &dominators))
        .map(|local| local.id)
        .collect::<HashSet<_>>();
    for local in &mut function.locals {
        local.storage = if required.contains(&local.id) {
            l::LocalStorageClass::Frame
        } else {
            l::LocalStorageClass::Activation
        };
    }
}

fn local_requires_frame(
    function: &l::Function,
    local: l::LocalId,
    store_blocks: &[BTreeSet<l::BlockId>],
    dominators: &[BTreeSet<l::BlockId>],
) -> bool {
    let Some(store_blocks) = store_blocks.get(local.0 as usize) else {
        return false;
    };
    function.blocks.iter().any(|suspend| {
        let l::Terminator::Suspend { successor, .. } = suspend.terminator else {
            return false;
        };
        let definition_dominates = store_blocks.iter().any(|definition| {
            *definition == suspend.id
                || dominators
                    .get(suspend.id.0 as usize)
                    .is_some_and(|blocks| blocks.contains(definition))
        });
        definition_dominates && local_read_before_redefinition(function, successor, local)
    })
}

fn local_read_before_redefinition(
    function: &l::Function,
    start: l::BlockId,
    local: l::LocalId,
) -> bool {
    let mut pending = VecDeque::from([start]);
    let mut visited = HashSet::new();
    while let Some(block) = pending.pop_front() {
        if !visited.insert(block) {
            continue;
        }
        let Some(block) = function.blocks.get(block.0 as usize) else {
            continue;
        };
        let mut redefined = false;
        for instruction in &block.instructions {
            match instruction.kind {
                l::InstructionKind::LoadLocal(id) | l::InstructionKind::AddressOfLocal(id)
                    if id == local =>
                {
                    return true;
                }
                l::InstructionKind::StoreLocal(id) if id == local => {
                    redefined = true;
                    break;
                }
                _ => {}
            }
        }
        if !redefined {
            pending.extend(successors(&block.terminator));
        }
    }
    false
}

fn replace_address_base(
    function: &mut l::Function,
    value: l::ValueId,
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    let Some(replacement) = replacement else {
        return;
    };
    if let Some(l::Value {
        ty: l::ValueType::Address(address),
        ..
    }) = function.values.get_mut(value.0 as usize)
    {
        if address.array_base == Some(original) {
            address.array_base = Some(replacement);
        }
    }
}

fn replace_operands(
    operands: &mut [l::Operand],
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    let Some(replacement) = replacement else {
        return;
    };
    for operand in operands {
        if matches!(operand, l::Operand::Value(value) if *value == original) {
            *operand = l::Operand::Value(replacement);
        }
    }
}

fn replace_ids(values: &mut [l::ValueId], original: l::ValueId, replacement: Option<l::ValueId>) {
    let Some(replacement) = replacement else {
        return;
    };
    for value in values {
        if *value == original {
            *value = replacement;
        }
    }
}

fn replace_terminator_uses(
    terminator: &mut l::Terminator,
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    if let Some(replacement) = replacement {
        // A replacement must update invalidation mentions with the read uses.
        terminator.map_values(|value| {
            if value == original {
                replacement
            } else {
                value
            }
        });
    }
}

fn append_normal_edge_argument(
    terminator: &mut l::Terminator,
    destination: l::BlockId,
    argument: l::Operand,
) {
    let append = |target: &mut l::BlockTarget| {
        if target.block == destination {
            target.arguments.push(argument.clone());
        }
    };
    match terminator {
        l::Terminator::Branch(target) => append(target),
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => {
            append(then_target);
            append(else_target);
        }
        l::Terminator::Switch { arms, default, .. } => {
            for arm in arms {
                append(&mut arm.target);
            }
            append(default);
        }
        l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_)
        | l::Terminator::Suspend { .. } => {}
    }
}

// The verifier implementation follows the lowering helpers so its graph
// algorithms can stay private to this module.
fn verify_function(module: &l::Module, function: &l::Function, errors: &mut Vec<VerifyError>) {
    verify_structure_and_types(module, function, errors);
    verify_counted_stores(function, errors);
    verify_dominance(function, errors);
    verify_address_invalidation(function, errors);
}

fn verify_counted_stores(function: &l::Function, errors: &mut Vec<VerifyError>) {
    let use_counts = value_use_counts(function);
    let fresh = function
        .values
        .iter()
        .map(|value| value.fresh_owner)
        .collect::<Vec<_>>();

    for block in &function.blocks {
        let mut retains = HashMap::<l::ValueId, usize>::new();
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if matches!(
                instruction.kind,
                l::InstructionKind::AsyncHandleRetain | l::InstructionKind::AsyncHandleArrayRetain
            ) {
                if let Some(l::Operand::Value(value)) = instruction.operands.first() {
                    *retains.entry(*value).or_default() += 1;
                }
            }
            for (operand_index, operand) in counted_instruction_stores(function, instruction) {
                verify_counted_store_operand(
                    function,
                    block.id,
                    &format!("instruction {instruction_index}"),
                    operand_index,
                    operand,
                    &use_counts,
                    &fresh,
                    &mut retains,
                    errors,
                );
            }
        }
        for (operand_index, operand) in counted_terminator_stores(function, &block.terminator) {
            verify_counted_store_operand(
                function,
                block.id,
                "terminator",
                operand_index,
                &operand,
                &use_counts,
                &fresh,
                &mut retains,
                errors,
            );
        }
    }
}

// Keeping the complete store-site context explicit makes every verifier
// diagnostic identify the exact counted operand that violated the invariant.
#[allow(clippy::too_many_arguments)]
fn verify_counted_store_operand(
    function: &l::Function,
    block: l::BlockId,
    site: &str,
    operand_index: usize,
    operand: &l::Operand,
    use_counts: &[usize],
    fresh: &[bool],
    retains: &mut HashMap<l::ValueId, usize>,
    errors: &mut Vec<VerifyError>,
) {
    let l::Operand::Value(value) = operand else {
        errors.push(finding(
            function,
            format!(
                "block {} {site} stores counted operand {operand_index} ({operand:?}) without an owner",
                block.0
            ),
        ));
        return;
    };
    let single_use_fresh = fresh.get(value.0 as usize).copied().unwrap_or(false)
        && use_counts.get(value.0 as usize).copied() == Some(1);
    if single_use_fresh {
        return;
    }
    if let Some(available) = retains.get_mut(value) {
        if *available != 0 {
            *available -= 1;
            return;
        }
    }
    errors.push(finding(
        function,
        format!(
            "block {} {site} stores counted operand {operand_index} (value {}) without a fresh single-use owner or a preceding retain",
            block.0, value.0
        ),
    ));
}

fn counted_instruction_stores<'i>(
    function: &l::Function,
    instruction: &'i l::Instruction,
) -> Vec<(usize, &'i l::Operand)> {
    let start = match &instruction.kind {
        l::InstructionKind::StoreLocal(_) | l::InstructionKind::StoreGlobal(_) => Some(0),
        l::InstructionKind::StoreAddress => Some(1),
        l::InstructionKind::ArrayLiteral | l::InstructionKind::ArraySpreadLiteral(_) => Some(0),
        l::InstructionKind::Call(target) => counted_operand_start(&target.kind),
        l::InstructionKind::AsyncHandleCreate(target) => match target.kind {
            l::CallTargetKind::Function(_) => Some(0),
            l::CallTargetKind::Method(_) => Some(1),
            _ => None,
        },
        _ => None,
    };
    start
        .into_iter()
        .flat_map(|start| instruction.operands.iter().enumerate().skip(start))
        .filter(|(_, operand)| {
            operand_type(function, operand)
                .as_ref()
                .is_some_and(is_async_owner_type)
        })
        .collect()
}

fn counted_terminator_stores(
    function: &l::Function,
    terminator: &l::Terminator,
) -> Vec<(usize, l::Operand)> {
    match terminator {
        l::Terminator::Return { value, .. } => value
            .iter()
            .filter(|operand| {
                operand_type(function, operand).is_some_and(|ty| is_async_owner_type(&ty))
            })
            .cloned()
            .map(|operand| (0, operand))
            .collect(),
        l::Terminator::Suspend {
            kind: l::SuspendKind::AsyncCall { target, operands },
            ..
        } => {
            let start = counted_operand_start(&target.kind);
            start
                .into_iter()
                .flat_map(|start| operands.iter().copied().enumerate().skip(start))
                .filter(|(_, value)| value_type(function, *value).is_some_and(is_async_owner_type))
                .map(|(index, value)| (index, l::Operand::Value(value)))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn counted_operand_start(kind: &l::CallTargetKind) -> Option<usize> {
    match kind {
        l::CallTargetKind::Function(_) | l::CallTargetKind::Intrinsic(_) => Some(0),
        l::CallTargetKind::StaticClosure(_)
        | l::CallTargetKind::Method(_)
        | l::CallTargetKind::Indirect
        | l::CallTargetKind::BuiltinMethod(_) => Some(1),
        l::CallTargetKind::Foreign(_) => None,
    }
}

fn value_use_counts(function: &l::Function) -> Vec<usize> {
    let mut counts = vec![0; function.values.len()];
    for block in &function.blocks {
        for instruction in &block.instructions {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    if let Some(count) = counts.get_mut(value.0 as usize) {
                        *count += 1;
                    }
                }
            }
        }
        for value in terminator_values(&block.terminator) {
            if let Some(count) = counts.get_mut(value.0 as usize) {
                *count += 1;
            }
        }
    }
    counts
}

fn finding(function: &l::Function, message: impl Into<String>) -> VerifyError {
    VerifyError {
        message: format!(
            "function {} (`{}`): {}",
            function.id.0,
            function.source_name,
            message.into()
        ),
    }
}

fn verify_structure_and_types(
    module: &l::Module,
    function: &l::Function,
    errors: &mut Vec<VerifyError>,
) {
    let mut definitions = vec![0_u32; function.values.len()];
    if function
        .blocks
        .get(function.entry.0 as usize)
        .is_none_or(|block| block.id != function.entry)
    {
        errors.push(finding(
            function,
            format!("entry block {} is missing", function.entry.0),
        ));
    }
    for (index, local) in function.locals.iter().enumerate() {
        if local.id.0 as usize != index {
            errors.push(finding(
                function,
                format!("local table index {index} carries id {}", local.id.0),
            ));
        }
    }
    for (index, value) in function.values.iter().enumerate() {
        if value.id.0 as usize != index {
            errors.push(finding(
                function,
                format!("value table index {index} carries id {}", value.id.0),
            ));
        }
    }
    for parameter in &function.parameters {
        count_definition(function, parameter.value, &mut definitions, errors);
        if function
            .values
            .get(parameter.value.0 as usize)
            .is_some_and(|value| value.fresh_owner)
        {
            errors.push(finding(
                function,
                format!(
                    "parameter value {} is marked as a fresh async owner",
                    parameter.value.0
                ),
            ));
        }
        if let Some(storage) = parameter.storage {
            let parameter_type = value_type(function, parameter.value);
            if function
                .locals
                .get(storage.0 as usize)
                .is_none_or(|local| local.id != storage || Some(&local.ty) != parameter_type)
            {
                errors.push(finding(
                    function,
                    format!(
                        "parameter value {} has invalid address-taken storage {}",
                        parameter.value.0, storage.0
                    ),
                ));
            }
        }
    }
    for (block_index, block) in function.blocks.iter().enumerate() {
        if block.id.0 as usize != block_index {
            errors.push(finding(
                function,
                format!("block table index {block_index} carries id {}", block.id.0),
            ));
        }
        for parameter in &block.parameters {
            count_definition(function, *parameter, &mut definitions, errors);
        }
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if let Some(result) = instruction.result {
                count_definition(function, result, &mut definitions, errors);
            }
            for (operand_index, operand) in instruction.operands.iter().enumerate() {
                if operand_type(function, operand).is_none() {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} instruction {instruction_index} operand {operand_index} names an unknown value",
                            block.id.0
                        ),
                    ));
                }
                verify_constant(
                    function,
                    operand,
                    &format!(
                        "block {} instruction {instruction_index} operand {operand_index}",
                        block.id.0
                    ),
                    errors,
                );
            }
            verify_instruction_contract(
                module,
                function,
                block,
                instruction_index,
                instruction,
                errors,
            );
            for invalidated in &instruction.invalidates {
                if !matches!(
                    value_type(function, *invalidated),
                    Some(l::ValueType::Data(Type::Array(_)))
                ) {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} instruction {instruction_index} invalidates non-array value {}",
                            block.id.0, invalidated.0
                        ),
                    ));
                }
            }
        }
        verify_terminator_types(module, function, block, errors);
    }
    for (id, count) in definitions.into_iter().enumerate() {
        if count != 1 {
            errors.push(finding(
                function,
                format!("value {id} has {count} definitions (expected exactly one)"),
            ));
        }
    }
}

fn count_definition(
    function: &l::Function,
    value: l::ValueId,
    definitions: &mut [u32],
    errors: &mut Vec<VerifyError>,
) {
    if let Some(count) = definitions.get_mut(value.0 as usize) {
        *count += 1;
    } else {
        errors.push(finding(
            function,
            format!("definition names undeclared value {}", value.0),
        ));
    }
}

fn value_type(function: &l::Function, value: l::ValueId) -> Option<&l::ValueType> {
    function
        .values
        .get(value.0 as usize)
        .filter(|entry| entry.id == value)
        .map(|entry| &entry.ty)
}

fn operand_type<'a>(function: &'a l::Function, operand: &'a l::Operand) -> Option<l::ValueType> {
    match operand {
        l::Operand::Value(value) => value_type(function, *value).cloned(),
        l::Operand::Constant(constant) => Some(l::ValueType::Data(constant.ty.clone())),
    }
}

fn verify_operand_type(
    function: &l::Function,
    operand: &l::Operand,
    expected: &l::ValueType,
    context: &str,
    errors: &mut Vec<VerifyError>,
) {
    match operand_type(function, operand) {
        Some(actual) if actual == *expected => {}
        Some(actual) => errors.push(finding(
            function,
            format!("{context} has type {actual:?}, expected {expected:?}"),
        )),
        None => errors.push(finding(
            function,
            format!("{context} names an unknown value"),
        )),
    }
    verify_constant(function, operand, context, errors);
}

fn verify_constant(
    function: &l::Function,
    operand: &l::Operand,
    context: &str,
    errors: &mut Vec<VerifyError>,
) {
    if let l::Operand::Constant(constant) = operand {
        let valid = match constant.kind {
            l::ConstantKind::Boolean(_) => constant.ty == Type::Bool,
            l::ConstantKind::Null => matches!(
                constant.ty,
                Type::Null
                    | Type::Nullable(_)
                    | Type::Object
                    | Type::Class(_)
                    | Type::Array(_)
                    | Type::Map(..)
                    | Type::Set(_)
                    | Type::Worker(..)
                    | Type::Inbox(_)
                    | Type::Outbox(_)
                    | Type::Func(_)
                    | Type::Generator(_)
                    | Type::RegExp
            ),
            l::ConstantKind::FloatBits(_) => {
                matches!(constant.ty, Type::F16 | Type::F32 | Type::F64)
            }
            l::ConstantKind::Integer(_) => matches!(
                constant.ty,
                Type::I8
                    | Type::U8
                    | Type::I16
                    | Type::U16
                    | Type::I32
                    | Type::U32
                    | Type::I64
                    | Type::U64
                    | Type::F16
                    | Type::Date
                    | Type::Enum(_)
                    | Type::StringAlias(_)
            ),
        };
        if !valid {
            errors.push(finding(
                function,
                format!(
                    "{context} has an invalid constant/type pairing: {:?}",
                    constant
                ),
            ));
        }
    }
}

fn verify_instruction_contract(
    module: &l::Module,
    function: &l::Function,
    block: &l::BasicBlock,
    instruction_index: usize,
    instruction: &l::Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let context = format!("block {} instruction {instruction_index}", block.id.0);
    let operand_types = instruction
        .operands
        .iter()
        .filter_map(|operand| operand_type(function, operand))
        .collect::<Vec<_>>();
    let result_type = instruction
        .result
        .and_then(|result| value_type(function, result))
        .cloned();
    let bad = |message: &str, errors: &mut Vec<VerifyError>| {
        errors.push(finding(
            function,
            format!(
                "{context} {message}: kind={:?}, operands={:?}, result={:?}",
                instruction.kind, operand_types, result_type
            ),
        ));
    };
    if let Some(result) = instruction.result {
        let expected_fresh = instruction.kind.produces_fresh_async_owner()
            && result_type.as_ref().is_some_and(is_async_owner_type);
        if function
            .values
            .get(result.0 as usize)
            .is_some_and(|value| value.fresh_owner != expected_fresh)
        {
            bad(
                "fresh-owner bit disagrees with the instruction kind",
                errors,
            );
        }
    }
    for trap in &instruction.traps {
        let l::TrapKind::JsonResultValue(ok_field) = trap.kind else {
            continue;
        };
        let valid = match instruction.kind {
            l::InstructionKind::LoadField(l::FieldRef::Class(value_field)) => module
                .classes
                .iter()
                .find(|class| class.fields.iter().any(|field| field.id == value_field))
                .is_some_and(|class| {
                    class
                        .fields
                        .iter()
                        .any(|field| field.id == ok_field && field.ty == Type::Bool)
                }),
            _ => false,
        };
        if !valid {
            bad(
                "JsonResultValue trap names no boolean field in the loaded field's class",
                errors,
            );
        }
    }
    match &instruction.kind {
        l::InstructionKind::Copy => {
            if operand_types.len() != 1 || result_type.as_ref() != operand_types.first() {
                bad("copy input/result types do not match", errors);
            }
        }
        l::InstructionKind::Coerce => {
            let data_coercion =
                matches!(
                    (operand_types.first(), result_type.as_ref()),
                    (Some(l::ValueType::Data(_)), Some(l::ValueType::Data(_)))
                ) && !boundary_box_coercion_signature(module, &operand_types, result_type.as_ref());
            if operand_types.len() != 1 || !data_coercion {
                bad("implicit coercion signature is invalid", errors);
            }
        }
        l::InstructionKind::StringLiteral(_) => {
            if !instruction.operands.is_empty()
                || result_type != Some(l::ValueType::Data(Type::Str))
            {
                bad("string literal signature is invalid", errors);
            }
        }
        l::InstructionKind::LoadLocal(local) => {
            let expected = function.locals.get(local.0 as usize).map(|local| &local.ty);
            if !instruction.operands.is_empty() || result_type.as_ref() != expected {
                bad("local load signature is invalid", errors);
            }
        }
        l::InstructionKind::StoreLocal(local) => {
            let expected = function.locals.get(local.0 as usize).map(|local| &local.ty);
            if operand_types.first() != expected
                || operand_types.len() != 1
                || instruction.result.is_some()
            {
                bad("local store signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfLocal(local) => {
            let expected = function.locals.get(local.0 as usize).map(|local| &local.ty);
            let valid = match (expected, result_type.as_ref()) {
                (Some(l::ValueType::Data(stored)), Some(l::ValueType::Address(address))) => {
                    address.pointee == *stored
                }
                _ => false,
            };
            if !valid || !instruction.operands.is_empty() {
                bad("local address signature is invalid", errors);
            }
        }
        l::InstructionKind::LoadGlobal(global) => {
            let expected = module
                .globals
                .get(global.0 as usize)
                .map(|global| l::ValueType::Data(global.ty.clone()));
            if !instruction.operands.is_empty() || result_type != expected {
                bad("global load signature is invalid", errors);
            }
        }
        l::InstructionKind::StoreGlobal(global) => {
            let expected = module
                .globals
                .get(global.0 as usize)
                .map(|global| l::ValueType::Data(global.ty.clone()));
            if operand_types.len() != 1
                || operand_types.first() != expected.as_ref()
                || instruction.result.is_some()
            {
                bad("global store signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfGlobal(global) => {
            let expected = module
                .globals
                .get(global.0 as usize)
                .map(|global| &global.ty);
            let valid = match (expected, result_type.as_ref()) {
                (Some(stored), Some(l::ValueType::Address(address))) => {
                    address.pointee == *stored && address.array_base.is_none()
                }
                _ => false,
            };
            if !valid || !instruction.operands.is_empty() {
                bad("global address signature is invalid", errors);
            }
        }
        l::InstructionKind::FunctionRef(target) => {
            if module.functions.get(target.0 as usize).is_none()
                || !instruction.operands.is_empty()
                || !matches!(result_type, Some(l::ValueType::Data(Type::Func(_))))
            {
                bad("function reference signature is invalid", errors);
            }
        }
        l::InstructionKind::Unary(op) => {
            if let ([input], Some(output)) = (operand_types.as_slice(), result_type.as_ref()) {
                let valid = match (op, input, output) {
                    (
                        l::UnaryOp::Not,
                        l::ValueType::Data(Type::Bool),
                        l::ValueType::Data(Type::Bool),
                    ) => true,
                    (l::UnaryOp::Neg, l::ValueType::Data(a), l::ValueType::Data(b)) => {
                        a == b && a.is_numeric()
                    }
                    (l::UnaryOp::BitNot, l::ValueType::Data(a), l::ValueType::Data(b)) => {
                        a == b && a.is_integer()
                    }
                    _ => false,
                };
                if !valid {
                    bad("unary operand/result types are invalid", errors);
                }
            } else {
                bad("unary signature is incomplete", errors);
            }
        }
        l::InstructionKind::Binary(op) => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Data(left), l::ValueType::Data(right)],
                    Some(l::ValueType::Data(result)),
                ) => match op {
                    l::BinaryOp::Add if *left == Type::Str => {
                        *right == Type::Str && *result == Type::Str
                    }
                    l::BinaryOp::Add
                    | l::BinaryOp::Sub
                    | l::BinaryOp::Mul
                    | l::BinaryOp::Div
                    | l::BinaryOp::Rem => left == right && left == result && left.is_numeric(),
                    l::BinaryOp::Eq
                    | l::BinaryOp::Ne
                    | l::BinaryOp::Lt
                    | l::BinaryOp::Le
                    | l::BinaryOp::Gt
                    | l::BinaryOp::Ge => *result == Type::Bool,
                    l::BinaryOp::BitAnd
                    | l::BinaryOp::BitOr
                    | l::BinaryOp::BitXor
                    | l::BinaryOp::Shl
                    | l::BinaryOp::Shr
                    | l::BinaryOp::UShr => {
                        left.is_integer() && right.is_integer() && left == result
                    }
                },
                _ => false,
            };
            if !valid {
                bad("binary operand/result types are invalid", errors);
            }
        }
        l::InstructionKind::Cast => {
            if operand_types.len() != 1
                || !matches!(operand_types[0], l::ValueType::Data(_))
                || !matches!(result_type, Some(l::ValueType::Data(_)))
            {
                bad("cast signature is invalid", errors);
            }
        }
        l::InstructionKind::AllocateClass(class) => {
            let expected = module.classes.get(class.0).map(|definition| {
                if definition.is_value {
                    l::ValueType::Address(l::AddressType {
                        pointee: Type::Class(*class),
                        array_base: None,
                    })
                } else {
                    l::ValueType::Data(Type::Class(*class))
                }
            });
            if !instruction.operands.is_empty() || result_type != expected {
                bad("class allocation signature is invalid", errors);
            }
        }
        l::InstructionKind::BoxBoundaryValue { payload } => {
            let valid =
                boundary_box_signature(module, *payload, &operand_types, result_type.as_ref());
            if !valid {
                bad("boundary value box signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfValue => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                ([l::ValueType::Data(value)], Some(l::ValueType::Address(address))) => {
                    address.pointee == *value && address.array_base.is_none()
                }
                _ => false,
            };
            if !valid {
                bad("temporary address signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfField(field) => {
            let expected_field = lir_field_type(module, *field, operand_types.first());
            let base_valid = operand_types
                .first()
                .is_some_and(|base| field_base_accepts(module, *field, base));
            let result_valid = match (expected_field, result_type.as_ref()) {
                (Some(expected), Some(l::ValueType::Address(address))) => {
                    let expected_base = operand_types.first().and_then(address_base);
                    address.pointee == expected && address.array_base == expected_base
                }
                _ => false,
            };
            if operand_types.len() != 1 || !base_valid || !result_valid {
                bad("field address signature is invalid", errors);
            }
        }
        l::InstructionKind::Call(target) => {
            if is_operation_table_target(&target.kind) {
                if !target.parameter_types.is_empty() {
                    bad(
                        "intrinsic or built-in target restates table parameter types",
                        errors,
                    );
                }
                let matching = module.operation_signatures(&target.kind).find(|signature| {
                    call_parameters_match(&operand_types, &signature.parameter_types)
                        && target.return_type == signature.return_type
                });
                if matching.is_none() || result_type != target.return_type {
                    let name = operation_name(module, &target.kind);
                    let declared = module
                        .operation_signatures(&target.kind)
                        .map(|signature| {
                            format!(
                                "{:?} -> {:?}",
                                signature.parameter_types, signature.return_type
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" or ");
                    errors.push(finding(
                        function,
                        format!(
                            "{context} call disagrees with the signature table: {name} declares {declared}, got {operand_types:?} -> {result_type:?}"
                        ),
                    ));
                }
            } else {
                if !call_parameters_match(&operand_types, &target.parameter_types)
                    || result_type != target.return_type
                {
                    bad("call signature disagrees with its target", errors);
                }
                if let Some((parameters, result)) =
                    declared_call_signature(module, &target.kind, &operand_types)
                {
                    if !declared_parameters_match(
                        module,
                        &target.kind,
                        &target.parameter_types,
                        &parameters,
                    ) || target.return_type != result
                    {
                        bad(
                            "call signature disagrees with the target declaration",
                            errors,
                        );
                    }
                }
            }
            let target_exists = match &target.kind {
                l::CallTargetKind::Function(id) => declared_function(module, *id).is_some(),
                l::CallTargetKind::StaticClosure(id) => {
                    declared_function(module, *id)
                        .is_some_and(|function| function.kind == l::FunctionKind::Lambda)
                        && instruction.operands.first().is_some_and(|operand| {
                            static_closure_operand_matches(function, operand, *id)
                        })
                }
                l::CallTargetKind::Method(id) => declared_method_function(module, *id).is_some(),
                l::CallTargetKind::Foreign(id) => module
                    .foreign_functions
                    .get(id.0 as usize)
                    .is_some_and(|foreign| foreign.id == *id),
                l::CallTargetKind::Indirect => matches!(
                    target.parameter_types.first(),
                    Some(l::ValueType::Data(Type::Func(_)))
                ),
                l::CallTargetKind::Intrinsic(intrinsic) => {
                    (intrinsic.family == l::IntrinsicFamily::ContextBytes)
                        == intrinsic.type_argument.is_some()
                        && module.intrinsic_operations.iter().any(|operation| {
                            operation.family == intrinsic.family
                                && operation.operation == intrinsic.operation
                        })
                }
                l::CallTargetKind::BuiltinMethod(_) => {
                    module.operation_signatures(&target.kind).next().is_some()
                }
            };
            if !target_exists {
                bad("call target identity/signature is invalid", errors);
            }
        }
        l::InstructionKind::AsyncHandleCreate(target) => {
            let declared = match target.kind {
                l::CallTargetKind::Function(id) => declared_function(module, id),
                l::CallTargetKind::Method(id) => declared_method_function(module, id),
                _ => None,
            };
            let valid_result = matches!(result_type.as_ref(), Some(l::ValueType::Data(Type::AsyncHandle(value)))
                if target.return_type.as_ref() == ((**value != Type::Void)
                    .then(|| l::ValueType::Data((**value).clone()))) .as_ref());
            if !call_parameters_match(&operand_types, &target.parameter_types)
                || !valid_result
                || declared.is_none_or(|function| !function.is_async)
            {
                bad("async handle creation signature is invalid", errors);
            }
        }
        l::InstructionKind::AsyncHandleRetain | l::InstructionKind::AsyncHandleRelease => {
            if operand_types.len() != 1
                || !matches!(
                    operand_types.first(),
                    Some(l::ValueType::Data(Type::AsyncHandle(_)))
                )
                || instruction.result.is_some()
            {
                bad("async handle ownership instruction is invalid", errors);
            }
        }
        l::InstructionKind::AsyncHandleArrayRetain
        | l::InstructionKind::AsyncHandleArrayRelease => {
            if operand_types.len() != 1
                || !matches!(operand_types.first(), Some(l::ValueType::Data(Type::Array(element)))
                    if matches!(&**element, Type::AsyncHandle(_)))
                || instruction.result.is_some()
            {
                bad("async handle array release signature is invalid", errors);
            }
        }
        l::InstructionKind::LoadAddress => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                ([l::ValueType::Address(address)], Some(l::ValueType::Data(result))) => {
                    address.pointee == *result
                }
                _ => false,
            };
            if !valid {
                bad("address load signature is invalid", errors);
            }
        }
        l::InstructionKind::StoreAddress => {
            let valid = match operand_types.as_slice() {
                [l::ValueType::Address(address), l::ValueType::Data(value)] => {
                    address.pointee == *value
                }
                _ => false,
            };
            if !valid || instruction.result.is_some() {
                bad("address store signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfIndex { checked } => {
            let valid_index = matches!(
                operand_types.get(1),
                Some(l::ValueType::Data(Type::I32 | Type::U32))
            );
            let element = operand_types.first().and_then(index_element_type);
            let result_element = match result_type.as_ref() {
                Some(l::ValueType::Address(address)) => Some(&address.pointee),
                _ => None,
            };
            let expected_base = match (instruction.operands.first(), operand_types.first()) {
                (Some(l::Operand::Value(value)), Some(l::ValueType::Data(Type::Array(_)))) => {
                    Some(*value)
                }
                (_, Some(l::ValueType::Address(address))) => address.array_base,
                _ => None,
            };
            let result_base = match result_type.as_ref() {
                Some(l::ValueType::Address(address)) => address.array_base,
                _ => None,
            };
            let has_bounds_trap = instruction
                .traps
                .iter()
                .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead | l::TrapKind::IndexWrite));
            if operand_types.len() != 2
                || !valid_index
                || element != result_element
                || expected_base != result_base
            {
                bad("index address signature is invalid", errors);
            }
            if *checked != has_bounds_trap {
                bad("index address check disagrees with its bounds trap", errors);
            }
        }
        l::InstructionKind::LoadField(field) => {
            let expected_field = lir_field_type(module, *field, operand_types.first());
            let base_valid = operand_types
                .first()
                .is_some_and(|base| field_base_accepts(module, *field, base));
            if operand_types.len() != 1
                || !base_valid
                || result_type != expected_field.map(l::ValueType::Data)
            {
                bad("field load signature is invalid", errors);
            }
        }
        l::InstructionKind::Length => {
            let valid_subject = matches!(
                operand_types.first(),
                Some(l::ValueType::Data(
                    Type::Str | Type::Array(_) | Type::FixedArray(..)
                ))
            );
            if operand_types.len() != 1
                || !valid_subject
                || !matches!(result_type, Some(l::ValueType::Data(Type::I32)))
            {
                bad("length signature is invalid", errors);
            }
        }
        l::InstructionKind::ForeignArrayData => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Data(Type::Array(element))],
                    Some(l::ValueType::Address(address)),
                ) => address.pointee == **element && address.array_base.is_none(),
                _ => false,
            };
            if !valid {
                bad("foreign array data signature is invalid", errors);
            }
        }
        l::InstructionKind::ArrayLiteral => {
            let valid = match result_type.as_ref() {
                Some(l::ValueType::Data(Type::Array(element))) => operand_types
                    .iter()
                    .all(|ty| ty == &l::ValueType::Data((**element).clone())),
                Some(l::ValueType::Data(Type::FixedArray(element, count))) => {
                    operand_types.len() == *count as usize
                        && operand_types
                            .iter()
                            .all(|ty| ty == &l::ValueType::Data((**element).clone()))
                }
                _ => false,
            };
            if !valid {
                bad("array literal signature is invalid", errors);
            }
        }
        l::InstructionKind::ArrayWithCapacity => {
            let valid = matches!(
                (operand_types.as_slice(), result_type.as_ref()),
                (
                    [l::ValueType::Data(Type::I32)],
                    Some(l::ValueType::Data(Type::Array(_)))
                )
            ) && instruction.traps.len() == 1
                && instruction.traps[0].kind == l::TrapKind::Call;
            if !valid {
                bad("capacity array signature/traps are invalid", errors);
            }
        }
        l::InstructionKind::ArraySpreadLiteral(spreads) => {
            let valid = match result_type.as_ref() {
                Some(l::ValueType::Data(Type::Array(element))) => {
                    spreads.len() == operand_types.len()
                        && spreads.iter().zip(&operand_types).all(
                            |(spread, operand)| match spread {
                                None => operand == &l::ValueType::Data((**element).clone()),
                                Some(_) => matches!(operand, l::ValueType::Data(_)),
                            },
                        )
                }
                _ => false,
            };
            if !valid {
                bad("spread-array literal signature is invalid", errors);
            }
        }
        l::InstructionKind::Template(parts) => {
            let valid_indices = parts.iter().all(|part| match part {
                l::TemplatePart::Text(_) => true,
                l::TemplatePart::Operand { index, format } => operand_types
                    .get(*index as usize)
                    .and_then(|ty| match ty {
                        l::ValueType::Data(ty) => Some(ty),
                        _ => None,
                    })
                    .is_some_and(|ty| format.accepts(ty)),
            });
            if !valid_indices || result_type != Some(l::ValueType::Data(Type::Str)) {
                bad("template signature is invalid", errors);
            }
        }
        l::InstructionKind::MakeClosure(target) => {
            let capture_count = module.functions.get(target.0 as usize).map(|target| {
                target
                    .parameters
                    .iter()
                    .take_while(|parameter| parameter.kind == l::ParameterKind::Capture)
                    .count()
            });
            if capture_count != Some(operand_types.len())
                || !matches!(result_type, Some(l::ValueType::Data(Type::Func(_))))
            {
                bad("closure signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorCreate { kind, bound } => {
            let valid = matches!(
                result_type.as_ref(),
                Some(l::ValueType::Iterator(iterator)) if iterator.kind == *kind
            );
            if operand_types.len() != 1 || !valid {
                bad("iterator creation signature is invalid", errors);
            }
            if *bound == l::IteratorBoundKind::Fixed
                && !matches!(
                    kind,
                    l::ForOfKind::ArrayValues
                        | l::ForOfKind::ArrayValuesReverse
                        | l::ForOfKind::ArrayKeysReverse
                )
            {
                bad(
                    "fixed iterator bound requires a dynamic-array value cursor",
                    errors,
                );
            }
        }
        l::InstructionKind::IteratorHasNext => {
            if !valid_iterator_state(&operand_types)
                || result_type != Some(l::ValueType::Data(Type::Bool))
            {
                bad("iterator condition signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorValue => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Iterator(iterator), l::ValueType::Data(Type::I32), l::ValueType::Data(Type::I32)],
                    Some(l::ValueType::Data(result)),
                ) => iterator.element == *result,
                _ => false,
            };
            if !valid {
                bad("iterator value signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorBound => {
            if !matches!(operand_types.as_slice(), [l::ValueType::Iterator(_)])
                || result_type != Some(l::ValueType::Data(Type::I32))
            {
                bad("iterator bound signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorAdvance => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Iterator(input), l::ValueType::Data(Type::I32), l::ValueType::Data(Type::I32)],
                    Some(l::ValueType::Iterator(output)),
                ) => input == output,
                _ => false,
            };
            if !valid {
                bad("iterator advance signature is invalid", errors);
            }
        }
        l::InstructionKind::Zero => {
            if !instruction.operands.is_empty()
                || !matches!(result_type, Some(l::ValueType::Data(_)))
            {
                bad("typed zero signature is invalid", errors);
            }
        }
    }
}

fn lir_field_type(
    module: &l::Module,
    field: l::FieldRef,
    base: Option<&l::ValueType>,
) -> Option<Type> {
    match field {
        l::FieldRef::Class(id) => module
            .classes
            .iter()
            .flat_map(|class| &class.fields)
            .find(|field| field.id == id)
            .map(|field| field.ty.clone()),
        l::FieldRef::IterDone => Some(Type::Bool),
        l::FieldRef::IterValue => match base {
            Some(l::ValueType::Data(Type::IterResult(value))) => Some((**value).clone()),
            Some(l::ValueType::Address(l::AddressType {
                pointee: Type::IterResult(value),
                ..
            })) => Some((**value).clone()),
            _ => None,
        },
    }
}

fn boundary_box_signature(
    module: &l::Module,
    payload: ClassId,
    operands: &[l::ValueType],
    result: Option<&l::ValueType>,
) -> bool {
    let (
        [l::ValueType::Data(Type::Class(source))],
        Some(l::ValueType::Data(Type::Nullable(target))),
    ) = (operands, result)
    else {
        return false;
    };
    let Type::Class(target) = target.as_ref() else {
        return false;
    };
    if *source != payload {
        return false;
    }
    let Some(source_class) = module.classes.get(source.0) else {
        return false;
    };
    if !source_class.is_value || !source_class.is_boundary {
        return false;
    }
    if source == target {
        return true;
    }
    source_class.fields.first().is_some_and(|field| {
        field.ty == Type::Class(*target)
            && module
                .classes
                .get(target.0)
                .is_some_and(|class| class.is_embedded_header)
    })
}

fn boundary_box_coercion_signature(
    module: &l::Module,
    operands: &[l::ValueType],
    result: Option<&l::ValueType>,
) -> bool {
    let [l::ValueType::Data(Type::Class(payload))] = operands else {
        return false;
    };
    boundary_box_signature(module, *payload, operands, result)
}

fn field_base_accepts(module: &l::Module, field: l::FieldRef, base: &l::ValueType) -> bool {
    match field {
        l::FieldRef::Class(id) => {
            let owner = module
                .classes
                .iter()
                .find(|class| class.fields.iter().any(|candidate| candidate.id == id));
            owner.is_some_and(|class| match base {
                l::ValueType::Data(Type::Class(id)) => *id == class.id,
                l::ValueType::Data(Type::Nullable(inner)) => {
                    matches!(inner.as_ref(), Type::Class(id)
                        if *id == class.id && class.is_value && class.is_boundary)
                }
                l::ValueType::Address(address) => address.pointee == Type::Class(class.id),
                _ => false,
            })
        }
        l::FieldRef::IterDone | l::FieldRef::IterValue => matches!(
            base,
            l::ValueType::Data(Type::IterResult(_))
                | l::ValueType::Address(l::AddressType {
                    pointee: Type::IterResult(_),
                    ..
                })
        ),
    }
}

fn index_element_type(ty: &l::ValueType) -> Option<&Type> {
    match ty {
        l::ValueType::Data(Type::Array(element) | Type::FixedArray(element, _)) => Some(element),
        l::ValueType::Address(l::AddressType {
            pointee: Type::FixedArray(element, _),
            ..
        }) => Some(element),
        _ => None,
    }
}

fn valid_iterator_state(types: &[l::ValueType]) -> bool {
    matches!(
        types,
        [
            l::ValueType::Iterator(_),
            l::ValueType::Data(Type::I32),
            l::ValueType::Data(Type::I32)
        ]
    )
}

fn declared_function(module: &l::Module, id: l::FunctionId) -> Option<&l::Function> {
    module
        .functions
        .get(id.0 as usize)
        .filter(|function| function.id == id)
}

fn declared_method_function(module: &l::Module, id: l::MethodId) -> Option<&l::Function> {
    let function = module.classes.iter().find_map(|class| {
        class
            .constructor
            .iter()
            .chain(&class.methods)
            .find(|method| method.id == id)
            .map(|method| method.function)
    })?;
    declared_function(module, function)
}

fn function_signature(function: &l::Function) -> (Vec<l::ValueType>, Option<l::ValueType>) {
    (
        function
            .parameters
            .iter()
            .filter_map(|parameter| value_type(function, parameter.value).cloned())
            .collect(),
        (function.return_type != Type::Void)
            .then(|| l::ValueType::Data(function.return_type.clone())),
    )
}

fn call_type_matches(actual: &l::ValueType, declared: &l::ValueType) -> bool {
    match (actual, declared) {
        (l::ValueType::Address(actual), l::ValueType::Address(declared)) => {
            actual.pointee == declared.pointee
        }
        _ => actual == declared,
    }
}

fn call_parameters_match(actual: &[l::ValueType], declared: &[l::ValueType]) -> bool {
    actual.len() == declared.len()
        && actual
            .iter()
            .zip(declared)
            .all(|(actual, declared)| call_type_matches(actual, declared))
}

fn declared_parameters_match(
    module: &l::Module,
    kind: &l::CallTargetKind,
    actual: &[l::ValueType],
    declared: &[l::ValueType],
) -> bool {
    actual.len() == declared.len()
        && actual.iter().zip(declared).all(|(actual, declared)| {
            call_type_matches(actual, declared)
                || matches!(kind, l::CallTargetKind::Foreign(_))
                    && foreign_boundary_pointer_type_matches(module, actual, declared)
        })
}

fn static_closure_operand_matches(
    function: &l::Function,
    operand: &l::Operand,
    target: l::FunctionId,
) -> bool {
    fn collect(
        function: &l::Function,
        operand: &l::Operand,
        target: l::FunctionId,
        visiting: &mut BTreeSet<l::ValueId>,
        saw_closure: &mut bool,
    ) -> bool {
        let l::Operand::Value(value) = operand else {
            return false;
        };
        if !visiting.insert(*value) {
            return true;
        }
        if let Some(instruction) = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| instruction.result == Some(*value))
        {
            let valid = match &instruction.kind {
                l::InstructionKind::MakeClosure(function) => {
                    *saw_closure = true;
                    *function == target
                }
                l::InstructionKind::Copy => instruction.operands.first().is_some_and(|operand| {
                    collect(function, operand, target, visiting, saw_closure)
                }),
                _ => false,
            };
            visiting.remove(value);
            return valid;
        }
        if function
            .parameters
            .iter()
            .any(|parameter| parameter.value == *value)
        {
            visiting.remove(value);
            return false;
        }
        let Some((destination, index)) = function.blocks.iter().find_map(|block| {
            block
                .parameters
                .iter()
                .position(|parameter| parameter == value)
                .map(|index| (block.id, index))
        }) else {
            visiting.remove(value);
            return false;
        };
        let mut found = false;
        let mut valid = true;
        let mut inspect = |edge: &l::BlockTarget| {
            if edge.block != destination {
                return;
            }
            found = true;
            valid &= edge
                .arguments
                .get(index)
                .is_some_and(|operand| collect(function, operand, target, visiting, saw_closure));
        };
        for block in &function.blocks {
            if matches!(block.terminator, l::Terminator::Suspend { .. }) {
                continue;
            }
            for edge in block.terminator.targets() {
                inspect(&edge);
            }
        }
        visiting.remove(value);
        found && valid
    }

    let mut saw_closure = false;
    let valid = collect(
        function,
        operand,
        target,
        &mut BTreeSet::new(),
        &mut saw_closure,
    );
    valid && saw_closure
}

fn foreign_boundary_pointer_type_matches(
    module: &l::Module,
    actual: &l::ValueType,
    declared: &l::ValueType,
) -> bool {
    let (l::ValueType::Address(address), l::ValueType::Data(declared)) = (actual, declared) else {
        return false;
    };
    boundary_box_class(module, declared).is_some_and(|class| address.pointee == Type::Class(class))
}

fn is_operation_table_target(kind: &l::CallTargetKind) -> bool {
    matches!(
        kind,
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
    )
}

fn operation_name(module: &l::Module, kind: &l::CallTargetKind) -> String {
    match kind {
        l::CallTargetKind::Intrinsic(intrinsic) => {
            let name = module
                .intrinsic_operations
                .iter()
                .find(|operation| {
                    operation.family == intrinsic.family
                        && operation.operation == intrinsic.operation
                })
                .map_or("<unknown>", |operation| operation.semantic_name.as_str());
            format!("{:?}.{name}", intrinsic.family)
        }
        l::CallTargetKind::BuiltinMethod(method) => format!("BuiltinMethod.{method:?}"),
        l::CallTargetKind::Function(_)
        | l::CallTargetKind::StaticClosure(_)
        | l::CallTargetKind::Method(_)
        | l::CallTargetKind::Foreign(_)
        | l::CallTargetKind::Indirect => "<declared call>".to_string(),
    }
}

fn declared_call_signature(
    module: &l::Module,
    kind: &l::CallTargetKind,
    operand_types: &[l::ValueType],
) -> Option<(Vec<l::ValueType>, Option<l::ValueType>)> {
    match kind {
        l::CallTargetKind::Function(id) => declared_function(module, *id).map(function_signature),
        l::CallTargetKind::StaticClosure(id) => {
            let function = declared_function(module, *id)?;
            let callable = operand_types.first()?.clone();
            if !matches!(callable, l::ValueType::Data(Type::Func(_))) {
                return None;
            }
            let mut parameters = vec![callable];
            parameters.extend(
                function
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.kind == l::ParameterKind::Explicit)
                    .filter_map(|parameter| value_type(function, parameter.value).cloned()),
            );
            Some((
                parameters,
                (function.return_type != Type::Void)
                    .then(|| l::ValueType::Data(function.return_type.clone())),
            ))
        }
        l::CallTargetKind::Method(id) => {
            declared_method_function(module, *id).map(function_signature)
        }
        l::CallTargetKind::Foreign(id) => module
            .foreign_functions
            .get(id.0 as usize)
            .filter(|function| function.id == *id)
            .map(|function| {
                (
                    function
                        .parameters
                        .iter()
                        .flat_map(|parameter| match &parameter.ty {
                            Type::Array(element) => vec![
                                l::ValueType::Address(l::AddressType {
                                    pointee: (**element).clone(),
                                    array_base: None,
                                }),
                                l::ValueType::Data(Type::I32),
                            ],
                            ty => vec![l::ValueType::Data(ty.clone())],
                        })
                        .collect(),
                    (function.return_type != Type::Void)
                        .then(|| l::ValueType::Data(function.return_type.clone())),
                )
            }),
        l::CallTargetKind::Indirect => {
            let callee = operand_types.first()?.clone();
            let l::ValueType::Data(Type::Func(signature)) = &callee else {
                return None;
            };
            let signature = signature.clone();
            let mut parameters = vec![callee];
            parameters.extend(signature.params.iter().cloned().map(l::ValueType::Data));
            Some((
                parameters,
                (signature.ret != Type::Void).then(|| l::ValueType::Data(signature.ret.clone())),
            ))
        }
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_) => module
            .operation_signatures(kind)
            .find(|signature| call_parameters_match(operand_types, &signature.parameter_types))
            .map(|signature| {
                (
                    signature.parameter_types.clone(),
                    signature.return_type.clone(),
                )
            }),
    }
}

fn verify_terminator_types(
    module: &l::Module,
    function: &l::Function,
    block: &l::BasicBlock,
    errors: &mut Vec<VerifyError>,
) {
    if !matches!(block.terminator, l::Terminator::Suspend { .. }) {
        for target in block.terminator.targets() {
            verify_edge(function, block, &target, errors);
        }
    }
    match &block.terminator {
        l::Terminator::Branch(_) => {}
        l::Terminator::ConditionalBranch { condition, .. } => {
            verify_operand_type(
                function,
                condition,
                &l::ValueType::Data(Type::Bool),
                &format!("block {} conditional", block.id.0),
                errors,
            );
        }
        l::Terminator::Switch { value, arms, .. } => {
            let discriminant = operand_type(function, value);
            for arm in arms {
                if discriminant != Some(l::ValueType::Data(arm.value.ty.clone())) {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} switch arm type differs from discriminant",
                            block.id.0
                        ),
                    ));
                }
            }
        }
        l::Terminator::Return { value: None, .. } if function.is_generator => {}
        l::Terminator::Return { value, .. } => match (value, &function.return_type) {
            (None, Type::Void) => {}
            (Some(value), ty) if *ty != Type::Void => verify_operand_type(
                function,
                value,
                &l::ValueType::Data(ty.clone()),
                &format!("block {} return", block.id.0),
                errors,
            ),
            _ => errors.push(finding(
                function,
                format!("block {} return type is invalid", block.id.0),
            )),
        },
        l::Terminator::Trap(_) | l::Terminator::Unreachable { .. } => {}
        l::Terminator::Suspend {
            kind,
            successor,
            resume_value,
            arguments,
            invalidates,
            ..
        } => {
            let Some(destination) = function.blocks.get(successor.0 as usize) else {
                errors.push(finding(
                    function,
                    format!(
                        "block {} suspend successor {} is missing",
                        block.id.0, successor.0
                    ),
                ));
                return;
            };
            if resume_value.is_some() && *resume_value != destination.parameters.first().copied() {
                errors.push(finding(
                    function,
                    format!(
                        "block {} suspend resume value is not its successor parameter",
                        block.id.0
                    ),
                ));
            }
            let parameters = &destination.parameters
                [usize::from(resume_value.is_some()).min(destination.parameters.len())..];
            if arguments.len() != parameters.len() {
                errors.push(finding(
                    function,
                    format!(
                        "block {} suspend edge to {} has {} arguments for {} live-in parameters",
                        block.id.0,
                        successor.0,
                        arguments.len(),
                        parameters.len()
                    ),
                ));
            }
            for (argument, parameter) in arguments.iter().zip(parameters) {
                if let Some(expected) = value_type(function, *parameter) {
                    verify_operand_type(
                        function,
                        argument,
                        expected,
                        &format!("suspend edge {} -> {}", block.id.0, successor.0),
                        errors,
                    );
                }
            }
            match kind {
                l::SuspendKind::Yield(value) => {
                    if let Some(value) = value {
                        if value_type(function, *value).is_none() {
                            errors.push(finding(
                                function,
                                format!(
                                    "block {} yield names unknown value {}",
                                    block.id.0, value.0
                                ),
                            ));
                        }
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { target, operands } => {
                    if let Some((parameters, result)) =
                        declared_call_signature(module, &target.kind, &target.parameter_types)
                    {
                        if !declared_parameters_match(
                            module,
                            &target.kind,
                            &target.parameter_types,
                            &parameters,
                        ) || target.return_type != result
                        {
                            errors.push(finding(
                                function,
                                format!(
                                    "block {} async-call signature disagrees with the target declaration",
                                    block.id.0
                                ),
                            ));
                        }
                    } else {
                        errors.push(finding(
                            function,
                            format!(
                                "block {} async-call target declaration is missing",
                                block.id.0
                            ),
                        ));
                    }
                    if operands.len() != target.parameter_types.len() {
                        errors.push(finding(
                            function,
                            format!("block {} async-call arity is invalid", block.id.0),
                        ));
                    }
                    for (operand, expected) in operands.iter().zip(&target.parameter_types) {
                        if value_type(function, *operand)
                            .is_none_or(|actual| !call_type_matches(actual, expected))
                        {
                            errors.push(finding(
                                function,
                                format!("block {} async-call operand type is invalid", block.id.0),
                            ));
                        }
                    }
                    if target.return_type.as_ref()
                        != resume_value.and_then(|value| value_type(function, value))
                    {
                        errors.push(finding(
                            function,
                            format!("block {} async-call resume type is invalid", block.id.0),
                        ));
                    }
                }
                l::SuspendKind::AsyncHandle { handle } => {
                    let Some(l::ValueType::Data(Type::AsyncHandle(value))) =
                        value_type(function, *handle)
                    else {
                        errors.push(finding(
                            function,
                            format!("block {} held await has an invalid handle", block.id.0),
                        ));
                        return;
                    };
                    let expected =
                        (**value != Type::Void).then(|| l::ValueType::Data((**value).clone()));
                    if expected.as_ref()
                        != resume_value.and_then(|value| value_type(function, value))
                    {
                        errors.push(finding(
                            function,
                            format!("block {} held await resume type is invalid", block.id.0),
                        ));
                    }
                }
            }
            for invalidated in invalidates {
                if !matches!(
                    value_type(function, *invalidated),
                    Some(l::ValueType::Data(Type::Array(_)))
                ) {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} suspend invalidates non-array value {}",
                            block.id.0, invalidated.0
                        ),
                    ));
                }
            }
        }
    }
}

fn verify_edge(
    function: &l::Function,
    source: &l::BasicBlock,
    edge: &l::BlockTarget,
    errors: &mut Vec<VerifyError>,
) {
    let Some(destination) = function.blocks.get(edge.block.0 as usize) else {
        errors.push(finding(
            function,
            format!(
                "block {} branches to missing block {}",
                source.id.0, edge.block.0
            ),
        ));
        return;
    };
    let parameters = destination.parameters.as_slice();
    if edge.arguments.len() != parameters.len() {
        errors.push(finding(
            function,
            format!(
                "edge {} -> {} has {} arguments for {} parameters",
                source.id.0,
                edge.block.0,
                edge.arguments.len(),
                parameters.len()
            ),
        ));
    }
    for (argument, parameter) in edge.arguments.iter().zip(parameters) {
        if let Some(expected) = value_type(function, *parameter) {
            verify_operand_type(
                function,
                argument,
                expected,
                &format!("edge {} -> {}", source.id.0, edge.block.0),
                errors,
            );
        }
    }
}

#[derive(Clone, Copy)]
enum DefinitionSite {
    Entry,
    BlockEntry(l::BlockId),
    Instruction(l::BlockId, usize),
}

fn verify_dominance(function: &l::Function, errors: &mut Vec<VerifyError>) {
    let predecessors = predecessors(function);
    let dominators = dominators(function, &predecessors);
    let mut definitions = vec![None; function.values.len()];
    for parameter in &function.parameters {
        set_definition(&mut definitions, parameter.value, DefinitionSite::Entry);
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            set_definition(
                &mut definitions,
                *parameter,
                DefinitionSite::BlockEntry(block.id),
            );
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Some(result) = instruction.result {
                set_definition(
                    &mut definitions,
                    result,
                    DefinitionSite::Instruction(block.id, index),
                );
            }
        }
    }
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    check_dominates(
                        function,
                        *value,
                        block.id,
                        index,
                        &definitions,
                        &dominators,
                        errors,
                    );
                }
            }
        }
        for value in terminator_values(&block.terminator) {
            check_dominates(
                function,
                value,
                block.id,
                block.instructions.len(),
                &definitions,
                &dominators,
                errors,
            );
        }
    }
    verify_array_base_dominance(function, &definitions, &dominators, errors);
    verify_suspend_definition_boundaries(function, &definitions, &dominators, errors);
}

fn verify_array_base_dominance(
    function: &l::Function,
    definitions: &[Option<DefinitionSite>],
    dominators: &[BTreeSet<l::BlockId>],
    errors: &mut Vec<VerifyError>,
) {
    for value in &function.values {
        let l::ValueType::Address(address) = &value.ty else {
            continue;
        };
        let Some(base) = address.array_base else {
            continue;
        };
        let Some(base_value) = function.values.get(base.0 as usize) else {
            errors.push(finding(
                function,
                format!(
                    "address value {} names undeclared array base value {}",
                    value.id.0, base.0
                ),
            ));
            continue;
        };
        if !matches!(base_value.ty, l::ValueType::Data(Type::Array(_))) {
            errors.push(finding(
                function,
                format!(
                    "address value {} names non-array base value {}",
                    value.id.0, base.0
                ),
            ));
            continue;
        }
        let Some(address_definition) = definitions
            .get(value.id.0 as usize)
            .and_then(|definition| *definition)
        else {
            continue;
        };
        let Some(base_definition) = definitions
            .get(base.0 as usize)
            .and_then(|definition| *definition)
        else {
            errors.push(finding(
                function,
                format!(
                    "address value {} names array base value {} without a definition",
                    value.id.0, base.0
                ),
            ));
            continue;
        };
        if !definition_dominates_definition(base_definition, address_definition, dominators) {
            errors.push(finding(
                function,
                format!(
                    "array base value {} does not dominate address value {}",
                    base.0, value.id.0
                ),
            ));
        }
    }
}

fn verify_suspend_definition_boundaries(
    function: &l::Function,
    definitions: &[Option<DefinitionSite>],
    dominators: &[BTreeSet<l::BlockId>],
    errors: &mut Vec<VerifyError>,
) {
    for suspend_block in &function.blocks {
        let l::Terminator::Suspend { successor, .. } = suspend_block.terminator else {
            continue;
        };
        for block in &function.blocks {
            if !dominators
                .get(block.id.0 as usize)
                .is_some_and(|set| set.contains(&successor))
            {
                continue;
            }
            let mut uses = block
                .instructions
                .iter()
                .flat_map(|instruction| &instruction.operands)
                .filter_map(|operand| match operand {
                    l::Operand::Value(value) => Some(*value),
                    l::Operand::Constant(_) => None,
                })
                .collect::<Vec<_>>();
            uses.extend(terminator_values(&block.terminator));
            for value in uses {
                let Some(definition) = definitions
                    .get(value.0 as usize)
                    .and_then(|definition| *definition)
                else {
                    continue;
                };
                let inside_resume_region = match definition {
                    DefinitionSite::Entry => false,
                    DefinitionSite::BlockEntry(block) | DefinitionSite::Instruction(block, _) => {
                        dominators
                            .get(block.0 as usize)
                            .is_some_and(|set| set.contains(&successor))
                    }
                };
                if !inside_resume_region {
                    errors.push(finding(
                        function,
                        format!(
                            "use of value {} in block {} crosses suspend in block {} without a successor parameter",
                            value.0, block.id.0, suspend_block.id.0
                        ),
                    ));
                }
            }
        }
    }
}

fn definition_dominates_definition(
    definition: DefinitionSite,
    target: DefinitionSite,
    dominators: &[BTreeSet<l::BlockId>],
) -> bool {
    match (definition, target) {
        (DefinitionSite::Entry, _) => true,
        (DefinitionSite::BlockEntry(_), DefinitionSite::Entry)
        | (DefinitionSite::Instruction(_, _), DefinitionSite::Entry) => false,
        (DefinitionSite::BlockEntry(definition), DefinitionSite::BlockEntry(target)) => {
            definition == target
                || dominators
                    .get(target.0 as usize)
                    .is_some_and(|set| set.contains(&definition))
        }
        (DefinitionSite::BlockEntry(definition), DefinitionSite::Instruction(target, _)) => {
            definition == target
                || dominators
                    .get(target.0 as usize)
                    .is_some_and(|set| set.contains(&definition))
        }
        (
            DefinitionSite::Instruction(definition_block, _),
            DefinitionSite::BlockEntry(target_block),
        ) => {
            definition_block != target_block
                && dominators
                    .get(target_block.0 as usize)
                    .is_some_and(|set| set.contains(&definition_block))
        }
        (
            DefinitionSite::Instruction(definition_block, definition_index),
            DefinitionSite::Instruction(target_block, target_index),
        ) => {
            (definition_block == target_block && definition_index < target_index)
                || (definition_block != target_block
                    && dominators
                        .get(target_block.0 as usize)
                        .is_some_and(|set| set.contains(&definition_block)))
        }
    }
}

fn set_definition(
    definitions: &mut [Option<DefinitionSite>],
    value: l::ValueId,
    site: DefinitionSite,
) {
    if let Some(slot) = definitions.get_mut(value.0 as usize) {
        *slot = Some(site);
    }
}

fn check_dominates(
    function: &l::Function,
    value: l::ValueId,
    use_block: l::BlockId,
    use_index: usize,
    definitions: &[Option<DefinitionSite>],
    dominators: &[BTreeSet<l::BlockId>],
    errors: &mut Vec<VerifyError>,
) {
    let Some(definition) = definitions.get(value.0 as usize).and_then(|site| *site) else {
        return;
    };
    let use_site = DefinitionSite::Instruction(use_block, use_index);
    let valid = definition_dominates_definition(definition, use_site, dominators);
    if !valid {
        errors.push(finding(
            function,
            format!(
                "use of value {} in block {} is not dominated by its definition",
                value.0, use_block.0
            ),
        ));
    }
}

fn predecessors(function: &l::Function) -> Vec<Vec<l::BlockId>> {
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in block.terminator.successors() {
            if let Some(list) = predecessors.get_mut(successor.0 as usize) {
                list.push(block.id);
            }
        }
    }
    predecessors
}

fn dominators(
    function: &l::Function,
    predecessors: &[Vec<l::BlockId>],
) -> Vec<BTreeSet<l::BlockId>> {
    let all: BTreeSet<_> = function.blocks.iter().map(|block| block.id).collect();
    let mut sets = vec![all.clone(); function.blocks.len()];
    if let Some(entry) = sets.get_mut(function.entry.0 as usize) {
        *entry = [function.entry].into_iter().collect();
    }
    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            if block.id == function.entry {
                continue;
            }
            let preds = &predecessors[block.id.0 as usize];
            let mut next = if let Some(first) = preds.first() {
                sets[first.0 as usize].clone()
            } else {
                BTreeSet::new()
            };
            for pred in preds.iter().skip(1) {
                next.retain(|candidate| sets[pred.0 as usize].contains(candidate));
            }
            next.insert(block.id);
            if next != sets[block.id.0 as usize] {
                sets[block.id.0 as usize] = next;
                changed = true;
            }
        }
    }
    sets
}

fn successors(terminator: &l::Terminator) -> Vec<l::BlockId> {
    terminator.successors()
}

fn terminator_values(terminator: &l::Terminator) -> Vec<l::ValueId> {
    terminator.value_uses()
}

fn verify_address_invalidation(function: &l::Function, errors: &mut Vec<VerifyError>) {
    for value in &function.values {
        let l::ValueType::Address(address) = &value.ty else {
            continue;
        };
        let Some(base) = address.array_base else {
            continue;
        };
        let Some(start) = definition_position(function, value.id) else {
            continue;
        };
        let mut queue = VecDeque::from([(start.0, start.1, false)]);
        let mut seen = HashSet::new();
        while let Some((block_id, mut index, mut invalidated)) = queue.pop_front() {
            if !seen.insert((block_id, index, invalidated)) {
                continue;
            }
            let Some(block) = function.blocks.get(block_id.0 as usize) else {
                continue;
            };
            while index < block.instructions.len() {
                let instruction = &block.instructions[index];
                if instruction.result == Some(value.id) {
                    // A back edge executes the address definition again;
                    // the new dynamic address starts valid even if the
                    // preceding iteration invalidated its predecessor.
                    invalidated = false;
                }
                if invalidated && instruction_uses(instruction, value.id) {
                    errors.push(finding(
                        function,
                        format!(
                            "address value {} is used in block {} after array value {} was invalidated",
                            value.id.0, block_id.0, base.0
                        ),
                    ));
                    break;
                }
                if instruction.invalidates.contains(&base) {
                    invalidated = true;
                }
                index += 1;
            }
            if invalidated && terminator_values(&block.terminator).contains(&value.id) {
                errors.push(finding(
                    function,
                    format!(
                        "address value {} reaches block {} terminator after array value {} was invalidated",
                        value.id.0, block_id.0, base.0
                    ),
                ));
            }
            let term_invalidates = match &block.terminator {
                l::Terminator::Suspend { invalidates, .. } => invalidates.contains(&base),
                _ => false,
            };
            for successor in successors(&block.terminator) {
                queue.push_back((successor, 0, invalidated || term_invalidates));
            }
        }
    }
}

fn definition_position(function: &l::Function, value: l::ValueId) -> Option<(l::BlockId, usize)> {
    if function
        .parameters
        .iter()
        .any(|parameter| parameter.value == value)
    {
        return Some((function.entry, 0));
    }
    for block in &function.blocks {
        if block.parameters.contains(&value) {
            return Some((block.id, 0));
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            if instruction.result == Some(value) {
                return Some((block.id, index + 1));
            }
        }
    }
    None
}

fn instruction_uses(instruction: &l::Instruction, value: l::ValueId) -> bool {
    instruction.operands.contains(&l::Operand::Value(value))
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
