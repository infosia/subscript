//! Typed HIR-to-LIR lowering and the mandatory LIR verifier.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;

use subscript_compiler::hir;
use subscript_compiler::lir as l;
use subscript_compiler::{ClassId, Pos, Type};

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

/// Lowers one complete typed HIR module to ordered LIR.
///
/// # Errors
///
/// Returns the first construct whose checked semantics cannot be encoded by
/// the closed LIR form.
pub fn lower_module(module: &hir::Module) -> Result<l::Module, LowerError> {
    let lowered = Lowering::new(module)?.run()?;
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
            for global in globals {
                let value = builder.require_expr(&global.init)?;
                let value = builder.coerce_operand(
                    value,
                    l::ValueType::Data(global.ty.clone()),
                    &global.pos,
                )?;
                let global_id = builder
                    .lowering
                    .globals
                    .get(&global.name)
                    .copied()
                    .ok_or_else(|| builder.error(&global.pos, "global id is missing"))?;
                builder.emit(
                    l::InstructionKind::StoreGlobal(global_id),
                    vec![value],
                    None,
                    false,
                    Vec::new(),
                    global.pos,
                )?;
            }
            builder.lower_statements(&builder.function.body.clone())?;
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
            intrinsic_operations: intrinsic_operations(),
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
        [
            "Spawn",
            "Post",
            "Poll",
            "Close",
            "Join",
            "InboxWait",
            "InboxPoll",
            "OutboxPost",
        ]
        .into_iter()
        .enumerate()
        .map(|(operation, semantic_name)| l::IntrinsicOperation {
            family: l::IntrinsicFamily::Worker,
            operation: operation as u16,
            semantic_name: semantic_name.to_string(),
        }),
    );
    table
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
                hir::TrapSite::JsonResultValue { .. } => l::TrapKind::JsonResultValue,
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
}

struct AddressTaken<'a> {
    module: &'a hir::Module,
    scopes: Vec<HashMap<String, BindingSite>>,
    taken: HashSet<BindingSite>,
}

fn address_taken_bindings(
    module: &hir::Module,
    function: &FunctionInput,
    captures: &[hir::Capture],
) -> HashSet<BindingSite> {
    let mut analysis = AddressTaken {
        module,
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
            hir::Stmt::Expr(expr) => self.expr(expr),
            hir::Stmt::Return {
                value: Some(value), ..
            } => self.expr(value),
            hir::Stmt::Return { value: None, .. } => {}
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
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
            _ => {}
        }
    }

    fn expr(&mut self, expr: &hir::Expr) {
        use hir::ExprKind as K;
        match &expr.kind {
            K::Unary { operand, .. }
            | K::Cast(operand)
            | K::JsonResultValue(operand)
            | K::Length(operand) => self.expr(operand),
            K::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            K::Assign { target, value, .. } => {
                match target.kind {
                    K::Local(_) | K::Global(_) => {}
                    _ => self.place(target),
                }
                self.expr(value);
            }
            K::Call { callee, args } => {
                match callee {
                    hir::Callee::Value(value) => self.expr(value),
                    hir::Callee::Method { recv, .. } if self.is_value_class(&recv.ty) => {
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
                    if parameter_types
                        .as_ref()
                        .and_then(|params| params.get(index))
                        .is_some_and(|parameter| {
                            self.is_boundary_struct_pointer(parameter)
                                && matches!(argument.ty, Type::Class(_))
                                && is_place_expr(argument)
                        })
                    {
                        self.place(argument);
                    } else {
                        self.expr(argument);
                    }
                }
            }
            K::New { class, args } => {
                let boundary_fields = self
                    .module
                    .classes
                    .get(class.0)
                    .filter(|definition| definition.is_boundary)
                    .map(|definition| &definition.fields);
                for (index, argument) in args.iter().enumerate() {
                    if boundary_fields
                        .and_then(|fields| fields.get(index))
                        .is_some_and(|field| {
                            self.is_boundary_struct_pointer(&field.ty)
                                && matches!(argument.ty, Type::Class(_))
                                && is_place_expr(argument)
                        })
                    {
                        self.place(argument);
                    } else {
                        self.expr(argument);
                    }
                }
            }
            K::DescriptorLit { fields, .. } => {
                for field in fields.iter().flatten() {
                    self.expr(field);
                }
            }
            K::Field { obj, .. } => self.expr(obj),
            K::Index { .. } => self.place(expr),
            K::ArrayLit(elements) => {
                for element in elements {
                    self.expr(element);
                }
            }
            K::ArraySpreadLit(elements) => {
                for element in elements {
                    self.expr(&element.expr);
                }
            }
            K::Template(parts) => {
                for part in parts {
                    if let hir::TplPart::Expr(expr) = part {
                        self.expr(expr);
                    }
                }
            }
            K::Yield(Some(value)) => self.expr(value),
            K::Yield(None) => {}
            K::AsyncCall { callee, args } => {
                if let Some(receiver) = callee.receiver() {
                    self.expr(receiver);
                }
                for argument in args {
                    self.expr(argument);
                }
            }
            K::Cond { cond, then, els } => {
                self.expr(cond);
                self.expr(then);
                self.expr(els);
            }
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::Local(_)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::RawNew { .. }
            | K::Lambda { .. }
            | K::AsyncSuspend => {}
            _ => {}
        }
    }

    fn place(&mut self, expr: &hir::Expr) {
        match &expr.kind {
            hir::ExprKind::Local(name) => self.mark(name),
            hir::ExprKind::Field { obj, .. } => {
                if self.is_stored_aggregate(&obj.ty) && is_place_expr(obj) {
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

    fn is_value_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if self.module.classes.get(id.0).is_some_and(|class| class.is_value))
    }

    fn is_stored_aggregate(&self, ty: &Type) -> bool {
        matches!(ty, Type::FixedArray(..) | Type::IterResult(_)) || self.is_value_class(ty)
    }

    fn is_boundary_struct_pointer(&self, ty: &Type) -> bool {
        matches!(ty, Type::Nullable(inner)
        if matches!(&**inner, Type::Class(class)
            if self.module.classes.get(class.0).is_some_and(|definition| {
                definition.is_value && definition.is_boundary
            })))
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

#[derive(Clone)]
struct PreparedPlace {
    kind: PreparedPlaceKind,
    traps: Vec<l::Trap>,
}

#[derive(Clone)]
enum PreparedPlaceKind {
    ExistingAddress(l::Operand, Type),
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

fn collect_place_traps(place: &PreparedPlace, traps: &mut Vec<l::Trap>) {
    traps.extend(place.traps.iter().cloned());
    let base = match &place.kind {
        PreparedPlaceKind::Field { base, .. } | PreparedPlaceKind::Index { base, .. } => base,
        PreparedPlaceKind::ExistingAddress(..)
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
        let address_taken = address_taken_bindings(lowering.hir, &function, &captures);
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
        let mut function = l::Function {
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
        thread_suspension_live_ins(&mut function)?;
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
            self.emit(
                l::InstructionKind::StoreLocal(local),
                vec![value.clone()],
                None,
                false,
                Vec::new(),
                pos.clone(),
            )?;
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
        if let Some(local) = entry.storage {
            self.emit(
                l::InstructionKind::StoreLocal(local),
                vec![value],
                None,
                false,
                traps,
                pos.clone(),
            )?;
        } else {
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
        self.scopes.pop();
        result
    }

    fn lower_statement(&mut self, statement: &hir::Stmt) -> Result<(), LowerError> {
        match statement {
            hir::Stmt::Let {
                name,
                ty,
                mutable,
                init,
                pos,
            } => {
                let value = self.require_expr(init)?;
                let value = self.coerce_operand(value, l::ValueType::Data(ty.clone()), pos)?;
                self.declare_binding(
                    name.clone(),
                    l::ValueType::Data(ty.clone()),
                    *mutable,
                    value,
                    pos.clone(),
                )?;
            }
            hir::Stmt::Expr(expr) => {
                self.lower_expr(expr)?;
            }
            hir::Stmt::Return { value, pos } => {
                let value = value
                    .as_ref()
                    .map(|value| {
                        let value = self.require_expr(value)?;
                        self.coerce_operand(
                            value,
                            l::ValueType::Data(self.function.ret.clone()),
                            pos,
                        )
                    })
                    .transpose()?;
                self.terminate(
                    l::Terminator::Return {
                        value,
                        pos: pos.clone(),
                    },
                    pos,
                )?;
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
                    .map(|control| control.break_target)
                    .ok_or_else(|| self.error(pos, "break has no enclosing target"))?;
                let edge = self.block_target(block, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
            hir::Stmt::Continue(pos) => {
                let block = self
                    .controls
                    .iter()
                    .rev()
                    .find_map(|control| control.continue_target)
                    .ok_or_else(|| self.error(pos, "continue has no enclosing loop"))?;
                let edge = self.block_target(block, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
            hir::Stmt::Block(statements) => self.lower_scoped(statements)?,
            other => {
                return Err(self.error(
                    &stmt_pos(other),
                    format!("unrecognized HIR statement form: {other:?}"),
                ));
            }
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
        self.scopes.pop();
        self.enter_block(exit)?;
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
                l::InstructionKind::IteratorCreate(kind),
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
        self.declare_binding(
            name.to_string(),
            l::ValueType::Data(ty.clone()),
            true,
            value,
            pos.clone(),
        )?;
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
        self.scopes.pop();
        self.enter_block(exit)?;
        Ok(())
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
                    .map(|element| {
                        let value = self.require_expr(element)?;
                        self.coerce_operand(
                            value,
                            l::ValueType::Data(element_type.clone()),
                            &element.pos,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit(
                    l::InstructionKind::ArrayLiteral,
                    operands,
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
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
                        let value = self.require_expr(&element.expr)?;
                        if element.spread.is_none() {
                            self.coerce_operand(
                                value,
                                l::ValueType::Data((**element_type).clone()),
                                &element.expr.pos,
                            )
                        } else {
                            Ok(value)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let spreads = elements
                    .iter()
                    .map(|element| element.spread.map(convert_spread))
                    .collect();
                self.emit(
                    l::InstructionKind::ArraySpreadLiteral(spreads),
                    operands,
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
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
                            lowered_parts.push(l::TemplatePart::Operand(index));
                        }
                        other => {
                            return Err(self.error(
                                &expr.pos,
                                format!("unrecognized template part: {other:?}"),
                            ));
                        }
                    }
                }
                self.emit(
                    l::InstructionKind::Template(lowered_parts),
                    operands,
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
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
            K::Cond { cond, then, els } => Some(self.lower_cond(cond, then, els, expr)?),
            other => {
                return Err(self.error(
                    &expr.pos,
                    format!("unrecognized HIR expression form: {other:?}"),
                ));
            }
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
        let then_value = self.require_expr(then)?;
        let then_value =
            self.coerce_operand(then_value, l::ValueType::Data(expr.ty.clone()), &then.pos)?;
        let edge = self.block_target(merge, vec![then_value])?;
        self.terminate(l::Terminator::Branch(edge), &then.pos)?;
        self.restore_bindings(&branch_state);
        self.current = Some(else_block);
        let else_value = self.require_expr(els)?;
        let else_value =
            self.coerce_operand(else_value, l::ValueType::Data(expr.ty.clone()), &els.pos)?;
        let edge = self.block_target(merge, vec![else_value])?;
        self.terminate(l::Terminator::Branch(edge), &els.pos)?;
        self.enter_block(merge)?;
        Ok(l::Operand::Value(
            self.blocks[merge.0 as usize].parameters[0],
        ))
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
            let value = self.require_expr(value_expr)?;
            let result = if let Some(op) = op {
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
                self.coerce_operand(
                    value,
                    l::ValueType::Data(target_expr.ty.clone()),
                    &target_expr.pos,
                )?
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
        let value = self.require_expr(value_expr)?;
        let result = if let Some(op) = op {
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
            value
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

        let (parameter_types, return_type) =
            self.declared_hir_call_signature(callee, args, expr)?;
        let (kind, mut operands, mut params, receiver_for_defaults) =
            self.resolve_call(callee, expr)?;
        if params.is_empty()
            && matches!(
                kind,
                l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
            )
        {
            params = args
                .iter()
                .enumerate()
                .map(|(index, argument)| CallParam {
                    name: format!("arg{index}"),
                    ty: argument.ty.clone(),
                    default: None,
                    pos: argument.pos.clone(),
                })
                .collect();
        }
        let explicit = self.lower_call_arguments(
            &params,
            args,
            receiver_for_defaults.as_ref(),
            matches!(kind, l::CallTargetKind::Foreign(_)),
        )?;
        operands.extend(explicit);
        if matches!(kind, l::CallTargetKind::Method(_)) {
            if let Some(PreparedBase::Place(place)) = receiver_for_defaults {
                operands[0] = self.materialize_address_inner(&place, &expr.pos, false)?;
            }
        }
        let target = l::CallTarget {
            kind,
            parameter_types,
            return_type: return_type.clone(),
        };
        self.emit(
            l::InstructionKind::Call(target),
            operands,
            return_type,
            true,
            convert_traps(&expr.trap_sites(self.lowering.hir)),
            expr.pos.clone(),
        )
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
                worker_operation(*value),
                None,
                match value {
                    hir::WorkerFn::Spawn(index) => Some(*index as u32),
                    _ => None,
                },
            )),
            other => Err(self.error(
                &expr.pos,
                format!("unrecognized HIR callee form: {other:?}"),
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
                if self.is_place_expr(recv) {
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
                self.lower_argument_value(&parameter.ty, argument)?
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
                let lowered = self.require_expr(default);
                self.this_value = saved_this;
                self.substitutions.pop();
                lowered?
            };
            let actual = self.operand_type(&value, &parameter.pos)?;
            let expected = l::ValueType::Data(parameter.ty.clone());
            let value = if actual == expected {
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
                    let later_suspends = args
                        .get(index + 1..)
                        .is_some_and(|later| later.iter().any(hir_expr_suspends));
                    if later_suspends {
                        delayed_array_snapshots.push((index, value, (**element).clone(), pos));
                        operand_groups.push(Vec::new());
                    } else {
                        operand_groups
                            .push(self.foreign_array_snapshot(value, element, pos)?.to_vec());
                    }
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
            .filter(|trap| trap.kind == l::TrapKind::Allocation)
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
                let value = self.require_expr(initializer);
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
                self.store_class_field(class_id, index, allocated.clone(), value, &field.pos)?;
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
            self.emit(
                l::InstructionKind::Call(l::CallTarget {
                    kind: l::CallTargetKind::Method(record.method.expect("constructor method id")),
                    parameter_types,
                    return_type: None,
                }),
                operands,
                None,
                true,
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
                self.require_expr(value)?
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
                let value = self.require_expr(default);
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
        self.emit(
            l::InstructionKind::StoreAddress,
            vec![address, value],
            None,
            false,
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
        if !self.is_boundary_struct_pointer(expected) || !matches!(argument.ty, Type::Class(_)) {
            return self.require_expr(argument);
        }
        if is_place_expr(argument) {
            let place = self.prepare_place(argument)?;
            return self.materialize_address(&place, &argument.pos);
        }
        let value = self.require_expr(argument)?;
        self.emit(
            l::InstructionKind::AddressOfValue,
            vec![value],
            Some(l::ValueType::Address(l::AddressType {
                pointee: argument.ty.clone(),
                array_base: None,
            })),
            false,
            Vec::new(),
            argument.pos.clone(),
        )?
        .ok_or_else(|| self.error(&argument.pos, "boundary pointer address produced no value"))
    }

    fn lower_lambda(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[hir::Capture],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
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
        self.emit(
            l::InstructionKind::MakeClosure(id),
            capture_values,
            Some(l::ValueType::Data(expr.ty.clone())),
            false,
            convert_traps(&expr.trap_sites(self.lowering.hir)),
            expr.pos.clone(),
        )?
        .ok_or_else(|| self.error(&expr.pos, "lambda produced no function value"))
    }

    fn lower_async_call(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let (kind, mut operands, params) = match callee {
            hir::AsyncCallee::Function(name) => {
                let record = self
                    .lowering
                    .free_functions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| {
                        self.error(&expr.pos, format!("unknown async function `{name}`"))
                    })?;
                let function = self
                    .lowering
                    .hir
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .cloned()
                    .ok_or_else(|| self.error(&expr.pos, "async function body is missing"))?;
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
                let record = self.lowering.method_record(class.0, name, &expr.pos)?;
                let function = self
                    .lowering
                    .hir
                    .classes
                    .get(class.0)
                    .and_then(|class| class.methods.iter().find(|method| method.name == *name))
                    .cloned()
                    .ok_or_else(|| self.error(&expr.pos, "async method body is missing"))?;
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
            other => {
                return Err(self.error(&expr.pos, format!("unrecognized async callee: {other:?}")));
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
        operands.extend(self.lower_call_arguments(&params, args, None, false)?);
        let return_type = (expr.ty != Type::Void).then(|| l::ValueType::Data(expr.ty.clone()));
        let target = l::CallTarget {
            kind,
            parameter_types,
            return_type: return_type.clone(),
        };
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
                let local = self.bindings[binding.0].storage.ok_or_else(|| {
                    self.error(
                        &expr.pos,
                        format!("local `{name}` is used as an address but has no storage"),
                    )
                })?;
                PreparedPlaceKind::Local(local, expr.ty.clone())
            }
            hir::ExprKind::Global(name) => {
                let global = self
                    .lowering
                    .globals
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown global `{name}`")))?;
                PreparedPlaceKind::Global(global, expr.ty.clone())
            }
            hir::ExprKind::Field { obj, name } => {
                let base = if self.is_stored_aggregate(&obj.ty) && self.is_place_expr(obj) {
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
                let base = if matches!(obj.ty, Type::FixedArray(..)) && self.is_place_expr(obj) {
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
        Ok(PreparedPlace { kind, traps })
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
                    l::InstructionKind::AddressOfIndex { checked: *checked },
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
        let value = self.coerce_operand(
            value,
            l::ValueType::Data(self.place_type(place).clone()),
            pos,
        )?;
        match &place.kind {
            PreparedPlaceKind::Local(local, _) => {
                self.emit(
                    l::InstructionKind::StoreLocal(*local),
                    vec![value],
                    None,
                    false,
                    place.traps.clone(),
                    pos.clone(),
                )?;
            }
            PreparedPlaceKind::Global(global, _) => {
                self.emit(
                    l::InstructionKind::StoreGlobal(*global),
                    vec![value],
                    None,
                    false,
                    place.traps.clone(),
                    pos.clone(),
                )?;
            }
            _ => {
                let address = self.materialize_address(place, pos)?;
                self.emit(
                    l::InstructionKind::StoreAddress,
                    vec![address, value],
                    None,
                    false,
                    Vec::new(),
                    pos.clone(),
                )?;
            }
        }
        Ok(())
    }

    fn place_type<'p>(&self, place: &'p PreparedPlace) -> &'p Type {
        match &place.kind {
            PreparedPlaceKind::ExistingAddress(_, ty)
            | PreparedPlaceKind::Local(_, ty)
            | PreparedPlaceKind::Global(_, ty)
            | PreparedPlaceKind::Field { ty, .. }
            | PreparedPlaceKind::Index { ty, .. } => ty,
        }
    }

    fn is_place_expr(&self, expr: &hir::Expr) -> bool {
        matches!(
            expr.kind,
            hir::ExprKind::Local(_)
                | hir::ExprKind::Global(_)
                | hir::ExprKind::Field { .. }
                | hir::ExprKind::Index { .. }
        )
    }

    fn is_stored_aggregate(&self, ty: &Type) -> bool {
        match ty {
            Type::FixedArray(..) | Type::IterResult(_) => true,
            Type::Class(id) => self
                .lowering
                .hir
                .classes
                .get(id.0)
                .is_some_and(|class| class.is_value),
            _ => false,
        }
    }

    fn is_boundary_struct_pointer(&self, ty: &Type) -> bool {
        matches!(ty, Type::Nullable(inner)
        if matches!(&**inner, Type::Class(class)
            if self.lowering.hir.classes.get(class.0).is_some_and(|definition| {
                definition.is_value && definition.is_boundary
            })))
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
        if self.operand_type(&operand, pos)? == expected {
            return Ok(operand);
        }
        self.emit(
            l::InstructionKind::Coerce,
            vec![operand],
            Some(expected),
            false,
            Vec::new(),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "implicit coercion produced no value"))
    }
}

fn hir_expr_suspends(expr: &hir::Expr) -> bool {
    use hir::ExprKind as K;
    match &expr.kind {
        K::Yield(_) | K::AsyncSuspend | K::AsyncCall { .. } => true,
        K::Unary { operand, .. }
        | K::Cast(operand)
        | K::Field { obj: operand, .. }
        | K::JsonResultValue(operand)
        | K::Length(operand) => hir_expr_suspends(operand),
        K::Binary { left, right, .. }
        | K::Assign {
            target: left,
            value: right,
            ..
        }
        | K::Index {
            obj: left,
            index: right,
            ..
        } => hir_expr_suspends(left) || hir_expr_suspends(right),
        K::Call { callee, args } => {
            let callee_suspends = match callee {
                hir::Callee::Value(value) => hir_expr_suspends(value),
                hir::Callee::Method { recv, .. } => hir_expr_suspends(recv),
                _ => false,
            };
            callee_suspends || args.iter().any(hir_expr_suspends)
        }
        K::New { args, .. } | K::ArrayLit(args) => args.iter().any(hir_expr_suspends),
        K::DescriptorLit { fields, .. } => fields.iter().flatten().any(hir_expr_suspends),
        K::ArraySpreadLit(elements) => elements
            .iter()
            .any(|element| hir_expr_suspends(&element.expr)),
        K::Template(parts) => parts
            .iter()
            .any(|part| matches!(part, hir::TplPart::Expr(expr) if hir_expr_suspends(expr))),
        K::Cond { cond, then, els } => {
            hir_expr_suspends(cond) || hir_expr_suspends(then) || hir_expr_suspends(els)
        }
        _ => false,
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

fn worker_operation(value: hir::WorkerFn) -> u16 {
    match value {
        hir::WorkerFn::Spawn(_) => 0,
        hir::WorkerFn::Post => 1,
        hir::WorkerFn::Poll => 2,
        hir::WorkerFn::Close => 3,
        hir::WorkerFn::Join => 4,
        hir::WorkerFn::InboxWait => 5,
        hir::WorkerFn::InboxPoll => 6,
        hir::WorkerFn::OutboxPost => 7,
        _ => u16::MAX,
    }
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
        _ => Pos::new("<statement>", 1, 1),
    }
}

fn target(block: l::BlockId, arguments: Vec<l::Operand>) -> l::BlockTarget {
    l::BlockTarget { block, arguments }
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
                source_name: definition.source_name,
            });
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
                    if pred_versions.is_empty() || pred_versions.iter().any(Option::is_none) {
                        None
                    } else {
                        let versions = pred_versions.into_iter().flatten().collect::<BTreeSet<_>>();
                        if versions.len() == 1 {
                            versions.first().copied()
                        } else if live_in[block_index].contains(&origin) {
                            let definition = function.values[origin.0 as usize].clone();
                            let parameter = l::ValueId(function.values.len() as u32);
                            function.values.push(l::Value {
                                id: parameter,
                                ty: definition.ty,
                                source_name: definition.source_name,
                            });
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
    Ok(())
}

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
    match terminator {
        l::Terminator::Branch(target) => {
            replace_operands(&mut target.arguments, original, replacement);
        }
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            replace_operands(std::slice::from_mut(condition), original, replacement);
            replace_operands(&mut then_target.arguments, original, replacement);
            replace_operands(&mut else_target.arguments, original, replacement);
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
            replace_operands(std::slice::from_mut(value), original, replacement);
            for arm in arms {
                replace_operands(&mut arm.target.arguments, original, replacement);
            }
            replace_operands(&mut default.arguments, original, replacement);
        }
        l::Terminator::Return { value, .. } => {
            if let Some(value) = value {
                replace_operands(std::slice::from_mut(value), original, replacement);
            }
        }
        l::Terminator::Suspend {
            kind,
            arguments,
            invalidates,
            ..
        } => {
            replace_operands(arguments, original, replacement);
            replace_ids(invalidates, original, replacement);
            match kind {
                l::SuspendKind::Yield(value) => {
                    if value == &Some(original) {
                        *value = replacement;
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    replace_ids(operands, original, replacement);
                }
            }
        }
        l::Terminator::Trap(_) | l::Terminator::Unreachable { .. } => {}
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
    verify_dominance(function, errors);
    verify_address_invalidation(function, errors);
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
    match &instruction.kind {
        l::InstructionKind::Copy => {
            if operand_types.len() != 1 || result_type.as_ref() != operand_types.first() {
                bad("copy input/result types do not match", errors);
            }
        }
        l::InstructionKind::Coerce => {
            let data_coercion = matches!(
                (operand_types.first(), result_type.as_ref()),
                (Some(l::ValueType::Data(_)), Some(l::ValueType::Data(_)))
            );
            let boundary_address_coercion = matches!(
                (operand_types.first(), result_type.as_ref()),
                (
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: Type::Class(source),
                        ..
                    })),
                    Some(l::ValueType::Data(Type::Nullable(target)))
                ) if matches!(&**target, Type::Class(target)
                    if source == target
                        && module.classes.get(target.0).is_some_and(|class| {
                            class.is_value && class.is_boundary
                        }))
            );
            if operand_types.len() != 1 || (!data_coercion && !boundary_address_coercion) {
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
            if !call_parameters_match(&operand_types, &target.parameter_types)
                || result_type != target.return_type
            {
                bad("call signature disagrees with its target", errors);
            }
            if let Some((parameters, result)) =
                declared_call_signature(module, &target.kind, &operand_types)
            {
                if !call_parameters_match(&target.parameter_types, &parameters)
                    || target.return_type != result
                {
                    bad(
                        "call signature disagrees with the target declaration",
                        errors,
                    );
                }
            }
            let target_exists = match &target.kind {
                l::CallTargetKind::Function(id) => declared_function(module, *id).is_some(),
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
                l::CallTargetKind::BuiltinMethod(_) => !target.parameter_types.is_empty(),
            };
            if !target_exists {
                bad("call target identity/signature is invalid", errors);
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
        l::InstructionKind::AddressOfIndex { .. } => {
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
            if operand_types.len() != 2
                || !valid_index
                || element != result_element
                || expected_base != result_base
            {
                bad("index address signature is invalid", errors);
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
                l::TemplatePart::Operand(index) => (*index as usize) < operand_types.len(),
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
        l::InstructionKind::IteratorCreate(kind) => {
            let valid = matches!(
                result_type.as_ref(),
                Some(l::ValueType::Iterator(iterator)) if iterator.kind == *kind
            );
            if operand_types.len() != 1 || !valid {
                bad("iterator creation signature is invalid", errors);
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

fn field_base_accepts(module: &l::Module, field: l::FieldRef, base: &l::ValueType) -> bool {
    match field {
        l::FieldRef::Class(id) => {
            let owner = module
                .classes
                .iter()
                .find(|class| class.fields.iter().any(|candidate| candidate.id == id));
            owner.is_some_and(|class| match base {
                l::ValueType::Data(Type::Class(id)) => *id == class.id,
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

fn declared_call_signature(
    module: &l::Module,
    kind: &l::CallTargetKind,
    operand_types: &[l::ValueType],
) -> Option<(Vec<l::ValueType>, Option<l::ValueType>)> {
    match kind {
        l::CallTargetKind::Function(id) => declared_function(module, *id).map(function_signature),
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
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_) => None,
    }
}

fn verify_terminator_types(
    module: &l::Module,
    function: &l::Function,
    block: &l::BasicBlock,
    errors: &mut Vec<VerifyError>,
) {
    match &block.terminator {
        l::Terminator::Branch(target) => verify_edge(function, block, target, false, errors),
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            verify_operand_type(
                function,
                condition,
                &l::ValueType::Data(Type::Bool),
                &format!("block {} conditional", block.id.0),
                errors,
            );
            verify_edge(function, block, then_target, false, errors);
            verify_edge(function, block, else_target, false, errors);
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
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
                verify_edge(function, block, &arm.target, false, errors);
            }
            verify_edge(function, block, default, false, errors);
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
                        if !call_parameters_match(&target.parameter_types, &parameters)
                            || target.return_type != result
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
    skip_resume: bool,
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
    let parameters = if skip_resume {
        destination.parameters.get(1..).unwrap_or(&[])
    } else {
        destination.parameters.as_slice()
    };
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
    let valid = match definition {
        DefinitionSite::Entry => true,
        DefinitionSite::BlockEntry(block) => {
            block == use_block
                || dominators
                    .get(use_block.0 as usize)
                    .is_some_and(|set| set.contains(&block))
        }
        DefinitionSite::Instruction(block, index) => {
            (block == use_block && index < use_index)
                || (block != use_block
                    && dominators
                        .get(use_block.0 as usize)
                        .is_some_and(|set| set.contains(&block)))
        }
    };
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
        for successor in successors(&block.terminator) {
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
    match terminator {
        l::Terminator::Branch(target) => vec![target.block],
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => vec![then_target.block, else_target.block],
        l::Terminator::Switch { arms, default, .. } => arms
            .iter()
            .map(|arm| arm.target.block)
            .chain(std::iter::once(default.block))
            .collect(),
        l::Terminator::Suspend { successor, .. } => vec![*successor],
        l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_) => Vec::new(),
    }
}

fn terminator_values(terminator: &l::Terminator) -> Vec<l::ValueId> {
    let mut values = Vec::new();
    let mut push_operand = |operand: &l::Operand| {
        if let l::Operand::Value(value) = operand {
            values.push(*value);
        }
    };
    match terminator {
        l::Terminator::Branch(target) => target.arguments.iter().for_each(&mut push_operand),
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            push_operand(condition);
            then_target.arguments.iter().for_each(&mut push_operand);
            else_target.arguments.iter().for_each(&mut push_operand);
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
            push_operand(value);
            for arm in arms {
                arm.target.arguments.iter().for_each(&mut push_operand);
            }
            default.arguments.iter().for_each(&mut push_operand);
        }
        l::Terminator::Return { value, .. } => {
            if let Some(value) = value {
                push_operand(value);
            }
        }
        l::Terminator::Suspend {
            kind, arguments, ..
        } => {
            arguments.iter().for_each(&mut push_operand);
            match kind {
                l::SuspendKind::Yield(value) => {
                    if let Some(value) = value {
                        values.push(*value);
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    values.extend(operands.iter().copied());
                }
            }
        }
        l::Terminator::Trap(_) | l::Terminator::Unreachable { .. } => {}
    }
    values
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
                mutable: true,
                pos: pos(),
            }],
            values: vec![
                l::Value {
                    id: l::ValueId(0),
                    ty: l::ValueType::Data(array_type.clone()),
                    source_name: Some("array".to_string()),
                },
                l::Value {
                    id: l::ValueId(1),
                    ty: address_type.clone(),
                    source_name: None,
                },
                l::Value {
                    id: l::ValueId(2),
                    ty: l::ValueType::Data(Type::I32),
                    source_name: None,
                },
            ],
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
                        traps: Vec::new(),
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
                source_name: Some("value".to_string()),
            }],
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
                    source_name: None,
                },
                l::Value {
                    id: l::ValueId(1),
                    ty: l::ValueType::Data(Type::Bool),
                    source_name: None,
                },
            ],
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
                source_name: Some(format!("arg{index}")),
            })
            .collect::<Vec<_>>();
        let result = actual_return.map(|ty| {
            let id = l::ValueId(values.len() as u32);
            values.push(l::Value {
                id,
                ty,
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
            blocks: vec![l::BasicBlock {
                id: l::BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: vec![l::Instruction {
                    result,
                    kind: l::InstructionKind::Call(l::CallTarget {
                        kind,
                        parameter_types: declared_parameters,
                        return_type: declared_return,
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

    #[test]
    fn valid_address_graph_passes() {
        verify_module(&base_module()).expect("valid graph");
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
                source_name: Some("before".to_string()),
            },
            l::Value {
                id: l::ValueId(1),
                ty: l::ValueType::Data(Type::I32),
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
            .contains("call signature disagrees with its target")));
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
            .contains("call signature disagrees with its target")));
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
