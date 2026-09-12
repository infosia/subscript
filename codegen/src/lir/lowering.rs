//! Module lowering: classes, foreign functions, globals, and the function table.

use super::*;

impl<'a> Lowering<'a> {
    pub(super) fn new(module: &'a hir::Module) -> Result<Self, LowerError> {
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

    pub(super) fn run(mut self) -> Result<l::Module, LowerError> {
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

    pub(super) fn method_record(
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

    pub(super) fn allocate_function_id(&mut self) -> l::FunctionId {
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

    pub(super) fn lower_function_input(
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
