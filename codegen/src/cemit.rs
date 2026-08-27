//! LIR-to-C transcriber for the shipping tier.
//!
//! LIR fixes evaluation order, control flow, entity identity, traps, and
//! suspension state. This module assigns C storage and writes those blocks
//! and instructions without consulting typed HIR.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use subscript_compiler::lir as l;
use subscript_compiler::types::{ClassId, Type};
use subscript_compiler::Pos;
use subscript_runtime::context as rtc;
use subscript_runtime::TrapKind;

use crate::lir::{lir_live_ins, verify_module};

/// An emitted C translation unit and its source-position metadata.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CProgram {
    /// Complete C source.
    pub source: String,
    /// Trap position table indexed by generated `pos_id` values.
    pub positions: Vec<Pos>,
    /// Generated allocation metadata declarations.
    pub allocation_metadata_header: String,
    /// Generated allocation metadata definitions.
    pub allocation_metadata_source: String,
    pub(crate) foreign_symbols: Vec<String>,
}

/// Transcribes one verified LIR module to ship-tier C.
pub(crate) fn emit_lir_c(module: &l::Module, require_main: bool) -> Result<CProgram, String> {
    verify_module(module).map_err(|errors| {
        format!(
            "internal error: LIR verification failed before C transcription:\n{}",
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    Emitter::new(module).emit(require_main)
}

fn internal(message: impl AsRef<str>) -> String {
    format!("internal error: {}", message.as_ref())
}

fn data_type(ty: &l::ValueType) -> Result<&Type, String> {
    match ty {
        l::ValueType::Data(ty) => Ok(ty),
        other => Err(internal(format!("expected a data type, found {other:?}"))),
    }
}

fn explicit_parameters(function: &l::Function) -> impl Iterator<Item = &l::Parameter> {
    function
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == l::ParameterKind::Explicit)
}

fn capture_parameters(function: &l::Function) -> impl Iterator<Item = &l::Parameter> {
    function
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == l::ParameterKind::Capture)
}

fn runtime_traps(function: &l::Function) -> Vec<l::Trap> {
    let mut traps = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            traps.extend(instruction.traps.iter().cloned());
        }
        match &block.terminator {
            l::Terminator::Trap(trap) => traps.push(trap.clone()),
            l::Terminator::Unreachable { .. } => {}
            l::Terminator::Suspend { traps: sites, .. } => traps.extend(sites.iter().cloned()),
            l::Terminator::Branch(_)
            | l::Terminator::ConditionalBranch { .. }
            | l::Terminator::Switch { .. }
            | l::Terminator::Return { .. } => {}
        }
    }
    traps
}

fn verify_trap_consumption(
    function: &l::Function,
    expected: &[l::Trap],
    consumed: &[l::Trap],
) -> Result<(), String> {
    let mut matched = vec![false; consumed.len()];
    let mut missing = Vec::new();
    for trap in expected {
        if let Some(index) = consumed
            .iter()
            .zip(&matched)
            .position(|(candidate, matched)| !matched && candidate == trap)
        {
            matched[index] = true;
        } else {
            missing.push(trap);
        }
    }
    let extra = consumed
        .iter()
        .zip(matched)
        .filter_map(|(trap, matched)| (!matched).then_some(trap))
        .collect::<Vec<_>>();
    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }
    let site = missing
        .first()
        .copied()
        .or_else(|| extra.first().copied())
        .map_or(&function.pos, |trap| &trap.pos);
    Err(internal(format!(
        "function {} `{}` trap-consumption mismatch at {site}: LIR carries {} site(s), transcriber consumed {}; missing {missing:?}; extra {extra:?}",
        function.id.0,
        function.source_name,
        expected.len(),
        consumed.len()
    )))
}

struct Emitter<'m> {
    module: &'m l::Module,
    positions: Vec<Pos>,
    runtime_symbols: BTreeMap<String, (String, Vec<String>)>,
    foreign_symbols: Vec<String>,
    field_owners: HashMap<l::FieldId, (ClassId, usize)>,
    helper_prototypes: String,
    helpers: String,
    helper_count: u32,
}

struct BoundaryPtrWriteback {
    class: ClassId,
    source: String,
    scratch: String,
}

impl<'m> Emitter<'m> {
    fn new(module: &'m l::Module) -> Self {
        let field_owners = module
            .classes
            .iter()
            .flat_map(|class| {
                class
                    .fields
                    .iter()
                    .enumerate()
                    .map(move |(index, field)| (field.id, (class.id, index)))
            })
            .collect();
        Self {
            module,
            positions: Vec::new(),
            runtime_symbols: BTreeMap::new(),
            foreign_symbols: Vec::new(),
            field_owners,
            helper_prototypes: String::new(),
            helpers: String::new(),
            helper_count: 0,
        }
    }

    fn pos_id(&mut self, pos: &Pos) -> u32 {
        self.positions.push(pos.clone());
        (self.positions.len() - 1) as u32
    }

    fn runtime_call(
        &mut self,
        return_type: &str,
        name: &str,
        argument_types: &[String],
        arguments: &[String],
    ) -> String {
        self.runtime_symbols
            .entry(name.to_string())
            .or_insert_with(|| (return_type.to_string(), argument_types.to_vec()));
        format!("{name}({})", arguments.join(", "))
    }

    fn class(&self, id: ClassId) -> Result<&l::Class, String> {
        self.module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .ok_or_else(|| internal(format!("class {} is missing", id.0)))
    }

    fn field(&self, id: l::FieldId) -> Result<(ClassId, usize, &l::Field), String> {
        let (class, index) = self
            .field_owners
            .get(&id)
            .copied()
            .ok_or_else(|| internal(format!("field {} is missing", id.0)))?;
        let field = self
            .class(class)?
            .fields
            .get(index)
            .ok_or_else(|| internal(format!("field {} has no declaration", id.0)))?;
        Ok((class, index, field))
    }

    fn function(&self, id: l::FunctionId) -> Result<&l::Function, String> {
        self.module
            .functions
            .get(id.0 as usize)
            .filter(|function| function.id == id)
            .ok_or_else(|| internal(format!("function {} is missing", id.0)))
    }

    fn method_function(&self, id: l::MethodId) -> Result<l::FunctionId, String> {
        self.module
            .classes
            .iter()
            .flat_map(|class| class.constructor.iter().chain(&class.methods))
            .find(|method| method.id == id)
            .map(|method| method.function)
            .ok_or_else(|| internal(format!("method {} is missing", id.0)))
    }

    fn operation_name(&self, intrinsic: &l::Intrinsic) -> Result<&str, String> {
        self.module
            .intrinsic_operations
            .iter()
            .find(|operation| {
                operation.family == intrinsic.family && operation.operation == intrinsic.operation
            })
            .map(|operation| operation.semantic_name.as_str())
            .ok_or_else(|| {
                internal(format!(
                    "intrinsic {:?}.{} is missing from the module table",
                    intrinsic.family, intrinsic.operation
                ))
            })
    }

    fn is_value_class(&self, id: ClassId) -> Result<bool, String> {
        Ok(self.class(id)?.is_value)
    }

    fn class_name(&self, id: ClassId) -> String {
        format!("SubC{}", id.0)
    }

    fn fixed_array_name(&self, element: &Type, count: u32) -> Result<String, String> {
        Ok(format!("SubFA_{}_{count}", self.type_tag(element)?))
    }

    fn iter_result_name(&self, value: &Type) -> Result<String, String> {
        Ok(format!("SubIR_{}", self.type_tag(value)?))
    }

    fn type_tag(&self, ty: &Type) -> Result<String, String> {
        Ok(match ty {
            Type::I8 => "i8".into(),
            Type::U8 => "u8".into(),
            Type::I16 => "i16".into(),
            Type::U16 => "u16".into(),
            Type::F16 => "f16".into(),
            Type::I32 => "i32".into(),
            Type::U32 => "u32".into(),
            Type::I64 => "i64".into(),
            Type::U64 => "u64".into(),
            Type::Date => "date".into(),
            Type::F32 => "f32".into(),
            Type::F64 => "f64".into(),
            Type::Bool => "bool".into(),
            Type::Enum(id) => format!("e{}", id.0),
            Type::StringAlias(id) => format!("s{}", id.0),
            Type::Class(id) if self.is_value_class(*id)? => format!("c{}", id.0),
            Type::FixedArray(element, count) => {
                format!("fa{}_{}", self.type_tag(element)?, count)
            }
            Type::IterResult(value) => format!("ir{}", self.type_tag(value)?),
            Type::Func(_) => "fn".into(),
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Generator(_)
            | Type::Nullable(_)
            | Type::Null
            | Type::Class(_) => "ptr".into(),
            other => return Err(internal(format!("type tag for {other:?}"))),
        })
    }

    fn ctype(&self, ty: &Type) -> Result<String, String> {
        Ok(match ty {
            Type::I8 => "int8_t".into(),
            Type::U8 => "uint8_t".into(),
            Type::I16 => "int16_t".into(),
            Type::U16 | Type::F16 => "uint16_t".into(),
            Type::I32 | Type::Enum(_) | Type::StringAlias(_) => "int32_t".into(),
            Type::U32 => "uint32_t".into(),
            Type::I64 | Type::Date => "int64_t".into(),
            Type::U64 => "uint64_t".into(),
            Type::F32 => "float".into(),
            Type::F64 => "double".into(),
            Type::Bool => "int32_t".into(),
            Type::Void => "void".into(),
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Generator(_)
            | Type::Nullable(_)
            | Type::Null => "void*".into(),
            Type::Func(_) => "SubFn".into(),
            Type::Class(id) if self.is_value_class(*id)? => self.class_name(*id),
            Type::Class(_) => "void*".into(),
            Type::FixedArray(element, count) => self.fixed_array_name(element, *count)?,
            Type::IterResult(value) => self.iter_result_name(value)?,
            other => return Err(internal(format!("C type for {other:?}"))),
        })
    }

    fn value_ctype(&self, ty: &l::ValueType) -> Result<String, String> {
        match ty {
            l::ValueType::Data(ty) => self.ctype(ty),
            l::ValueType::Address(address) => Ok(format!("{}*", self.ctype(&address.pointee)?)),
            l::ValueType::Iterator(_) => Ok("SubIter".into()),
        }
    }

    fn zero(&self, ty: &l::ValueType) -> Result<String, String> {
        Ok(match ty {
            l::ValueType::Address(_) => "NULL".into(),
            l::ValueType::Iterator(_) => "(SubIter){0}".into(),
            l::ValueType::Data(Type::F32) => "0.0f".into(),
            l::ValueType::Data(Type::F64) => "0.0".into(),
            l::ValueType::Data(
                ty @ (Type::Class(_) | Type::FixedArray(_, _) | Type::IterResult(_)),
            ) if !matches!(ty, Type::Class(id) if !self.is_value_class(*id)?) => {
                format!("({}){{0}}", self.ctype(ty)?)
            }
            l::ValueType::Data(Type::Func(_)) => "(SubFn){0}".into(),
            _ => "0".into(),
        })
    }

    fn type_contains_managed(
        &self,
        ty: &Type,
        visiting: &mut HashSet<ClassId>,
    ) -> Result<bool, String> {
        Ok(match ty {
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Generator(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Func(_) => true,
            Type::Nullable(inner) => self.type_contains_managed(inner, visiting)?,
            Type::Class(id) => {
                let class = self.class(*id)?;
                if !class.is_value {
                    true
                } else if !visiting.insert(*id) {
                    false
                } else {
                    let result = class
                        .fields
                        .iter()
                        .map(|field| self.type_contains_managed(&field.ty, visiting))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .any(|contains| contains);
                    visiting.remove(id);
                    result
                }
            }
            Type::FixedArray(element, _) | Type::IterResult(element) => {
                self.type_contains_managed(element, visiting)?
            }
            _ => false,
        })
    }

    fn value_contains_managed(&self, ty: &l::ValueType) -> Result<bool, String> {
        match ty {
            l::ValueType::Data(ty) => self.type_contains_managed(ty, &mut HashSet::new()),
            l::ValueType::Iterator(iterator) => Ok(matches!(
                iterator.kind,
                l::ForOfKind::ArrayValues
                    | l::ForOfKind::ArrayKeys
                    | l::ForOfKind::MapKeys
                    | l::ForOfKind::MapValues
                    | l::ForOfKind::SetValues
                    | l::ForOfKind::StringCodePoints
            )),
            l::ValueType::Address(_) => Ok(false),
        }
    }

    fn emit(mut self, require_main: bool) -> Result<CProgram, String> {
        if require_main && self.module.entry.is_none() {
            return Err("no exported `main(): void` entry point".into());
        }

        let mut types = String::new();
        self.emit_types(&mut types)?;
        let mut globals = String::new();
        self.emit_globals(&mut globals)?;
        let mut prototypes = String::new();
        self.emit_prototypes(&mut prototypes)?;
        let mut bodies = String::new();
        for function in self.module.functions.clone() {
            self.emit_function(&mut bodies, &function)?;
        }
        self.emit_init_and_exports(&mut bodies)?;
        self.emit_worker_adapters()?;
        prototypes.push_str(&self.helper_prototypes);
        bodies.push_str(&self.helpers);

        let mut source = String::new();
        source.push_str(PREAMBLE);
        let mut emitted_includes = HashSet::new();
        for include in self
            .module
            .foreign_functions
            .iter()
            .map(|foreign| foreign.include.as_str())
            .filter(|include| emitted_includes.insert(*include))
        {
            if include.contains('"') {
                return Err(internal("foreign include contains a quote"));
            }
            let _ = writeln!(source, "#include \"{include}\"");
        }
        if !self.module.foreign_functions.is_empty() {
            source.push_str(CALLBACK_VIEW);
        }
        for (symbol, (return_type, parameter_types)) in &self.runtime_symbols {
            if !runtime_header_declares(symbol) {
                let parameters = if parameter_types.is_empty() {
                    "void".to_string()
                } else {
                    parameter_types.join(", ")
                };
                let _ = writeln!(source, "extern {return_type} {symbol}({parameters});");
            }
        }
        source.push('\n');
        source.push_str(&types);
        source.push_str(&globals);
        source.push_str(&prototypes);
        source.push_str(&bodies);

        let positions = std::mem::take(&mut self.positions);
        let allocation_metadata_source =
            render_allocation_metadata_definitions(self.module, &positions);
        source.push_str(&allocation_metadata_source);
        Ok(CProgram {
            source,
            positions,
            allocation_metadata_header: render_allocation_metadata_header(),
            allocation_metadata_source,
            foreign_symbols: self.foreign_symbols,
        })
    }

    fn emit_types(&self, out: &mut String) -> Result<(), String> {
        out.push_str("typedef struct { void* code; void* env; } SubFn;\n");
        out.push_str(
            "typedef struct { void* subject; uint64_t position; uint64_t bound; } SubIter;\n",
        );
        for class in &self.module.classes {
            let _ = writeln!(out, "typedef struct SubC{} SubC{};", class.id.0, class.id.0);
        }
        for ty in self.ordered_aggregate_types()? {
            match ty {
                Type::FixedArray(element, count) => {
                    let name = self.fixed_array_name(&element, count)?;
                    let _ = writeln!(
                        out,
                        "typedef struct {{ {} a[{}]; }} {};",
                        self.ctype(&element)?,
                        count,
                        name
                    );
                }
                Type::IterResult(value) => {
                    let name = self.iter_result_name(&value)?;
                    let _ = writeln!(
                        out,
                        "typedef struct {{ int32_t done; {} value; }} {};",
                        self.ctype(&value)?,
                        name
                    );
                }
                Type::Class(id) => {
                    let class = self.class(id)?;
                    let _ = writeln!(out, "struct SubC{} {{", class.id.0);
                    if class.fields.is_empty() {
                        let prefix = class
                            .alignment
                            .map_or_else(String::new, |align| format!("_Alignas({align}) "));
                        let _ = writeln!(out, "    {prefix}unsigned char empty;");
                    }
                    for (index, field) in class.fields.iter().enumerate() {
                        let prefix = if index == 0 {
                            class
                                .alignment
                                .map_or_else(String::new, |align| format!("_Alignas({align}) "))
                        } else {
                            String::new()
                        };
                        let _ = writeln!(
                            out,
                            "    {prefix}{} d{};",
                            self.ctype(&field.ty)?,
                            field.id.0
                        );
                    }
                    out.push_str("};\n");
                }
                _ => {}
            }
        }
        for function in &self.module.functions {
            if matches!(function.kind, l::FunctionKind::Lambda) {
                let captures = capture_parameters(function).collect::<Vec<_>>();
                if !captures.is_empty() {
                    let _ = writeln!(out, "typedef struct SubEnv{} {{", function.id.0);
                    for parameter in captures {
                        let ty = &function.values[parameter.value.0 as usize].ty;
                        let _ =
                            writeln!(out, "    {} c{};", self.value_ctype(ty)?, parameter.value.0);
                    }
                    let _ = writeln!(out, "}} SubEnv{};", function.id.0);
                }
            }
        }
        for function in &self.module.functions {
            if function.is_generator || function.is_async {
                self.emit_frame_type(out, function)?;
            }
        }
        out.push('\n');
        Ok(())
    }

    fn aggregate_types(&self) -> Vec<Type> {
        let mut result = Vec::new();
        let mut add = |ty: &Type| collect_aggregates(ty, &mut result);
        for class in &self.module.classes {
            for field in &class.fields {
                add(&field.ty);
            }
        }
        for global in &self.module.globals {
            add(&global.ty);
        }
        for function in &self.module.functions {
            add(&function.return_type);
            for value in &function.values {
                match &value.ty {
                    l::ValueType::Data(ty) => add(ty),
                    l::ValueType::Address(address) => add(&address.pointee),
                    l::ValueType::Iterator(iterator) => add(&iterator.element),
                }
            }
        }
        result
    }

    fn ordered_aggregate_types(&self) -> Result<Vec<Type>, String> {
        let mut roots = self.aggregate_types();
        roots.extend(
            self.module
                .classes
                .iter()
                .map(|class| Type::Class(class.id)),
        );
        let mut seen = Vec::new();
        let mut ordered = Vec::new();
        for ty in roots {
            self.order_aggregate_type(&ty, &mut seen, &mut ordered)?;
        }
        Ok(ordered)
    }

    fn order_aggregate_type(
        &self,
        ty: &Type,
        seen: &mut Vec<Type>,
        ordered: &mut Vec<Type>,
    ) -> Result<(), String> {
        if seen.contains(ty) {
            return Ok(());
        }
        if !matches!(
            ty,
            Type::Class(_) | Type::FixedArray(_, _) | Type::IterResult(_)
        ) {
            return Ok(());
        }
        seen.push(ty.clone());
        match ty {
            Type::FixedArray(element, _) | Type::IterResult(element) => {
                self.order_stored_type(element, seen, ordered)?;
            }
            Type::Class(id) => {
                for field in &self.class(*id)?.fields {
                    self.order_stored_type(&field.ty, seen, ordered)?;
                }
            }
            _ => {}
        }
        ordered.push(ty.clone());
        Ok(())
    }

    fn order_stored_type(
        &self,
        ty: &Type,
        seen: &mut Vec<Type>,
        ordered: &mut Vec<Type>,
    ) -> Result<(), String> {
        match ty {
            Type::Class(id) if self.is_value_class(*id)? => {
                self.order_aggregate_type(ty, seen, ordered)
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => {
                self.order_aggregate_type(ty, seen, ordered)
            }
            _ => Ok(()),
        }
    }

    fn emit_frame_type(&self, out: &mut String, function: &l::Function) -> Result<(), String> {
        let _ = writeln!(out, "typedef struct SubFrame{} {{", function.id.0);
        out.push_str("    int32_t state;\n    uint32_t reserved;\n    SubAsyncResume resume;\n");
        for parameter in &function.parameters {
            let ty = &function.values[parameter.value.0 as usize].ty;
            let _ = writeln!(out, "    {} p{};", self.value_ctype(ty)?, parameter.value.0);
        }
        for block in &function.blocks {
            let l::Terminator::Suspend {
                successor,
                resume_value,
                kind,
                ..
            } = &block.terminator
            else {
                continue;
            };
            let destination = &function.blocks[successor.0 as usize];
            for parameter in destination
                .parameters
                .iter()
                .skip(usize::from(resume_value.is_some()))
            {
                let ty = &function.values[parameter.0 as usize].ty;
                let _ = writeln!(
                    out,
                    "    {} b{}_v{};",
                    self.value_ctype(ty)?,
                    block.id.0,
                    parameter.0
                );
            }
            if matches!(kind, l::SuspendKind::AsyncCall { .. }) {
                let _ = writeln!(out, "    void* b{}_child;", block.id.0);
            }
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let (Some(result), l::InstructionKind::AddressOfValue) =
                    (instruction.result, &instruction.kind)
                {
                    let l::ValueType::Address(address) = &function.values[result.0 as usize].ty
                    else {
                        continue;
                    };
                    let _ = writeln!(
                        out,
                        "    {} stable_v{};",
                        self.ctype(&address.pointee)?,
                        result.0
                    );
                }
                if let (Some(result), l::InstructionKind::AllocateClass(class)) =
                    (instruction.result, &instruction.kind)
                {
                    if matches!(
                        function.values[result.0 as usize].ty,
                        l::ValueType::Address(_)
                    ) {
                        let _ =
                            writeln!(out, "    {} stable_v{};", self.class_name(*class), result.0);
                    }
                }
                if let (Some(result), l::InstructionKind::MakeClosure(target)) =
                    (instruction.result, &instruction.kind)
                {
                    if capture_parameters(self.function(*target)?).next().is_some() {
                        let _ = writeln!(out, "    SubEnv{} env_v{};", target.0, result.0);
                    }
                }
            }
        }
        let _ = writeln!(out, "}} SubFrame{};", function.id.0);
        Ok(())
    }

    fn emit_globals(&self, out: &mut String) -> Result<(), String> {
        out.push_str("typedef struct SubscriptModuleGlobals {\n");
        if self.module.globals.is_empty() {
            out.push_str("    unsigned char empty;\n");
        }
        for global in &self.module.globals {
            let _ = writeln!(out, "    {} g{};", self.ctype(&global.ty)?, global.id.0);
        }
        out.push_str("} SubscriptModuleGlobals;\n");
        let offset = rtc::Context::globals_offset();
        let _ = writeln!(
            out,
            "static inline SubscriptModuleGlobals* subscript_globals(void* ctx) {{ SubscriptModuleGlobals* value; memcpy(&value, (unsigned char*)ctx + {offset}u, sizeof value); return value; }}"
        );
        for (index, alias) in self.module.string_aliases.iter().enumerate() {
            let _ = writeln!(
                out,
                "static const SubStringAliasMember sub_alias_{index}[] = {{"
            );
            for member in &alias.members {
                let _ = writeln!(
                    out,
                    "    {{ (const unsigned char*){}, {}ull }},",
                    c_string_literal(member.as_bytes()),
                    member.len()
                );
            }
            out.push_str("};\n");
        }
        out.push('\n');
        Ok(())
    }

    fn emit_prototypes(&self, out: &mut String) -> Result<(), String> {
        for function in &self.module.functions {
            let _ = writeln!(out, "{};", self.function_signature(function)?);
            if function.is_generator || function.is_async {
                let _ = writeln!(
                    out,
                    "static uint8_t sub_f{}_resume(void* ctx, void* frame, void* out);",
                    function.id.0
                );
            } else if matches!(function.kind, l::FunctionKind::Free) {
                let _ = writeln!(out, "{};", self.wrapper_signature(function)?);
            }
        }
        out.push_str("void subscript_init(subscript_rt_context* ctx);\n");
        out.push_str("void subscript_kick_async_exports(subscript_rt_context* ctx);\n\n");
        Ok(())
    }

    fn function_signature(&self, function: &l::Function) -> Result<String, String> {
        let return_type = if function.is_generator || function.is_async {
            "void*".to_string()
        } else {
            self.ctype(&function.return_type)?
        };
        let mut parameters = vec!["void* ctx".to_string()];
        if matches!(function.kind, l::FunctionKind::Lambda) {
            parameters.push("void* environment".to_string());
        }
        for parameter in &function.parameters {
            if parameter.kind == l::ParameterKind::Capture {
                continue;
            }
            let ty = &function.values[parameter.value.0 as usize].ty;
            parameters.push(format!("{} a{}", self.value_ctype(ty)?, parameter.value.0));
        }
        Ok(format!(
            "static {return_type} sub_f{}({})",
            function.id.0,
            parameters.join(", ")
        ))
    }

    fn wrapper_signature(&self, function: &l::Function) -> Result<String, String> {
        let mut parameters = vec!["void* ctx".to_string(), "void* environment".to_string()];
        for parameter in explicit_parameters(function) {
            let ty = &function.values[parameter.value.0 as usize].ty;
            parameters.push(format!("{} a{}", self.value_ctype(ty)?, parameter.value.0));
        }
        Ok(format!(
            "static {} sub_w{}({})",
            self.ctype(&function.return_type)?,
            function.id.0,
            parameters.join(", ")
        ))
    }

    fn emit_function(&mut self, out: &mut String, function: &l::Function) -> Result<(), String> {
        if function.is_generator || function.is_async {
            self.emit_coroutine(out, function)
        } else {
            self.emit_ordinary_function(out, function)?;
            if matches!(function.kind, l::FunctionKind::Free) {
                let signature = self.wrapper_signature(function)?;
                let args = explicit_parameters(function)
                    .map(|parameter| format!("a{}", parameter.value.0))
                    .collect::<Vec<_>>();
                let sep = if args.is_empty() { "" } else { ", " };
                let _ = writeln!(out, "{signature} {{ (void)environment;");
                if function.return_type == Type::Void {
                    let _ = writeln!(
                        out,
                        "    sub_f{}(ctx{sep}{});",
                        function.id.0,
                        args.join(", ")
                    );
                    out.push_str("}\n");
                } else {
                    let _ = writeln!(
                        out,
                        "    return sub_f{}(ctx{sep}{});",
                        function.id.0,
                        args.join(", ")
                    );
                    out.push_str("}\n");
                }
            }
            Ok(())
        }
    }

    fn emit_ordinary_function(
        &mut self,
        out: &mut String,
        function: &l::Function,
    ) -> Result<(), String> {
        let signature = self.function_signature(function)?;
        let _ = writeln!(out, "{signature} {{");
        let mut body = Body::new(self, function, false)?;
        body.emit_storage(out)?;
        body.emit_parameter_initializers(out)?;
        let _ = writeln!(out, "    goto b{};", function.entry.0);
        body.emit_graph(out)?;
        body.emit_unwind(out)?;
        verify_trap_consumption(function, &runtime_traps(function), &body.consumed_traps)?;
        out.push_str("}\n\n");
        Ok(())
    }

    fn emit_coroutine(&mut self, out: &mut String, function: &l::Function) -> Result<(), String> {
        let signature = self.function_signature(function)?;
        let frame = format!("SubFrame{}", function.id.0);
        let _ = writeln!(out, "{signature} {{");
        let allocation = function
            .creation_traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::Allocation)
            .ok_or_else(|| internal("coroutine creation has no allocation trap"))?;
        let pos = self.pos_id(&allocation.pos);
        let call = self.runtime_call(
            "void*",
            "subscript_rt_alloc",
            &[
                "void*".into(),
                "uint64_t".into(),
                "uint32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({frame})"),
                format!("{}u", rtc::CLASS_GENERATOR),
                format!("{pos}u"),
            ],
        );
        let _ = writeln!(out, "    {frame}* frame = ({frame}*){call};");
        out.push_str("    if (*(const uint32_t*)ctx != 0u) return NULL;\n");
        let _ = writeln!(
            out,
            "    memset(frame, 0, sizeof *frame);\n    frame->resume = sub_f{}_resume;",
            function.id.0
        );
        for parameter in &function.parameters {
            if parameter.kind != l::ParameterKind::Capture {
                let _ = writeln!(
                    out,
                    "    frame->p{} = a{};",
                    parameter.value.0, parameter.value.0
                );
            }
        }
        out.push_str("    return frame;\n}\n");
        verify_trap_consumption(
            function,
            &function.creation_traps,
            std::slice::from_ref(allocation),
        )?;

        let _ = writeln!(
            out,
            "static uint8_t sub_f{}_resume(void* ctx, void* raw_frame, void* coroutine_out) {{",
            function.id.0
        );
        let _ = writeln!(out, "    {frame}* frame = ({frame}*)raw_frame;");
        let mut body = Body::new(self, function, true)?;
        body.emit_storage(out)?;
        body.emit_coroutine_dispatch(out)?;
        body.emit_graph(out)?;
        body.emit_unwind(out)?;
        verify_trap_consumption(function, &runtime_traps(function), &body.consumed_traps)?;
        out.push_str("}\n\n");
        Ok(())
    }

    fn emit_init_and_exports(&mut self, out: &mut String) -> Result<(), String> {
        let init_call = self.runtime_call(
            "void*",
            "subscript_rt_globals_init",
            &["void*".into(), "uint64_t".into(), "uint64_t".into()],
            &[
                "ctx".into(),
                "(uint64_t)sizeof(SubscriptModuleGlobals)".into(),
                "(uint64_t)_Alignof(SubscriptModuleGlobals)".into(),
            ],
        );
        out.push_str("void subscript_init(subscript_rt_context* ctx) {\n");
        let _ = writeln!(out, "    if ({init_call} == NULL) return;");
        for global in &self.module.globals {
            if self.type_contains_managed(&global.ty, &mut HashSet::new())? {
                let call = self.runtime_call(
                    "void",
                    "subscript_rt_root_add",
                    &["void*".into(), "void*".into(), "uint64_t".into()],
                    &[
                        "ctx".into(),
                        format!("&subscript_globals(ctx)->g{}", global.id.0),
                        format!(
                            "(sizeof(subscript_globals(ctx)->g{}) + 7u) / 8u",
                            global.id.0
                        ),
                    ],
                );
                let _ = writeln!(out, "    {call};");
            }
        }
        if let Some(initializer) = self.module.initializer {
            let _ = writeln!(out, "    sub_f{}(ctx);", initializer.0);
        }
        out.push_str("}\n\n");

        for function in self.module.functions.clone() {
            if function.host_entry_traps.is_none() {
                continue;
            }
            let name = &function.source_name;
            if function.is_async {
                let kick = self.runtime_call(
                    "void",
                    "subscript_rt_async_kick",
                    &["void*".into(), "void*".into(), "SubAsyncResume".into()],
                    &[
                        "ctx".into(),
                        "frame".into(),
                        format!("sub_f{}_resume", function.id.0),
                    ],
                );
                let _ = writeln!(
                    out,
                    "void subscript_export_{name}(subscript_rt_context* ctx) {{"
                );
                let _ = writeln!(out, "    void* frame = sub_f{}(ctx);", function.id.0);
                out.push_str("    if (*(const uint32_t*)ctx != 0u) return;\n");
                let _ = writeln!(out, "    {kick};\n}}");
            } else {
                let parameters = explicit_parameters(&function).collect::<Vec<_>>();
                let declaration = parameters
                    .iter()
                    .map(|parameter| {
                        let ty = &function.values[parameter.value.0 as usize].ty;
                        Ok(format!("{} a{}", self.value_ctype(ty)?, parameter.value.0))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let separator = if declaration.is_empty() { "" } else { ", " };
                let args = parameters
                    .iter()
                    .map(|parameter| format!("a{}", parameter.value.0))
                    .collect::<Vec<_>>();
                let argument_separator = if args.is_empty() { "" } else { ", " };
                let _ = writeln!(
                    out,
                    "void subscript_export_{name}(subscript_rt_context* ctx{separator}{}) {{",
                    declaration.join(", ")
                );
                self.emit_host_entry_validations(out, &function, &parameters)?;
                let _ = writeln!(
                    out,
                    "    sub_f{}(ctx{argument_separator}{});\n}}",
                    function.id.0,
                    args.join(", ")
                );
            }
        }
        out.push_str("void subscript_kick_async_exports(subscript_rt_context* ctx) {\n");
        for root in &self.module.async_roots {
            let function = self.function(*root)?;
            if function.source_name != "main" {
                let _ = writeln!(out, "    subscript_export_{}(ctx);", function.source_name);
                out.push_str("    if (*(const uint32_t*)ctx != 0u) return;\n");
            }
        }
        out.push_str("}\n\n");
        Ok(())
    }

    fn emit_host_entry_validations(
        &mut self,
        out: &mut String,
        function: &l::Function,
        parameters: &[&l::Parameter],
    ) -> Result<(), String> {
        let expected = function
            .host_entry_traps
            .as_ref()
            .ok_or_else(|| internal("host wrapper has no trap attachment"))?;
        let mut matched = vec![false; expected.len()];
        let mut consumed = Vec::new();
        for parameter in parameters {
            let ty = data_type(&function.values[parameter.value.0 as usize].ty)?;
            let Type::StringAlias(alias) = ty else {
                continue;
            };
            let trap_index = expected
                .iter()
                .zip(&matched)
                .position(|(trap, matched)| {
                    !matched
                        && trap.kind == l::TrapKind::WireEnumValue(*alias)
                        && trap.pos == parameter.pos
                })
                .ok_or_else(|| internal("host wire parameter has no LIR trap"))?;
            matched[trap_index] = true;
            let trap = expected[trap_index].clone();
            let definition = self
                .module
                .string_aliases
                .get(alias.0)
                .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
            let source_name = definition.source_name.clone();
            let wires = definition
                .wire_values
                .clone()
                .ok_or_else(|| internal("host wire validation targets a plain alias"))?;
            let value = format!("a{}", parameter.value.0);
            let valid = wires
                .iter()
                .map(|wire| format!("{value} == {wire}"))
                .collect::<Vec<_>>()
                .join(" || ");
            let pos = self.pos_id(&trap.pos);
            let call = self.runtime_call(
                "void",
                "subscript_rt_trap_wire_enum",
                &[
                    "void*".into(),
                    "const unsigned char*".into(),
                    "uint64_t".into(),
                    "int32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    format!(
                        "(const unsigned char*){}",
                        c_string_literal(source_name.as_bytes())
                    ),
                    format!("{}ull", source_name.len()),
                    value,
                    format!("{pos}u"),
                ],
            );
            let condition = if valid.is_empty() { "0" } else { &valid };
            let _ = writeln!(out, "    if (!({condition})) {{ {call}; return; }}");
            consumed.push(trap);
        }
        verify_trap_consumption(function, expected, &consumed)
    }

    fn define_assoc_bridge(&mut self, key: &Type, value: Option<&Type>) -> Result<String, String> {
        let name = format!("subscript_assoc_bridge{}", self.helper_count);
        self.helper_count += 1;
        let key_type = self.ctype(key)?;
        let (signature, call) = if let Some(value) = value {
            let value_type = self.ctype(value)?;
            (
                format!("static void {name}(void* ctx, const void* code, const void* env, const void* value, const void* key)"),
                format!("((void (*)(void*, void*, {value_type}, {key_type}))code)(ctx, (void*)env, *(const {value_type}*)value, *(const {key_type}*)key)"),
            )
        } else {
            (
                format!("static void {name}(void* ctx, const void* code, const void* env, const void* key)"),
                format!("((void (*)(void*, void*, {key_type}))code)(ctx, (void*)env, *(const {key_type}*)key)"),
            )
        };
        let _ = writeln!(self.helper_prototypes, "{signature};");
        let _ = writeln!(self.helpers, "{signature} {{ {call}; }}");
        Ok(name)
    }

    fn define_group_bridge(&mut self, element: &Type, key: &Type) -> Result<String, String> {
        let name = format!("subscript_group_bridge{}", self.helper_count);
        self.helper_count += 1;
        let element_type = self.ctype(element)?;
        let key_type = self.ctype(key)?;
        let signature = format!("static void {name}(void* ctx, const void* code, const void* env, const void* element, void* key_out)");
        let call = format!("*({key_type}*)key_out = (({key_type} (*)(void*, void*, {element_type}))code)(ctx, (void*)env, *(const {element_type}*)element)");
        let _ = writeln!(self.helper_prototypes, "{signature};");
        let _ = writeln!(self.helpers, "{signature} {{ {call}; }}");
        Ok(name)
    }

    fn emit_worker_adapters(&mut self) -> Result<(), String> {
        for (index, entry) in self.module.worker_entries.iter().enumerate() {
            let function = self.function(entry.function)?;
            if function.is_async
                || function.is_generator
                || function.return_type != Type::Void
                || explicit_parameters(function).count() != 2
            {
                return Err(internal(format!(
                    "worker entry `{}` lost its checked shape",
                    function.source_name
                )));
            }
            let signature = format!("static void subscript_worker_entry{index}(subscript_rt_context* ctx, subscript_rt_worker_inbox* inbox, subscript_rt_worker_outbox* outbox)");
            let _ = writeln!(self.helper_prototypes, "{signature};");
            let _ = writeln!(
                self.helpers,
                "{signature} {{ sub_f{}(ctx, inbox, outbox); }}",
                entry.function.0
            );
        }
        Ok(())
    }
}

struct Body<'e, 'm, 'f> {
    emitter: &'e mut Emitter<'m>,
    function: &'f l::Function,
    coroutine: bool,
    rooted_values: HashSet<l::ValueId>,
    rooted_locals: HashSet<l::LocalId>,
    promoted_locals: HashMap<l::LocalId, l::ValueId>,
    address_definitions: HashMap<l::ValueId, &'f l::Instruction>,
    folded_addresses: HashSet<l::ValueId>,
    function_scoped_values: HashSet<l::ValueId>,
    block_value_declarations: Vec<Vec<l::ValueId>>,
    dominator_children: Vec<Vec<l::BlockId>>,
    graph_roots: Vec<l::BlockId>,
    removable_edge_copies: HashSet<(l::BlockId, l::BlockId, usize)>,
    value_storage: Vec<l::ValueId>,
    delayed_declarations: HashSet<l::ValueId>,
    consumed_traps: Vec<l::Trap>,
    temporary: u32,
}

enum EdgeCopySource {
    Value(l::ValueId),
    Constant(l::Constant),
    Temporary(String),
}

struct EdgeCopy {
    destination: l::ValueId,
    source: EdgeCopySource,
}

#[derive(Clone, Copy)]
enum AddressUse {
    Chain(l::ValueId),
    Terminal,
    Escape,
}

fn address_definitions(function: &l::Function) -> HashMap<l::ValueId, &l::Instruction> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| instruction.result.map(|result| (result, instruction)))
        .collect()
}

fn has_local_address_origin(
    value: l::ValueId,
    definitions: &HashMap<l::ValueId, &l::Instruction>,
    memo: &mut HashMap<l::ValueId, bool>,
    visiting: &mut HashSet<l::ValueId>,
) -> bool {
    if let Some(result) = memo.get(&value) {
        return *result;
    }
    if !visiting.insert(value) {
        return false;
    }
    let result = definitions.get(&value).is_some_and(|instruction| {
        if matches!(instruction.kind, l::InstructionKind::AddressOfLocal(_)) {
            return true;
        }
        if !matches!(
            instruction.kind,
            l::InstructionKind::AddressOfField(_) | l::InstructionKind::AddressOfIndex { .. }
        ) {
            return false;
        }
        let Some(l::Operand::Value(base)) = instruction.operands.first() else {
            return false;
        };
        has_local_address_origin(*base, definitions, memo, visiting)
    });
    visiting.remove(&value);
    memo.insert(value, result);
    result
}

fn record_address_escape(uses: &mut HashMap<l::ValueId, Vec<AddressUse>>, operand: &l::Operand) {
    if let l::Operand::Value(value) = operand {
        if let Some(uses) = uses.get_mut(value) {
            uses.push(AddressUse::Escape);
        }
    }
}

fn record_target_address_escapes(
    uses: &mut HashMap<l::ValueId, Vec<AddressUse>>,
    target: &l::BlockTarget,
) {
    for argument in &target.arguments {
        record_address_escape(uses, argument);
    }
}

fn record_terminator_address_escapes(
    uses: &mut HashMap<l::ValueId, Vec<AddressUse>>,
    terminator: &l::Terminator,
) {
    match terminator {
        l::Terminator::Branch(target) => record_target_address_escapes(uses, target),
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            record_address_escape(uses, condition);
            record_target_address_escapes(uses, then_target);
            record_target_address_escapes(uses, else_target);
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
            record_address_escape(uses, value);
            for arm in arms {
                record_target_address_escapes(uses, &arm.target);
            }
            record_target_address_escapes(uses, default);
        }
        l::Terminator::Return { value, .. } => {
            if let Some(value) = value {
                record_address_escape(uses, value);
            }
        }
        l::Terminator::Suspend {
            kind,
            arguments,
            invalidates,
            ..
        } => {
            match kind {
                l::SuspendKind::Yield(value) => {
                    if let Some(value) = value {
                        if let Some(uses) = uses.get_mut(value) {
                            uses.push(AddressUse::Escape);
                        }
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    for value in operands {
                        if let Some(uses) = uses.get_mut(value) {
                            uses.push(AddressUse::Escape);
                        }
                    }
                }
            }
            for argument in arguments {
                record_address_escape(uses, argument);
            }
            for value in invalidates {
                if let Some(uses) = uses.get_mut(value) {
                    uses.push(AddressUse::Escape);
                }
            }
        }
        l::Terminator::Unreachable { .. } | l::Terminator::Trap(_) => {}
    }
}

fn address_has_only_terminal_consumers(
    value: l::ValueId,
    uses: &HashMap<l::ValueId, Vec<AddressUse>>,
    memo: &mut HashMap<l::ValueId, bool>,
    visiting: &mut HashSet<l::ValueId>,
) -> bool {
    if let Some(result) = memo.get(&value) {
        return *result;
    }
    if !visiting.insert(value) {
        return false;
    }
    let result = uses.get(&value).is_some_and(|value_uses| {
        value_uses.iter().all(|use_| match use_ {
            AddressUse::Chain(child) => {
                address_has_only_terminal_consumers(*child, uses, memo, visiting)
            }
            AddressUse::Terminal => true,
            AddressUse::Escape => false,
        })
    });
    visiting.remove(&value);
    memo.insert(value, result);
    result
}

fn foldable_local_addresses(
    function: &l::Function,
    definitions: &HashMap<l::ValueId, &l::Instruction>,
) -> HashSet<l::ValueId> {
    let mut origin_memo = HashMap::new();
    let candidates = definitions
        .keys()
        .copied()
        .filter(|value| {
            has_local_address_origin(*value, definitions, &mut origin_memo, &mut HashSet::new())
        })
        .collect::<HashSet<_>>();
    let mut uses = candidates
        .iter()
        .map(|value| (*value, Vec::new()))
        .collect::<HashMap<_, _>>();

    for block in &function.blocks {
        for instruction in &block.instructions {
            for (index, operand) in instruction.operands.iter().enumerate() {
                let l::Operand::Value(value) = operand else {
                    continue;
                };
                let Some(value_uses) = uses.get_mut(value) else {
                    continue;
                };
                let use_ = match (&instruction.kind, index, instruction.result) {
                    (l::InstructionKind::LoadAddress, 0, _)
                    | (l::InstructionKind::StoreAddress, 0, _) => AddressUse::Terminal,
                    (l::InstructionKind::AddressOfField(_), 0, Some(child))
                    | (l::InstructionKind::AddressOfIndex { .. }, 0, Some(child))
                        if candidates.contains(&child) =>
                    {
                        AddressUse::Chain(child)
                    }
                    _ => AddressUse::Escape,
                };
                value_uses.push(use_);
            }
            for value in &instruction.invalidates {
                if let Some(value_uses) = uses.get_mut(value) {
                    value_uses.push(AddressUse::Escape);
                }
            }
        }
        record_terminator_address_escapes(&mut uses, &block.terminator);
    }

    let mut memo = HashMap::new();
    candidates
        .into_iter()
        .filter(|value| {
            address_has_only_terminal_consumers(*value, &uses, &mut memo, &mut HashSet::new())
        })
        .collect()
}

fn value_only_seeds_local(function: &l::Function, value: l::ValueId, local: l::LocalId) -> bool {
    for block in &function.blocks {
        for instruction in &block.instructions {
            for (index, operand) in instruction.operands.iter().enumerate() {
                if !matches!(operand, l::Operand::Value(candidate) if *candidate == value) {
                    continue;
                }
                if !matches!(instruction.kind, l::InstructionKind::StoreLocal(candidate) if candidate == local)
                    || index != 0
                {
                    return false;
                }
            }
            if instruction.invalidates.contains(&value) {
                return false;
            }
        }
        if terminator_uses_value(&block.terminator, value) {
            return false;
        }
    }
    true
}

fn promoted_local_values(
    function: &l::Function,
    folded_addresses: &HashSet<l::ValueId>,
) -> HashMap<l::LocalId, l::ValueId> {
    let materialized_locals = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            let l::InstructionKind::AddressOfLocal(local) = &instruction.kind else {
                return None;
            };
            (!instruction
                .result
                .is_some_and(|result| folded_addresses.contains(&result)))
            .then_some(*local)
        })
        .collect::<HashSet<_>>();
    let parameter_values = function
        .parameters
        .iter()
        .filter_map(|parameter| parameter.storage.map(|local| (local, parameter.value)))
        .collect::<HashMap<_, _>>();
    let mut store_values = HashMap::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            let l::InstructionKind::StoreLocal(local) = &instruction.kind else {
                continue;
            };
            if let Some(l::Operand::Value(value)) = instruction.operands.first() {
                store_values.entry(*local).or_insert(*value);
            }
        }
    }

    let mut used_values = HashSet::new();
    function
        .locals
        .iter()
        .filter(|local| !materialized_locals.contains(&local.id))
        .filter_map(|local| {
            let value = parameter_values
                .get(&local.id)
                .copied()
                .or_else(|| store_values.get(&local.id).copied())?;
            (used_values.insert(value) && value_only_seeds_local(function, value, local.id))
                .then_some((local.id, value))
        })
        .collect()
}

fn declaration_can_use_instruction_assignment(kind: &l::InstructionKind) -> bool {
    matches!(
        kind,
        l::InstructionKind::Copy
            | l::InstructionKind::StringLiteral(_)
            | l::InstructionKind::Zero
            | l::InstructionKind::LoadLocal(_)
            | l::InstructionKind::AddressOfLocal(_)
            | l::InstructionKind::LoadGlobal(_)
            | l::InstructionKind::AddressOfGlobal(_)
            | l::InstructionKind::FunctionRef(_)
            | l::InstructionKind::Unary(_)
            | l::InstructionKind::AllocateClass(_)
            | l::InstructionKind::AddressOfValue
            | l::InstructionKind::LoadAddress
            | l::InstructionKind::LoadField(_)
            | l::InstructionKind::Length
            | l::InstructionKind::ForeignArrayData
            | l::InstructionKind::ArrayLiteral
            | l::InstructionKind::IteratorCreate(_)
    )
}

struct DeclarationScopes {
    function_values: HashSet<l::ValueId>,
    block_values: Vec<Vec<l::ValueId>>,
    dominator_children: Vec<Vec<l::BlockId>>,
    graph_roots: Vec<l::BlockId>,
}

fn record_value_reference(
    references: &mut [HashSet<l::BlockId>],
    value_storage: &[l::ValueId],
    operand: &l::Operand,
    block: l::BlockId,
) {
    if let l::Operand::Value(value) = operand {
        references[value_storage[value.0 as usize].0 as usize].insert(block);
    }
}

fn record_target_value_references(
    function: &l::Function,
    references: &mut [HashSet<l::BlockId>],
    value_storage: &[l::ValueId],
    target: &l::BlockTarget,
    source: l::BlockId,
) {
    for argument in &target.arguments {
        record_value_reference(references, value_storage, argument, source);
    }
    for parameter in &function.blocks[target.block.0 as usize].parameters {
        references[value_storage[parameter.0 as usize].0 as usize].insert(source);
    }
}

fn record_terminator_value_references(
    function: &l::Function,
    references: &mut [HashSet<l::BlockId>],
    value_storage: &[l::ValueId],
    forced_function: &mut HashSet<l::ValueId>,
    block: l::BlockId,
    terminator: &l::Terminator,
) {
    match terminator {
        l::Terminator::Branch(target) => {
            record_target_value_references(function, references, value_storage, target, block);
        }
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            record_value_reference(references, value_storage, condition, block);
            record_target_value_references(function, references, value_storage, then_target, block);
            record_target_value_references(function, references, value_storage, else_target, block);
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
            record_value_reference(references, value_storage, value, block);
            for arm in arms {
                record_target_value_references(
                    function,
                    references,
                    value_storage,
                    &arm.target,
                    block,
                );
            }
            record_target_value_references(function, references, value_storage, default, block);
        }
        l::Terminator::Return { value, .. } => {
            if let Some(value) = value {
                record_value_reference(references, value_storage, value, block);
            }
        }
        l::Terminator::Suspend {
            kind,
            successor,
            resume_value,
            arguments,
            invalidates,
            ..
        } => {
            match kind {
                l::SuspendKind::Yield(value) => {
                    if let Some(value) = value {
                        references[value_storage[value.0 as usize].0 as usize].insert(block);
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    for value in operands {
                        references[value_storage[value.0 as usize].0 as usize].insert(block);
                    }
                }
            }
            for argument in arguments {
                record_value_reference(references, value_storage, argument, block);
            }
            for value in invalidates {
                references[value_storage[value.0 as usize].0 as usize].insert(block);
            }
            for parameter in &function.blocks[successor.0 as usize].parameters {
                forced_function.insert(value_storage[parameter.0 as usize]);
            }
            if let Some(value) = resume_value {
                forced_function.insert(value_storage[value.0 as usize]);
            }
        }
        l::Terminator::Unreachable { .. } | l::Terminator::Trap(_) => {}
    }
}

fn terminator_successors(terminator: &l::Terminator) -> Vec<l::BlockId> {
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

fn instruction_uses_value(instruction: &l::Instruction, value: l::ValueId) -> bool {
    instruction
        .operands
        .iter()
        .any(|operand| matches!(operand, l::Operand::Value(candidate) if *candidate == value))
        || instruction.invalidates.contains(&value)
}

fn target_uses_value(target: &l::BlockTarget, value: l::ValueId) -> bool {
    target
        .arguments
        .iter()
        .any(|operand| matches!(operand, l::Operand::Value(candidate) if *candidate == value))
}

fn terminator_uses_value(terminator: &l::Terminator, value: l::ValueId) -> bool {
    match terminator {
        l::Terminator::Branch(target) => target_uses_value(target, value),
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            matches!(condition, l::Operand::Value(candidate) if *candidate == value)
                || target_uses_value(then_target, value)
                || target_uses_value(else_target, value)
        }
        l::Terminator::Switch {
            value: discriminant,
            arms,
            default,
        } => {
            matches!(discriminant, l::Operand::Value(candidate) if *candidate == value)
                || arms.iter().any(|arm| target_uses_value(&arm.target, value))
                || target_uses_value(default, value)
        }
        l::Terminator::Return {
            value: returned, ..
        } => matches!(returned, Some(l::Operand::Value(candidate)) if *candidate == value),
        l::Terminator::Suspend {
            kind,
            arguments,
            invalidates,
            ..
        } => {
            let kind_uses = match kind {
                l::SuspendKind::Yield(yielded) => *yielded == Some(value),
                l::SuspendKind::Async => false,
                l::SuspendKind::AsyncCall { operands, .. } => operands.contains(&value),
            };
            kind_uses
                || arguments.iter().any(
                    |operand| matches!(operand, l::Operand::Value(candidate) if *candidate == value),
                )
                || invalidates.contains(&value)
        }
        l::Terminator::Unreachable { .. } | l::Terminator::Trap(_) => false,
    }
}

fn block_uses_value(block: &l::BasicBlock, value: l::ValueId) -> bool {
    block
        .instructions
        .iter()
        .any(|instruction| instruction_uses_value(instruction, value))
        || terminator_uses_value(&block.terminator, value)
}

fn value_used_from(function: &l::Function, value: l::ValueId, start: l::BlockId) -> bool {
    let mut seen = HashSet::new();
    let mut pending = vec![start];
    while let Some(block) = pending.pop() {
        if !seen.insert(block) {
            continue;
        }
        let definition = &function.blocks[block.0 as usize];
        if definition.parameters.contains(&value) {
            continue;
        }
        if block_uses_value(definition, value) {
            return true;
        }
        pending.extend(terminator_successors(&definition.terminator));
    }
    false
}

fn ordinary_targets(terminator: &l::Terminator) -> Vec<&l::BlockTarget> {
    match terminator {
        l::Terminator::Branch(target) => vec![target],
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => vec![then_target, else_target],
        l::Terminator::Switch { arms, default, .. } => arms
            .iter()
            .map(|arm| &arm.target)
            .chain(std::iter::once(default))
            .collect(),
        l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_)
        | l::Terminator::Suspend { .. } => Vec::new(),
    }
}

fn removable_block_parameter_copies(
    function: &l::Function,
) -> (
    HashSet<(l::BlockId, l::BlockId, usize)>,
    HashSet<l::ValueId>,
) {
    let used_values = function
        .values
        .iter()
        .filter(|value| {
            function
                .blocks
                .iter()
                .any(|block| block_uses_value(block, value.id))
        })
        .map(|value| value.id)
        .collect::<HashSet<_>>();
    let mut removable = HashSet::new();
    let mut incoming = HashMap::<l::ValueId, usize>::new();
    let mut removable_incoming = HashMap::<l::ValueId, usize>::new();
    for block in &function.blocks {
        for target in ordinary_targets(&block.terminator) {
            let destination = &function.blocks[target.block.0 as usize];
            for (index, (argument, parameter)) in target
                .arguments
                .iter()
                .zip(&destination.parameters)
                .enumerate()
            {
                *incoming.entry(*parameter).or_default() += 1;
                if used_values.contains(parameter) {
                    continue;
                }
                let source_dead = match argument {
                    l::Operand::Constant(_) => true,
                    l::Operand::Value(source) => {
                        !value_used_from(function, *source, target.block)
                            && !target.arguments[index + 1..].iter().any(|argument| {
                                matches!(argument, l::Operand::Value(candidate) if candidate == source)
                            })
                    }
                };
                if source_dead {
                    removable.insert((block.id, target.block, index));
                    *removable_incoming.entry(*parameter).or_default() += 1;
                }
            }
        }
    }
    let elided_values = incoming
        .into_iter()
        .filter_map(|(value, count)| {
            (removable_incoming.get(&value).copied() == Some(count)).then_some(value)
        })
        .collect();
    (removable, elided_values)
}

fn record_operand_liveness_use(values: &mut BTreeSet<l::ValueId>, operand: &l::Operand) {
    if let l::Operand::Value(value) = operand {
        values.insert(*value);
    }
}

fn record_target_liveness_uses(values: &mut BTreeSet<l::ValueId>, target: &l::BlockTarget) {
    for argument in &target.arguments {
        record_operand_liveness_use(values, argument);
    }
}

fn record_terminator_liveness_uses(values: &mut BTreeSet<l::ValueId>, terminator: &l::Terminator) {
    match terminator {
        l::Terminator::Branch(target) => record_target_liveness_uses(values, target),
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            record_operand_liveness_use(values, condition);
            record_target_liveness_uses(values, then_target);
            record_target_liveness_uses(values, else_target);
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
            record_operand_liveness_use(values, value);
            for arm in arms {
                record_target_liveness_uses(values, &arm.target);
            }
            record_target_liveness_uses(values, default);
        }
        l::Terminator::Return { value, .. } => {
            if let Some(value) = value {
                record_operand_liveness_use(values, value);
            }
        }
        l::Terminator::Suspend {
            kind,
            arguments,
            invalidates,
            ..
        } => {
            match kind {
                l::SuspendKind::Yield(value) => values.extend(value),
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    values.extend(operands.iter().copied());
                }
            }
            for argument in arguments {
                record_operand_liveness_use(values, argument);
            }
            values.extend(invalidates.iter().copied());
        }
        l::Terminator::Unreachable { .. } | l::Terminator::Trap(_) => {}
    }
}

fn add_interference(interference: &mut [HashSet<l::ValueId>], left: l::ValueId, right: l::ValueId) {
    if left == right {
        return;
    }
    interference[left.0 as usize].insert(right);
    interference[right.0 as usize].insert(left);
}

fn value_interference(function: &l::Function) -> Vec<HashSet<l::ValueId>> {
    // Reuse the lowering's fixed-point liveness. The transcriber only derives
    // interference at each definition and does not run a second graph walk.
    let live_in = lir_live_ins(function, function.values.len());
    let live_out = function
        .blocks
        .iter()
        .map(|block| {
            terminator_successors(&block.terminator)
                .into_iter()
                .flat_map(|successor| live_in[successor.0 as usize].iter().copied())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut interference = vec![HashSet::new(); function.values.len()];
    for block in &function.blocks {
        let index = block.id.0 as usize;
        let mut live = live_out[index].clone();
        record_terminator_liveness_uses(&mut live, &block.terminator);
        for instruction in block.instructions.iter().rev() {
            if let Some(result) = instruction.result {
                for other in live.iter().copied() {
                    add_interference(&mut interference, result, other);
                }
                live.remove(&result);
            }
            for operand in &instruction.operands {
                record_operand_liveness_use(&mut live, operand);
            }
            live.extend(instruction.invalidates.iter().copied());
        }
        for parameter in &block.parameters {
            for other in live.iter().copied() {
                add_interference(&mut interference, *parameter, other);
            }
            for other in &block.parameters {
                add_interference(&mut interference, *parameter, *other);
            }
        }
    }

    let entry_live = &live_in[function.entry.0 as usize];
    for parameter in &function.parameters {
        for other in entry_live.iter().copied() {
            add_interference(&mut interference, parameter.value, other);
        }
        for other in &function.parameters {
            add_interference(&mut interference, parameter.value, other.value);
        }
    }
    interference
}

struct Coalescing {
    parents: Vec<usize>,
    members: Vec<Vec<l::ValueId>>,
}

impl Coalescing {
    fn new(value_count: usize) -> Self {
        Self {
            parents: (0..value_count).collect(),
            members: (0..value_count)
                .map(|index| vec![l::ValueId(index as u32)])
                .collect(),
        }
    }

    fn root(&self, value: l::ValueId) -> usize {
        let mut root = value.0 as usize;
        while self.parents[root] != root {
            root = self.parents[root];
        }
        root
    }

    fn try_merge(
        &mut self,
        left: l::ValueId,
        right: l::ValueId,
        interference: &[HashSet<l::ValueId>],
    ) {
        let left = self.root(left);
        let right = self.root(right);
        if left == right
            || self.members[left].iter().any(|left| {
                self.members[right]
                    .iter()
                    .any(|right| interference[left.0 as usize].contains(right))
            })
        {
            return;
        }
        let (representative, merged) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        self.parents[merged] = representative;
        let merged_members = std::mem::take(&mut self.members[merged]);
        self.members[representative].extend(merged_members);
    }

    fn representatives(&self) -> Vec<l::ValueId> {
        (0..self.parents.len())
            .map(|index| l::ValueId(self.root(l::ValueId(index as u32)) as u32))
            .collect()
    }
}

fn coalesced_value_storage(
    function: &l::Function,
    rooted_values: &HashSet<l::ValueId>,
    folded_addresses: &HashSet<l::ValueId>,
    removable_edge_copies: &HashSet<(l::BlockId, l::BlockId, usize)>,
    elided_values: &HashSet<l::ValueId>,
    promoted_local_values: &HashSet<l::ValueId>,
) -> Vec<l::ValueId> {
    let interference = value_interference(function);
    let mut coalescing = Coalescing::new(function.values.len());
    // Prefer every block-parameter copy. A merge is valid only when no value
    // in either storage group interferes with a value in the other group.
    for block in &function.blocks {
        for target in ordinary_targets(&block.terminator) {
            let destination = &function.blocks[target.block.0 as usize];
            for (index, (argument, parameter)) in target
                .arguments
                .iter()
                .zip(&destination.parameters)
                .enumerate()
            {
                if removable_edge_copies.contains(&(block.id, target.block, index))
                    || elided_values.contains(parameter)
                    || folded_addresses.contains(parameter)
                    || promoted_local_values.contains(parameter)
                {
                    continue;
                }
                let l::Operand::Value(argument) = argument else {
                    continue;
                };
                if elided_values.contains(argument)
                    || folded_addresses.contains(argument)
                    || promoted_local_values.contains(argument)
                    || function.values[argument.0 as usize].ty
                        != function.values[parameter.0 as usize].ty
                    || rooted_values.contains(argument) != rooted_values.contains(parameter)
                {
                    continue;
                }
                coalescing.try_merge(*argument, *parameter, &interference);
            }
        }
    }
    coalescing.representatives()
}

fn declaration_scopes(
    function: &l::Function,
    coroutine: bool,
    rooted_values: &HashSet<l::ValueId>,
    folded_addresses: &HashSet<l::ValueId>,
    elided_values: &HashSet<l::ValueId>,
    value_storage: &[l::ValueId],
    promoted_locals: &HashMap<l::LocalId, l::ValueId>,
) -> DeclarationScopes {
    let block_count = function.blocks.len();
    let mut block_values = vec![Vec::new(); block_count];
    let mut dominator_children = vec![Vec::new(); block_count];
    if coroutine {
        return DeclarationScopes {
            function_values: function
                .values
                .iter()
                .map(|value| value.id)
                .filter(|value| {
                    value_storage[value.0 as usize] == *value
                        && !rooted_values.contains(value)
                        && !folded_addresses.contains(value)
                        && !elided_values.contains(value)
                })
                .collect(),
            block_values,
            dominator_children,
            graph_roots: function.blocks.iter().map(|block| block.id).collect(),
        };
    }

    let mut references = vec![HashSet::new(); function.values.len()];
    let mut forced_function = function
        .parameters
        .iter()
        .map(|parameter| value_storage[parameter.value.0 as usize])
        .collect::<HashSet<_>>();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Some(result) = instruction.result {
                references[value_storage[result.0 as usize].0 as usize].insert(block.id);
            }
            for operand in &instruction.operands {
                record_value_reference(&mut references, value_storage, operand, block.id);
            }
            let promoted_local = match &instruction.kind {
                l::InstructionKind::LoadLocal(local)
                | l::InstructionKind::StoreLocal(local)
                | l::InstructionKind::AddressOfLocal(local) => promoted_locals.get(local),
                _ => None,
            };
            if let Some(value) = promoted_local {
                references[value_storage[value.0 as usize].0 as usize].insert(block.id);
            }
            if matches!(
                instruction.kind,
                l::InstructionKind::Cast | l::InstructionKind::Coerce
            ) {
                // A value-class-to-nullable conversion emits its storage address.
                // Function scope preserves that storage through later aggregate stores.
                if let (Some(result), Some(l::Operand::Value(source))) =
                    (instruction.result, instruction.operands.first())
                {
                    let source_type = &function.values[source.0 as usize].ty;
                    let result_type = &function.values[result.0 as usize].ty;
                    if matches!(
                        (source_type, result_type),
                        (
                            l::ValueType::Data(Type::Class(source)),
                            l::ValueType::Data(Type::Nullable(target))
                        ) if matches!(target.as_ref(), Type::Class(target) if target == source)
                    ) {
                        forced_function.insert(value_storage[source.0 as usize]);
                    }
                }
            }
            for value in &instruction.invalidates {
                references[value_storage[value.0 as usize].0 as usize].insert(block.id);
            }
        }
        record_terminator_value_references(
            function,
            &mut references,
            value_storage,
            &mut forced_function,
            block.id,
            &block.terminator,
        );
    }

    let mut predecessors = vec![Vec::new(); block_count];
    for block in &function.blocks {
        for successor in terminator_successors(&block.terminator) {
            predecessors[successor.0 as usize].push(block.id);
        }
    }
    let entry = function.entry.0 as usize;
    let mut reachable = vec![false; block_count];
    let mut pending = vec![function.entry];
    while let Some(block) = pending.pop() {
        let index = block.0 as usize;
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        pending.extend(terminator_successors(&function.blocks[index].terminator));
    }
    let reachable_blocks = reachable
        .iter()
        .enumerate()
        .filter_map(|(index, reachable)| reachable.then_some(l::BlockId(index as u32)))
        .collect::<HashSet<_>>();
    let mut dominators = vec![HashSet::new(); block_count];
    for (index, is_reachable) in reachable.iter().copied().enumerate() {
        if !is_reachable {
            dominators[index].insert(l::BlockId(index as u32));
        } else if index == entry {
            dominators[index].insert(function.entry);
        } else {
            dominators[index] = reachable_blocks.clone();
        }
    }
    loop {
        let mut changed = false;
        for index in 0..block_count {
            if index == entry || !reachable[index] {
                continue;
            }
            let mut incoming = predecessors[index]
                .iter()
                .copied()
                .filter(|predecessor| reachable[predecessor.0 as usize]);
            let mut next = incoming
                .next()
                .map(|predecessor| dominators[predecessor.0 as usize].clone())
                .unwrap_or_default();
            for predecessor in incoming {
                next.retain(|dominator| dominators[predecessor.0 as usize].contains(dominator));
            }
            next.insert(l::BlockId(index as u32));
            if dominators[index] != next {
                dominators[index] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for index in 0..block_count {
        if index == entry || !reachable[index] {
            continue;
        }
        let block = l::BlockId(index as u32);
        let immediate = dominators[index]
            .iter()
            .copied()
            .filter(|dominator| *dominator != block)
            .max_by_key(|dominator| dominators[dominator.0 as usize].len());
        if let Some(immediate) = immediate {
            dominator_children[immediate.0 as usize].push(block);
        }
    }
    for children in &mut dominator_children {
        children.sort_by_key(|block| block.0);
    }

    let mut function_values = HashSet::new();
    for value in &function.values {
        if value_storage[value.id.0 as usize] != value.id
            || rooted_values.contains(&value.id)
            || folded_addresses.contains(&value.id)
            || elided_values.contains(&value.id)
        {
            continue;
        }
        if forced_function.contains(&value.id) || references[value.id.0 as usize].is_empty() {
            function_values.insert(value.id);
            continue;
        }
        let mut blocks = references[value.id.0 as usize].iter().copied();
        let Some(first) = blocks.next() else {
            function_values.insert(value.id);
            continue;
        };
        if !reachable[first.0 as usize] {
            function_values.insert(value.id);
            continue;
        }
        let mut common = dominators[first.0 as usize].clone();
        let mut all_reachable = true;
        for block in blocks {
            if !reachable[block.0 as usize] {
                all_reachable = false;
                break;
            }
            common.retain(|dominator| dominators[block.0 as usize].contains(dominator));
        }
        let scope = all_reachable
            .then(|| {
                common
                    .into_iter()
                    .max_by_key(|dominator| dominators[dominator.0 as usize].len())
            })
            .flatten();
        if let Some(scope) = scope {
            block_values[scope.0 as usize].push(value.id);
        } else {
            function_values.insert(value.id);
        }
    }
    let mut graph_roots = vec![function.entry];
    graph_roots.extend(
        function
            .blocks
            .iter()
            .filter(|block| !reachable[block.id.0 as usize])
            .map(|block| block.id),
    );
    DeclarationScopes {
        function_values,
        block_values,
        dominator_children,
        graph_roots,
    }
}

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    fn new(
        emitter: &'e mut Emitter<'m>,
        function: &'f l::Function,
        coroutine: bool,
    ) -> Result<Self, String> {
        let rooted_values = function
            .values
            .iter()
            .map(|value| {
                emitter
                    .value_contains_managed(&value.ty)
                    .map(|contains| contains.then_some(value.id))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();
        let address_definitions = address_definitions(function);
        let folded_addresses = foldable_local_addresses(function, &address_definitions);
        let promoted_locals = promoted_local_values(function, &folded_addresses);
        let promoted_local_values = promoted_locals.values().copied().collect::<HashSet<_>>();
        let rooted_locals = function
            .locals
            .iter()
            .filter(|local| !promoted_locals.contains_key(&local.id))
            .map(|local| {
                emitter
                    .value_contains_managed(&local.ty)
                    .map(|contains| contains.then_some(local.id))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();
        let (removable_edge_copies, elided_values) = removable_block_parameter_copies(function);
        let value_storage = coalesced_value_storage(
            function,
            &rooted_values,
            &folded_addresses,
            &removable_edge_copies,
            &elided_values,
            &promoted_local_values,
        );
        let declaration_scopes = declaration_scopes(
            function,
            coroutine,
            &rooted_values,
            &folded_addresses,
            &elided_values,
            &value_storage,
            &promoted_locals,
        );
        let mut delayed_declarations = HashSet::new();
        for (block, values) in declaration_scopes.block_values.iter().enumerate() {
            for value in values {
                if value_storage[value.0 as usize] == *value
                    && function.blocks[block]
                        .instructions
                        .iter()
                        .any(|instruction| {
                            instruction.result == Some(*value)
                                && declaration_can_use_instruction_assignment(&instruction.kind)
                        })
                {
                    delayed_declarations.insert(*value);
                }
            }
        }
        Ok(Self {
            emitter,
            function,
            coroutine,
            rooted_values,
            rooted_locals,
            promoted_locals,
            address_definitions,
            folded_addresses,
            function_scoped_values: declaration_scopes.function_values,
            block_value_declarations: declaration_scopes.block_values,
            dominator_children: declaration_scopes.dominator_children,
            graph_roots: declaration_scopes.graph_roots,
            removable_edge_copies,
            value_storage,
            delayed_declarations,
            consumed_traps: Vec::new(),
            temporary: 0,
        })
    }

    fn fresh(&mut self) -> String {
        let value = format!("t{}", self.temporary);
        self.temporary += 1;
        value
    }

    fn value(&self, id: l::ValueId) -> String {
        let id = self.value_storage[id.0 as usize];
        if self.rooted_values.contains(&id) {
            format!("roots.v{}", id.0)
        } else {
            format!("v{}", id.0)
        }
    }

    fn local(&self, id: l::LocalId) -> String {
        if let Some(value) = self.promoted_locals.get(&id) {
            return self.value(*value);
        }
        if self.rooted_locals.contains(&id) {
            format!("roots.l{}", id.0)
        } else {
            format!("l{}", id.0)
        }
    }

    fn value_type(&self, id: l::ValueId) -> Result<&l::ValueType, String> {
        self.function
            .values
            .get(id.0 as usize)
            .filter(|value| value.id == id)
            .map(|value| &value.ty)
            .ok_or_else(|| internal(format!("value {} is missing", id.0)))
    }

    fn folded_address_expression(&mut self, value: l::ValueId) -> Result<String, String> {
        if !self.folded_addresses.contains(&value) {
            return Err(internal(format!(
                "address value {} is not foldable",
                value.0
            )));
        }
        let instruction = self
            .address_definitions
            .get(&value)
            .copied()
            .ok_or_else(|| internal(format!("address value {} has no definition", value.0)))?
            .clone();
        match instruction.kind {
            l::InstructionKind::AddressOfLocal(local) => Ok(self.local(local)),
            l::InstructionKind::AddressOfField(field) => {
                let Some(l::Operand::Value(base)) = instruction.operands.first() else {
                    return Err(internal("folded field address has no value base"));
                };
                let base = *base;
                let base_expression = if self.folded_addresses.contains(&base) {
                    self.folded_address_expression(base)?
                } else {
                    format!("*({})", self.value(base))
                };
                match field {
                    l::FieldRef::Class(field) => {
                        let (class, _, _) = self.emitter.field(field)?;
                        let l::ValueType::Address(address) = self.value_type(base)? else {
                            return Err(internal("folded field base is not an address"));
                        };
                        match &address.pointee {
                            Type::Class(id) if self.emitter.is_value_class(*id)? => {
                                Ok(format!("({base_expression}).d{}", field.0))
                            }
                            Type::Class(_) => Ok(format!(
                                "((({}*)({base_expression}))->d{})",
                                self.emitter.class_name(class),
                                field.0
                            )),
                            other => Err(internal(format!(
                                "folded class field base has type {other:?}"
                            ))),
                        }
                    }
                    l::FieldRef::IterDone => Ok(format!("({base_expression}).done")),
                    l::FieldRef::IterValue => Ok(format!("({base_expression}).value")),
                }
            }
            l::InstructionKind::AddressOfIndex { .. } => {
                let Some(l::Operand::Value(base)) = instruction.operands.first() else {
                    return Err(internal("folded index address has no value base"));
                };
                let base = *base;
                let base_expression = if self.folded_addresses.contains(&base) {
                    self.folded_address_expression(base)?
                } else {
                    format!("*({})", self.value(base))
                };
                let index = instruction
                    .operands
                    .get(1)
                    .ok_or_else(|| internal("folded index address has no index"))?;
                let index = self.operand(index)?;
                let l::ValueType::Address(address) = self.value_type(base)? else {
                    return Err(internal("folded index base is not an address"));
                };
                match &address.pointee {
                    Type::FixedArray(_, _) => Ok(format!("({base_expression}).a[{index}]")),
                    other => Err(internal(format!(
                        "folded indexed address points to {other:?}"
                    ))),
                }
            }
            ref other => Err(internal(format!(
                "folded address value {} has definition {other:?}",
                value.0
            ))),
        }
    }

    fn emit_storage(&mut self, out: &mut String) -> Result<(), String> {
        if !self.rooted_values.is_empty() || !self.rooted_locals.is_empty() {
            out.push_str("    struct {\n");
            for value in &self.function.values {
                if self.value_storage[value.id.0 as usize] == value.id
                    && self.rooted_values.contains(&value.id)
                {
                    let _ = writeln!(
                        out,
                        "        {} v{};",
                        self.emitter.value_ctype(&value.ty)?,
                        value.id.0
                    );
                }
            }
            for local in &self.function.locals {
                if self.rooted_locals.contains(&local.id) {
                    let _ = writeln!(
                        out,
                        "        {} l{};",
                        self.emitter.value_ctype(&local.ty)?,
                        local.id.0
                    );
                }
            }
            out.push_str("    } roots = {0};\n");
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_shadow_push",
                &["void*".into(), "void*".into(), "uint64_t".into()],
                &[
                    "ctx".into(),
                    "&roots".into(),
                    "(sizeof roots + 7u) / 8u".into(),
                ],
            );
            let _ = writeln!(out, "    {call};");
        }
        for value in &self.function.values {
            if self.function_scoped_values.contains(&value.id) {
                let ctype = self.emitter.value_ctype(&value.ty)?;
                if let Some(initializer) = self.parameter_declaration_initializer(value.id) {
                    let _ = writeln!(out, "    {ctype} v{} = {initializer};", value.id.0);
                } else {
                    let zero = self.emitter.zero(&value.ty)?;
                    let _ = writeln!(out, "    {ctype} v{} = {zero};", value.id.0);
                }
            }
        }
        for local in &self.function.locals {
            if !self.rooted_locals.contains(&local.id)
                && !self.promoted_locals.contains_key(&local.id)
            {
                let _ = writeln!(
                    out,
                    "    {} l{} = {};",
                    self.emitter.value_ctype(&local.ty)?,
                    local.id.0,
                    self.emitter.zero(&local.ty)?
                );
            }
        }
        Ok(())
    }

    fn emit_parameter_initializers(&mut self, out: &mut String) -> Result<(), String> {
        for parameter in &self.function.parameters {
            let destination = self.value(parameter.value);
            if self
                .parameter_declaration_initializer(self.value_storage[parameter.value.0 as usize])
                .is_none()
            {
                match parameter.kind {
                    l::ParameterKind::Capture => {
                        let _ = writeln!(
                            out,
                            "    {destination} = ((SubEnv{}*)environment)->c{};",
                            self.function.id.0, parameter.value.0
                        );
                    }
                    l::ParameterKind::Explicit | l::ParameterKind::Receiver => {
                        let _ = writeln!(out, "    {destination} = a{};", parameter.value.0);
                    }
                }
            }
            if let Some(storage) = parameter.storage {
                let storage = self.local(storage);
                if storage != destination
                    && !self.entry_initializes_parameter_storage(parameter, storage.as_str())
                {
                    let _ = writeln!(out, "    {storage} = {destination};");
                }
            }
        }
        Ok(())
    }

    fn entry_initializes_parameter_storage(&self, parameter: &l::Parameter, storage: &str) -> bool {
        self.function.blocks[self.function.entry.0 as usize]
            .instructions
            .iter()
            .any(|instruction| {
                matches!(instruction.kind, l::InstructionKind::StoreLocal(local)
                    if self.local(local) == storage)
                    && matches!(instruction.operands.as_slice(),
                        [l::Operand::Value(value)] if *value == parameter.value)
            })
    }

    fn parameter_declaration_initializer(&self, value: l::ValueId) -> Option<String> {
        if self.coroutine || !self.function_scoped_values.contains(&value) {
            return None;
        }
        self.function.parameters.iter().find_map(|parameter| {
            (self.value_storage[parameter.value.0 as usize] == value).then(|| {
                match parameter.kind {
                    l::ParameterKind::Capture => format!(
                        "((SubEnv{}*)environment)->c{}",
                        self.function.id.0, parameter.value.0
                    ),
                    l::ParameterKind::Explicit | l::ParameterKind::Receiver => {
                        format!("a{}", parameter.value.0)
                    }
                }
            })
        })
    }

    fn emit_coroutine_dispatch(&mut self, out: &mut String) -> Result<(), String> {
        for parameter in &self.function.parameters {
            let _ = writeln!(
                out,
                "    {} = frame->p{};",
                self.value(parameter.value),
                parameter.value.0
            );
            if let Some(storage) = parameter.storage {
                let storage = self.local(storage);
                let value = self.value(parameter.value);
                if storage != value {
                    let _ = writeln!(out, "    {storage} = {value};");
                }
            }
        }
        let _ = writeln!(
            out,
            "    if (frame->state == 0) goto b{};",
            self.function.entry.0
        );
        let mut state = 1u32;
        for block in &self.function.blocks {
            if matches!(block.terminator, l::Terminator::Suspend { .. }) {
                let _ = writeln!(
                    out,
                    "    if (frame->state == {state}) goto resume_b{};",
                    block.id.0
                );
                state += 1;
            }
        }
        out.push_str("    goto coroutine_done;\n");
        state = 1;
        for block in &self.function.blocks {
            let l::Terminator::Suspend {
                successor,
                resume_value,
                kind,
                ..
            } = &block.terminator
            else {
                continue;
            };
            let _ = writeln!(out, "resume_b{}:", block.id.0);
            if matches!(kind, l::SuspendKind::AsyncCall { .. }) {
                self.emit_async_child_resume(out, block, state)?;
            } else {
                if resume_value.is_some() {
                    return Err(internal("non-call suspension defines a resume value"));
                }
                self.restore_suspend_arguments(out, block)?;
                let _ = writeln!(out, "    goto b{};", successor.0);
            }
            state += 1;
        }
        Ok(())
    }

    fn emit_graph(&mut self, out: &mut String) -> Result<(), String> {
        for root in self.graph_roots.clone() {
            self.emit_dominator_subtree(out, root)?;
        }
        Ok(())
    }

    fn emit_dominator_subtree(
        &mut self,
        out: &mut String,
        block_id: l::BlockId,
    ) -> Result<(), String> {
        let block = self
            .function
            .blocks
            .get(block_id.0 as usize)
            .filter(|block| block.id == block_id)
            .ok_or_else(|| internal(format!("block {} is missing", block_id.0)))?
            .clone();
        let _ = writeln!(out, "b{}:\n    {{", block.id.0);
        for value in self.block_value_declarations[block.id.0 as usize].clone() {
            if self.delayed_declarations.contains(&value) {
                continue;
            }
            let _ = writeln!(
                out,
                "    {} v{} = {};",
                self.emitter.value_ctype(self.value_type(value)?)?,
                value.0,
                self.emitter.zero(self.value_type(value)?)?
            );
        }
        for instruction in &block.instructions {
            self.emit_instruction(out, instruction).map_err(|error| {
                internal(format!(
                    "function {} block {} instruction {:?}: {error}",
                    self.function.id.0, block.id.0, instruction.kind
                ))
            })?;
        }
        self.emit_terminator(out, &block)?;
        for child in self.dominator_children[block.id.0 as usize].clone() {
            self.emit_dominator_subtree(out, child)?;
        }
        out.push_str("    }\n");
        Ok(())
    }

    fn emit_instruction(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
    ) -> Result<(), String> {
        let operands = instruction
            .operands
            .iter()
            .map(|operand| self.operand(operand))
            .collect::<Result<Vec<_>, _>>()?;
        let operand_types = instruction
            .operands
            .iter()
            .map(|operand| self.operand_type(operand))
            .collect::<Result<Vec<_>, _>>()?;
        let result = instruction.result.map(|id| self.value(id));
        match &instruction.kind {
            l::InstructionKind::Copy => self.assign(out, result, &operands[0]),
            l::InstructionKind::StringLiteral(text) => {
                let trap = self.take_pending_trap(&instruction.traps, l::TrapKind::Allocation)?;
                let pos = self.emitter.pos_id(&trap.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_str_lit",
                    &[
                        "void*".into(),
                        "const unsigned char*".into(),
                        "uint64_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!(
                            "(const unsigned char*){}",
                            c_string_literal(text.as_bytes())
                        ),
                        format!("{}ull", text.len()),
                        format!("{pos}u"),
                    ],
                );
                self.assign(out, result, &call)?;
                self.emit_pending_check(out);
                Ok(())
            }
            l::InstructionKind::Zero => {
                let id = instruction
                    .result
                    .ok_or_else(|| internal("Zero has no result"))?;
                let zero = self.emitter.zero(self.value_type(id)?)?;
                self.assign(out, result, &zero)
            }
            l::InstructionKind::LoadLocal(local) => self.assign(out, result, &self.local(*local)),
            l::InstructionKind::StoreLocal(local) => {
                let local = self.local(*local);
                if local == operands[0] {
                    Ok(())
                } else {
                    self.assign(out, Some(local), &operands[0])
                }
            }
            l::InstructionKind::AddressOfLocal(local) => {
                if instruction
                    .result
                    .is_some_and(|result| self.folded_addresses.contains(&result))
                {
                    Ok(())
                } else {
                    self.assign(out, result, &format!("&{}", self.local(*local)))
                }
            }
            l::InstructionKind::LoadGlobal(global) => self.assign(
                out,
                result,
                &format!("subscript_globals(ctx)->g{}", global.0),
            ),
            l::InstructionKind::StoreGlobal(global) => self.assign(
                out,
                Some(format!("subscript_globals(ctx)->g{}", global.0)),
                &operands[0],
            ),
            l::InstructionKind::AddressOfGlobal(global) => self.assign(
                out,
                result,
                &format!("&subscript_globals(ctx)->g{}", global.0),
            ),
            l::InstructionKind::FunctionRef(function) => self.assign(
                out,
                result,
                &format!("(SubFn){{ (void*)&sub_w{}, NULL }}", function.0),
            ),
            l::InstructionKind::Unary(operator) => {
                let expression = match operator {
                    l::UnaryOp::Neg => format!("(-({}))", operands[0]),
                    l::UnaryOp::Not => format!("(!({}))", operands[0]),
                    l::UnaryOp::BitNot => format!("(~({}))", operands[0]),
                };
                self.assign(out, result, &expression)
            }
            l::InstructionKind::Binary(operator) => {
                self.emit_binary(out, instruction, *operator, &operands, &operand_types)
            }
            l::InstructionKind::Cast | l::InstructionKind::Coerce => {
                self.emit_conversion(out, instruction, &operands[0], &operand_types[0])
            }
            l::InstructionKind::AllocateClass(class) => {
                self.emit_allocate_class(out, instruction, *class, result)
            }
            l::InstructionKind::AddressOfValue => {
                let id = instruction
                    .result
                    .ok_or_else(|| internal("AddressOfValue has no result"))?;
                if self.coroutine {
                    let _ = writeln!(out, "    frame->stable_v{} = {};", id.0, operands[0]);
                    self.assign(out, result, &format!("&frame->stable_v{}", id.0))
                } else {
                    let temporary = self.fresh();
                    let pointee = match self.value_type(id)? {
                        l::ValueType::Address(address) => &address.pointee,
                        _ => return Err(internal("AddressOfValue result is not an address")),
                    };
                    let _ = writeln!(
                        out,
                        "    {} {temporary} = {};",
                        self.emitter.ctype(pointee)?,
                        operands[0]
                    );
                    self.assign(out, result, &format!("&{temporary}"))
                }
            }
            l::InstructionKind::AddressOfField(field) => {
                self.emit_field_address(out, instruction, *field, &operands, &operand_types, result)
            }
            l::InstructionKind::AddressOfIndex { checked } => self.emit_index_address(
                out,
                instruction,
                *checked,
                &operands,
                &operand_types,
                result,
            ),
            l::InstructionKind::LoadAddress => {
                let expression = match instruction.operands.first() {
                    Some(l::Operand::Value(address)) if self.folded_addresses.contains(address) => {
                        self.folded_address_expression(*address)?
                    }
                    _ => format!("*({})", operands[0]),
                };
                self.assign(out, result, &expression)
            }
            l::InstructionKind::StoreAddress => {
                let destination = match instruction.operands.first() {
                    Some(l::Operand::Value(address)) if self.folded_addresses.contains(address) => {
                        self.folded_address_expression(*address)?
                    }
                    _ => format!("*({})", operands[0]),
                };
                self.assign(out, Some(destination), &operands[1])
            }
            l::InstructionKind::LoadField(field) => {
                self.emit_load_field(out, instruction, *field, &operands, &operand_types, result)
            }
            l::InstructionKind::Length => {
                self.emit_length(out, &operands[0], &operand_types[0], result)
            }
            l::InstructionKind::ForeignArrayData => {
                let call = self.emitter.runtime_call(
                    "const void*",
                    "subscript_rt_array_data",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), operands[0].clone()],
                );
                self.assign(out, result, &call)
            }
            l::InstructionKind::ArrayLiteral => {
                self.emit_array_literal(out, instruction, &operands, result)
            }
            l::InstructionKind::ArraySpreadLiteral(spreads) => {
                self.emit_spread_array(out, instruction, spreads, &operands, &operand_types, result)
            }
            l::InstructionKind::Template(parts) => {
                self.emit_template(out, instruction, parts, &operands, &operand_types, result)
            }
            l::InstructionKind::MakeClosure(function) => {
                self.emit_closure(out, instruction, *function, &operands, result)
            }
            l::InstructionKind::Call(target) => {
                self.emit_call(out, instruction, target, &operands, &operand_types, result)
            }
            l::InstructionKind::IteratorCreate(kind) => {
                self.emit_iterator_create(out, *kind, &operands[0], &operand_types[0], result)
            }
            l::InstructionKind::IteratorBound => {
                self.emit_iterator_bound(out, instruction, &operands[0], &operand_types[0], result)
            }
            l::InstructionKind::IteratorHasNext => {
                self.emit_iterator_has_next(out, instruction, &operands, &operand_types, result)
            }
            l::InstructionKind::IteratorValue => {
                self.emit_iterator_value(out, instruction, &operands, &operand_types, result)
            }
            l::InstructionKind::IteratorAdvance => {
                self.emit_iterator_advance(out, instruction, &operands, &operand_types, result)
            }
        }
    }

    fn assign(
        &mut self,
        out: &mut String,
        destination: Option<String>,
        value: &str,
    ) -> Result<(), String> {
        let Some(destination) = destination else {
            return Ok(());
        };
        let delayed = self
            .delayed_declarations
            .iter()
            .find(|candidate| self.value(**candidate) == destination)
            .copied();
        if let Some(delayed) = delayed {
            self.delayed_declarations.remove(&delayed);
            let ctype = self.emitter.value_ctype(self.value_type(delayed)?)?;
            let _ = writeln!(out, "    {ctype} {destination} = {value};");
            return Ok(());
        }
        let _ = writeln!(out, "    {destination} = {value};");
        Ok(())
    }

    fn operand(&mut self, operand: &l::Operand) -> Result<String, String> {
        match operand {
            l::Operand::Value(value) => Ok(self.value(*value)),
            l::Operand::Constant(constant) => self.constant(constant),
        }
    }

    fn operand_type(&self, operand: &l::Operand) -> Result<l::ValueType, String> {
        match operand {
            l::Operand::Value(value) => Ok(self.value_type(*value)?.clone()),
            l::Operand::Constant(constant) => Ok(l::ValueType::Data(constant.ty.clone())),
        }
    }

    fn constant(&mut self, constant: &l::Constant) -> Result<String, String> {
        Ok(match &constant.kind {
            l::ConstantKind::Integer(value) => int_literal(*value, &constant.ty),
            l::ConstantKind::FloatBits(bits) => match constant.ty {
                Type::F16 => self.emitter.runtime_call(
                    "uint16_t",
                    "subscript_rt_f16_from_f64",
                    &["double".into()],
                    &[float_literal(f64::from_bits(*bits), &Type::F64)],
                ),
                Type::F32 => float_literal(f64::from(f32::from_bits(*bits as u32)), &constant.ty),
                Type::F64 => float_literal(f64::from_bits(*bits), &constant.ty),
                ref other => return Err(internal(format!("float bits have type {other:?}"))),
            },
            l::ConstantKind::Boolean(value) => i32::from(*value).to_string(),
            l::ConstantKind::Null => "NULL".into(),
        })
    }

    fn take_pending_trap(
        &mut self,
        traps: &[l::Trap],
        kind: l::TrapKind,
    ) -> Result<l::Trap, String> {
        let trap = traps
            .iter()
            .find(|trap| trap.kind == kind)
            .cloned()
            .ok_or_else(|| internal(format!("operation has no {kind:?} trap")))?;
        self.consumed_traps.push(trap.clone());
        Ok(trap)
    }

    fn consume(&mut self, trap: &l::Trap) {
        self.consumed_traps.push(trap.clone());
    }

    fn emit_pending_check(&self, out: &mut String) {
        out.push_str("    if (*(const uint32_t*)ctx != 0u) goto unwind;\n");
    }

    fn emit_pop(&mut self, out: &mut String) {
        if !self.rooted_values.is_empty() || !self.rooted_locals.is_empty() {
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_shadow_pop",
                &["void*".into()],
                &["ctx".into()],
            );
            let _ = writeln!(out, "    {call};");
        }
    }

    fn emit_unwind(&mut self, out: &mut String) -> Result<(), String> {
        out.push_str("unwind:\n");
        self.emit_pop(out);
        if self.coroutine {
            out.push_str("    return 1;\ncoroutine_done:\n    return 1;\n");
        } else if self.function.return_type == Type::Void {
            out.push_str("    return;\n");
        } else {
            let zero = self
                .emitter
                .zero(&l::ValueType::Data(self.function.return_type.clone()))?;
            let _ = writeln!(out, "    return {zero};");
        }
        Ok(())
    }

    fn emit_terminator(&mut self, out: &mut String, block: &l::BasicBlock) -> Result<(), String> {
        match &block.terminator {
            l::Terminator::Branch(target) => self.emit_edge(out, block.id, target),
            l::Terminator::ConditionalBranch {
                condition,
                then_target,
                else_target,
            } => {
                let condition = self.operand(condition)?;
                let next = self.fresh();
                let _ = writeln!(
                    out,
                    "    if ({condition}) goto {next}_then; else goto {next}_else;\n{next}_then:"
                );
                self.emit_edge(out, block.id, then_target)?;
                let _ = writeln!(out, "{next}_else:");
                self.emit_edge(out, block.id, else_target)
            }
            l::Terminator::Switch {
                value,
                arms,
                default,
            } => {
                let switch_type = self.operand_type(value)?;
                let value_type = data_type(&switch_type)?;
                let value = self.operand(value)?;
                let _ = writeln!(
                    out,
                    "    {{\n    {} _disc = {value};\n    switch (_disc) {{",
                    self.emitter.ctype(value_type)?
                );
                for arm in arms {
                    let constant = self.constant(&arm.value)?;
                    let _ = writeln!(out, "    case {constant}:");
                    self.emit_edge(out, block.id, &arm.target)?;
                }
                out.push_str("    default:\n");
                self.emit_edge(out, block.id, default)?;
                out.push_str("    }\n    }\n");
                Ok(())
            }
            l::Terminator::Return { value, pos } => {
                let mut value = value
                    .as_ref()
                    .map(|value| self.operand(value))
                    .transpose()?;
                if self.coroutine {
                    if self.function.is_async {
                        if let Some(value) = value {
                            let ty = self.emitter.ctype(&self.function.return_type)?;
                            let _ = writeln!(out, "    *(({ty}*)coroutine_out) = {value};");
                        }
                    }
                    out.push_str("    frame->state = 0x7fffffff;\n");
                    self.emit_pop(out);
                    out.push_str("    return 1;\n");
                } else {
                    if let (Some(returned), Type::Class(class)) =
                        (value.as_ref(), &self.function.return_type)
                    {
                        if self.emitter.is_value_class(*class)?
                            && boundary_class_contains_pointer_member(
                                self.emitter.module,
                                *class,
                                &mut HashSet::new(),
                            )
                        {
                            let stable = self.fresh();
                            let _ = writeln!(
                                out,
                                "    {} {stable} = {returned};",
                                self.emitter.class_name(*class)
                            );
                            self.emit_stabilize_boundary_return_value(
                                out,
                                *class,
                                &format!("&{stable}"),
                                pos,
                                &mut HashSet::new(),
                            )?;
                            value = Some(stable);
                        }
                    }
                    self.emit_pop(out);
                    if let Some(value) = value {
                        let _ = writeln!(out, "    return {value};");
                    } else {
                        out.push_str("    return;\n");
                    }
                }
                Ok(())
            }
            l::Terminator::Unreachable { .. } => {
                out.push_str("    goto unwind;\n");
                Ok(())
            }
            l::Terminator::Trap(trap) => {
                self.consume(trap);
                let pos = self.emitter.pos_id(&trap.pos);
                let kind = trap_runtime_kind(&trap.kind)? as u32;
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_trap",
                    &["void*".into(), "uint32_t".into(), "uint32_t".into()],
                    &["ctx".into(), format!("{kind}u"), format!("{pos}u")],
                );
                let _ = writeln!(out, "    {call};\n    goto unwind;");
                Ok(())
            }
            l::Terminator::Suspend { .. } => self.emit_suspend(out, block),
        }
    }

    fn emit_stabilize_boundary_return_value(
        &mut self,
        out: &mut String,
        class: ClassId,
        address: &str,
        position: &Pos,
        visiting: &mut HashSet<ClassId>,
    ) -> Result<(), String> {
        if !visiting.insert(class) {
            return Ok(());
        }
        let definition = self.emitter.class(class)?.clone();
        for field in &definition.fields {
            let field_address = format!("({address})->d{}", field.id.0);
            match &field.ty {
                Type::Nullable(inner) => {
                    let Type::Class(target) = inner.as_ref() else {
                        continue;
                    };
                    if !self.emitter.is_value_class(*target)? {
                        continue;
                    }
                    let target_type = self.emitter.class_name(*target);
                    let stable = self.fresh();
                    let _ = writeln!(out, "    if ({field_address} != NULL) {{");
                    let pos = self.emitter.pos_id(position);
                    let allocation = self.emitter.runtime_call(
                        "void*",
                        "subscript_rt_boundary_scratch_alloc",
                        &["void*".into(), "uint64_t".into(), "uint32_t".into()],
                        &[
                            "ctx".into(),
                            format!("(uint64_t)sizeof({target_type})"),
                            format!("{pos}u"),
                        ],
                    );
                    let _ = writeln!(
                        out,
                        "        {target_type}* {stable} = ({target_type}*){allocation};\n        if (*(const uint32_t*)ctx != 0u) goto unwind;\n        memcpy({stable}, (const void*)({field_address}), sizeof *{stable});\n        {field_address} = (void*){stable};"
                    );
                    self.emit_stabilize_boundary_return_value(
                        out, *target, &stable, position, visiting,
                    )?;
                    out.push_str("    }\n");
                }
                Type::Class(nested) if self.emitter.is_value_class(*nested)? => {
                    self.emit_stabilize_boundary_return_value(
                        out,
                        *nested,
                        &format!("&{field_address}"),
                        position,
                        visiting,
                    )?;
                }
                Type::Array(element) => {
                    let Type::Class(element) = element.as_ref() else {
                        continue;
                    };
                    if !self.emitter.is_value_class(*element)? {
                        continue;
                    }
                    let element_type = self.emitter.class_name(*element);
                    let count = self.fresh();
                    let data = self.fresh();
                    let index = self.fresh();
                    let length = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_len",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), field_address.clone()],
                    );
                    let storage = self.emitter.runtime_call(
                        "const void*",
                        "subscript_rt_array_data",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), field_address],
                    );
                    let _ = writeln!(
                        out,
                        "    int32_t {count} = {length};\n    {element_type}* {data} = ({element_type}*){storage};\n    for (int32_t {index} = 0; {index} < {count}; {index}++) {{"
                    );
                    self.emit_stabilize_boundary_return_value(
                        out,
                        *element,
                        &format!("&{data}[{index}]"),
                        position,
                        visiting,
                    )?;
                    out.push_str("    }\n");
                }
                _ => {}
            }
        }
        visiting.remove(&class);
        Ok(())
    }

    fn emit_edge(
        &mut self,
        out: &mut String,
        source: l::BlockId,
        target: &l::BlockTarget,
    ) -> Result<(), String> {
        let destination = &self.function.blocks[target.block.0 as usize];
        let mut copies = Vec::new();
        for (index, (argument, parameter)) in target
            .arguments
            .iter()
            .zip(&destination.parameters)
            .enumerate()
        {
            if self
                .removable_edge_copies
                .contains(&(source, target.block, index))
            {
                continue;
            }
            let destination = self.value_storage[parameter.0 as usize];
            let source = match argument {
                l::Operand::Value(value) => {
                    let value = self.value_storage[value.0 as usize];
                    if value == destination {
                        continue;
                    }
                    EdgeCopySource::Value(value)
                }
                l::Operand::Constant(constant) => EdgeCopySource::Constant(constant.clone()),
            };
            copies.push(EdgeCopy {
                destination,
                source,
            });
        }
        while !copies.is_empty() {
            if let Some(index) = copies.iter().position(|copy| {
                !copies.iter().any(|other| {
                    matches!(other.source, EdgeCopySource::Value(source) if source == copy.destination)
                })
            }) {
                let copy = copies.remove(index);
                let value = match copy.source {
                    EdgeCopySource::Value(value) => self.value(value),
                    EdgeCopySource::Constant(constant) => self.constant(&constant)?,
                    EdgeCopySource::Temporary(temporary) => temporary,
                };
                let _ = writeln!(out, "    {} = {value};", self.value(copy.destination));
                continue;
            }

            let cycle_source = copies
                .iter()
                .find_map(|copy| match copy.source {
                    EdgeCopySource::Value(value) => Some(value),
                    EdgeCopySource::Constant(_) | EdgeCopySource::Temporary(_) => None,
                })
                .ok_or_else(|| internal("parallel edge copies could not make progress"))?;
            // One saved source breaks this cycle. Copies outside the cycle
            // remain pending and can expose another independent cycle.
            let temporary = self.fresh();
            let _ = writeln!(
                out,
                "    {} {temporary} = {};",
                self.emitter.value_ctype(self.value_type(cycle_source)?)?,
                self.value(cycle_source)
            );
            for copy in &mut copies {
                if matches!(copy.source, EdgeCopySource::Value(value) if value == cycle_source) {
                    copy.source = EdgeCopySource::Temporary(temporary.clone());
                }
            }
        }
        let _ = writeln!(out, "    goto b{};", target.block.0);
        Ok(())
    }

    fn emit_suspend(&mut self, out: &mut String, block: &l::BasicBlock) -> Result<(), String> {
        let l::Terminator::Suspend {
            kind,
            arguments,
            traps,
            ..
        } = &block.terminator
        else {
            return Err(internal("non-suspend passed to suspend emitter"));
        };
        self.save_suspend_arguments(out, block, arguments)?;
        let state = self.suspend_state(block.id)?;
        match kind {
            l::SuspendKind::Yield(value) => {
                if let Some(value) = value {
                    let ty = data_type(self.value_type(*value)?)?;
                    let _ = writeln!(
                        out,
                        "    *(({}*)coroutine_out) = {};",
                        self.emitter.ctype(ty)?,
                        self.value(*value)
                    );
                }
                let _ = writeln!(out, "    frame->state = {state};");
                self.emit_pop(out);
                out.push_str("    return 0;\n");
            }
            l::SuspendKind::Async => {
                let _ = writeln!(out, "    frame->state = {state};");
                self.emit_pop(out);
                out.push_str("    return 0;\n");
            }
            l::SuspendKind::AsyncCall { target, operands } => {
                self.emit_async_child_create(out, block, target, operands, traps)?;
                self.emit_async_child_resume(out, block, state)?;
            }
        }
        Ok(())
    }

    fn suspend_state(&self, id: l::BlockId) -> Result<u32, String> {
        self.function
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
            .position(|block| block.id == id)
            .map(|index| index as u32 + 1)
            .ok_or_else(|| internal(format!("suspend block {} is missing", id.0)))
    }

    fn save_suspend_arguments(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        arguments: &[l::Operand],
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            return Err(internal("save arguments on non-suspend"));
        };
        let destination = &self.function.blocks[successor.0 as usize];
        for (argument, parameter) in arguments.iter().zip(
            destination
                .parameters
                .iter()
                .skip(usize::from(resume_value.is_some())),
        ) {
            let value = self.operand(argument)?;
            let _ = writeln!(
                out,
                "    frame->b{}_v{} = {value};",
                block.id.0, parameter.0
            );
        }
        Ok(())
    }

    fn restore_suspend_arguments(
        &self,
        out: &mut String,
        block: &l::BasicBlock,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            return Err(internal("restore arguments on non-suspend"));
        };
        let destination = &self.function.blocks[successor.0 as usize];
        for parameter in destination
            .parameters
            .iter()
            .skip(usize::from(resume_value.is_some()))
        {
            let _ = writeln!(
                out,
                "    {} = frame->b{}_v{};",
                self.value(*parameter),
                block.id.0,
                parameter.0
            );
        }
        Ok(())
    }

    fn emit_async_child_create(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        target: &l::CallTarget,
        operands: &[l::ValueId],
        traps: &[l::Trap],
    ) -> Result<(), String> {
        let function = match target.kind {
            l::CallTargetKind::Function(function) => function,
            l::CallTargetKind::Method(method) => self.emitter.method_function(method)?,
            ref other => return Err(internal(format!("async target {other:?} is invalid"))),
        };
        for trap in traps {
            match trap.kind {
                l::TrapKind::DevOnlyLifetime => self.consume(trap),
                l::TrapKind::Call => {}
                _ => {
                    return Err(internal(format!(
                        "unexpected async-call trap {:?}",
                        trap.kind
                    )))
                }
            }
        }
        let args = operands
            .iter()
            .map(|id| self.value(*id))
            .collect::<Vec<_>>();
        let separator = if args.is_empty() { "" } else { ", " };
        let _ = writeln!(
            out,
            "    frame->b{}_child = sub_f{}(ctx{separator}{});",
            block.id.0,
            function.0,
            args.join(", ")
        );
        if let Some(trap) = traps.iter().find(|trap| trap.kind == l::TrapKind::Call) {
            self.consume(trap);
            self.emit_pending_check(out);
        }
        Ok(())
    }

    fn emit_async_child_resume(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        state: u32,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            kind: l::SuspendKind::AsyncCall { target, .. },
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            return Err(internal("child resume on non-call suspend"));
        };
        let function = match target.kind {
            l::CallTargetKind::Function(function) => function,
            l::CallTargetKind::Method(method) => self.emitter.method_function(method)?,
            ref other => return Err(internal(format!("async target {other:?} is invalid"))),
        };
        let output = if let Some(value) = resume_value {
            format!("&{}", self.value(*value))
        } else {
            "NULL".into()
        };
        let done = self.fresh();
        let _ = writeln!(
            out,
            "    uint8_t {done} = sub_f{}_resume(ctx, frame->b{}_child, {output});",
            function.0, block.id.0
        );
        self.emit_pending_check(out);
        let _ = writeln!(out, "    if (!{done}) {{ frame->state = {state};");
        self.emit_pop(out);
        out.push_str("        return 0;\n    }\n");
        self.restore_suspend_arguments(out, block)?;
        let _ = writeln!(out, "    goto b{};", successor.0);
        Ok(())
    }

    // Instruction families are implemented below in semantic groups.
}

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    fn emit_binary(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operator: l::BinaryOp,
        operands: &[String],
        operand_types: &[l::ValueType],
    ) -> Result<(), String> {
        let destination = instruction
            .result
            .map(|result| self.value(result))
            .ok_or_else(|| internal("binary instruction has no result"))?;
        let ty = data_type(
            operand_types
                .first()
                .ok_or_else(|| internal("binary operand type is missing"))?,
        )?;
        if *ty == Type::Str {
            return match operator {
                l::BinaryOp::Add => {
                    let trap =
                        self.take_pending_trap(&instruction.traps, l::TrapKind::Allocation)?;
                    let pos = self.emitter.pos_id(&trap.pos);
                    let call = self.emitter.runtime_call(
                        "void*",
                        "subscript_rt_str_concat",
                        &[
                            "void*".into(),
                            "const void*".into(),
                            "const void*".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            operands[0].clone(),
                            operands[1].clone(),
                            format!("{pos}u"),
                        ],
                    );
                    let _ = writeln!(out, "    {destination} = {call};");
                    self.emit_pending_check(out);
                    Ok(())
                }
                l::BinaryOp::Eq | l::BinaryOp::Ne => {
                    let call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_str_eq",
                        &["void*".into(), "const void*".into(), "const void*".into()],
                        &["ctx".into(), operands[0].clone(), operands[1].clone()],
                    );
                    let negation = if operator == l::BinaryOp::Ne { "!" } else { "" };
                    let _ = writeln!(out, "    {destination} = {negation}({call});");
                    Ok(())
                }
                other => Err(internal(format!(
                    "invalid string binary operator {other:?}"
                ))),
            };
        }
        if *ty == Type::F16 {
            if !matches!(
                operator,
                l::BinaryOp::Eq
                    | l::BinaryOp::Ne
                    | l::BinaryOp::Lt
                    | l::BinaryOp::Le
                    | l::BinaryOp::Gt
                    | l::BinaryOp::Ge
            ) {
                return Err(internal(format!(
                    "invalid f16 binary operator {operator:?}"
                )));
            }
            let left = self.emitter.runtime_call(
                "double",
                "subscript_rt_f16_to_f64",
                &["uint16_t".into()],
                &[operands[0].clone()],
            );
            let right = self.emitter.runtime_call(
                "double",
                "subscript_rt_f16_to_f64",
                &["uint16_t".into()],
                &[operands[1].clone()],
            );
            let symbol = binary_symbol(operator)?;
            let _ = writeln!(out, "    {destination} = ({left}) {symbol} ({right});");
            return Ok(());
        }
        if matches!(operator, l::BinaryOp::Div | l::BinaryOp::Rem) && ty.is_integer() {
            let trap = self.take_pending_trap(&instruction.traps, l::TrapKind::DivisionByZero)?;
            let pos = self.emitter.pos_id(&trap.pos);
            let trap_call = self.emitter.runtime_call(
                "void",
                "subscript_rt_trap",
                &["void*".into(), "uint32_t".into(), "uint32_t".into()],
                &[
                    "ctx".into(),
                    format!("{}u", TrapKind::DivisionByZero as u32),
                    format!("{pos}u"),
                ],
            );
            let _ = writeln!(
                out,
                "    if (({}) == 0) {{ {trap_call}; goto unwind; }}",
                operands[1]
            );
            let ctype = self.emitter.ctype(ty)?;
            if is_unsigned(ty) {
                let symbol = if operator == l::BinaryOp::Div {
                    "/"
                } else {
                    "%"
                };
                let _ = writeln!(
                    out,
                    "    {destination} = ({ctype})(({}) {symbol} ({}));",
                    operands[0], operands[1]
                );
            } else if operator == l::BinaryOp::Div {
                let _ = writeln!(
                    out,
                    "    {destination} = (({}) == ({ctype})-1) ? ({ctype})(0 - ({})({})) : ({ctype})(({}) / ({}));",
                    operands[1],
                    unsigned_ctype(ty)?,
                    operands[0],
                    operands[0],
                    operands[1]
                );
            } else {
                let _ = writeln!(
                    out,
                    "    {destination} = (({}) == ({ctype})-1) ? ({ctype})0 : ({ctype})(({}) % ({}));",
                    operands[1], operands[0], operands[1]
                );
            }
            return Ok(());
        }
        let expression = match operator {
            l::BinaryOp::Shl | l::BinaryOp::Shr | l::BinaryOp::UShr => {
                shift_expression(operator, ty, &operands[0], &operands[1])?
            }
            l::BinaryOp::Add | l::BinaryOp::Sub | l::BinaryOp::Mul if ty.is_integer() => {
                let symbol = binary_symbol(operator)?;
                let carrier = unsigned_ctype(ty)?;
                let target = self.emitter.ctype(ty)?;
                format!(
                    "(({target})((({carrier})({})) {symbol} (({carrier})({}))))",
                    operands[0], operands[1]
                )
            }
            _ => format!(
                "(({}) {} ({}))",
                operands[0],
                binary_symbol(operator)?,
                operands[1]
            ),
        };
        let _ = writeln!(out, "    {destination} = {expression};");
        Ok(())
    }

    fn emit_conversion(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operand: &str,
        operand_type: &l::ValueType,
    ) -> Result<(), String> {
        let result = instruction
            .result
            .ok_or_else(|| internal("conversion has no result"))?;
        let destination = self.value(result);
        let result_type = self.value_type(result)?.clone();
        for trap in &instruction.traps {
            match trap.kind {
                l::TrapKind::NullNarrowing => {
                    self.consume(trap);
                    self.emit_guard(out, &format!("({operand}) != NULL"), trap)?;
                }
                l::TrapKind::ClassMismatch(class) => {
                    self.consume(trap);
                    let offset = rtc::CLASS_ID_OFFSET;
                    self.emit_guard(
                        out,
                        &format!(
                            "(*(const uint32_t*)((const unsigned char*)({operand}) + {offset})) == {}u",
                            class.0
                        ),
                        trap,
                    )?;
                }
                l::TrapKind::DevOnlyLifetime => self.consume(trap),
                ref other => {
                    return Err(internal(format!(
                        "conversion carries unexpected trap {other:?}"
                    )))
                }
            }
        }
        if matches!(operand_type, l::ValueType::Address(_))
            && matches!(result_type, l::ValueType::Data(Type::Nullable(_)))
        {
            let _ = writeln!(out, "    {destination} = (void*)({operand});");
            return Ok(());
        }
        if let (
            l::ValueType::Data(Type::Class(source)),
            l::ValueType::Data(Type::Nullable(target)),
        ) = (operand_type, &result_type)
        {
            if matches!(target.as_ref(), Type::Class(target) if target == source)
                && self.emitter.is_value_class(*source)?
            {
                let _ = writeln!(out, "    {destination} = (void*)&({operand});");
                return Ok(());
            }
        }
        let source = data_type(operand_type)?;
        let target = data_type(&result_type)?;
        let expression = if source == target {
            operand.to_string()
        } else if *target == Type::F16 {
            let value = if *source == Type::F32 {
                format!("(double)({operand})")
            } else {
                operand.to_string()
            };
            self.emitter.runtime_call(
                "uint16_t",
                "subscript_rt_f16_from_f64",
                &["double".into()],
                &[value],
            )
        } else if *source == Type::F16 {
            let wide = self.emitter.runtime_call(
                "double",
                "subscript_rt_f16_to_f64",
                &["uint16_t".into()],
                &[operand.into()],
            );
            format!("({})({wide})", self.emitter.ctype(target)?)
        } else if source.is_float() && target.is_integer() {
            format!("{}({operand})", float_to_int_helper(target)?)
        } else {
            format!("(({})({operand}))", self.emitter.ctype(target)?)
        };
        let _ = writeln!(out, "    {destination} = {expression};");
        Ok(())
    }

    fn emit_guard(
        &mut self,
        out: &mut String,
        condition: &str,
        trap: &l::Trap,
    ) -> Result<(), String> {
        let pos = self.emitter.pos_id(&trap.pos);
        let kind = trap_runtime_kind(&trap.kind)? as u32;
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_trap",
            &["void*".into(), "uint32_t".into(), "uint32_t".into()],
            &["ctx".into(), format!("{kind}u"), format!("{pos}u")],
        );
        let _ = writeln!(out, "    if (!({condition})) {{ {call}; goto unwind; }}");
        Ok(())
    }

    fn emit_allocate_class(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        class: ClassId,
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("AllocateClass has no result"))?;
        if self.emitter.is_value_class(class)? {
            let result_id = instruction
                .result
                .ok_or_else(|| internal("allocation has no id"))?;
            if matches!(self.value_type(result_id)?, l::ValueType::Address(_)) {
                if self.coroutine {
                    let _ = writeln!(
                        out,
                        "    memset(&frame->stable_v{}, 0, sizeof frame->stable_v{});",
                        result_id.0, result_id.0
                    );
                    let _ = writeln!(out, "    {destination} = &frame->stable_v{};", result_id.0);
                } else {
                    let temporary = self.fresh();
                    let _ = writeln!(
                        out,
                        "    {} {temporary} = ({} ){{0}};",
                        self.emitter.class_name(class),
                        self.emitter.class_name(class)
                    );
                    self.assign(out, Some(destination), &format!("&{temporary}"))?;
                }
            } else {
                let zero = format!("({}){{0}}", self.emitter.class_name(class));
                self.assign(out, Some(destination), &zero)?;
            }
            for trap in &instruction.traps {
                self.consume(trap);
            }
            return Ok(());
        }
        let trap = self.take_pending_trap(&instruction.traps, l::TrapKind::Allocation)?;
        let pos = self.emitter.pos_id(&trap.pos);
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_alloc",
            &[
                "void*".into(),
                "uint64_t".into(),
                "uint32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({})", self.emitter.class_name(class)),
                format!("{}u", class.0),
                format!("{pos}u"),
            ],
        );
        self.assign(out, Some(destination), &call)?;
        self.emit_pending_check(out);
        Ok(())
    }

    fn emit_field_address(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        field: l::FieldRef,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        for trap in &instruction.traps {
            if trap.kind == l::TrapKind::DevOnlyLifetime {
                self.consume(trap);
            }
        }
        let result_id = instruction
            .result
            .ok_or_else(|| internal("field address has no result"))?;
        if self.folded_addresses.contains(&result_id) {
            return Ok(());
        }
        let destination = result.ok_or_else(|| internal("field address has no destination"))?;
        let expression = match field {
            l::FieldRef::Class(field) => {
                let (class, _, _) = self.emitter.field(field)?;
                match &operand_types[0] {
                    l::ValueType::Address(_) => format!("&(({})->d{})", operands[0], field.0),
                    l::ValueType::Data(Type::Class(id)) if self.emitter.is_value_class(*id)? => {
                        format!("&(({}).d{})", operands[0], field.0)
                    }
                    l::ValueType::Data(Type::Class(_)) => format!(
                        "&((({}*)({}))->d{})",
                        self.emitter.class_name(class),
                        operands[0],
                        field.0
                    ),
                    other => return Err(internal(format!("field base is invalid: {other:?}"))),
                }
            }
            l::FieldRef::IterDone => format!("&(({}).done)", operands[0]),
            l::FieldRef::IterValue => format!("&(({}).value)", operands[0]),
        };
        let _ = writeln!(out, "    {destination} = {expression};");
        Ok(())
    }

    fn emit_load_field(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        field: l::FieldRef,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if let l::FieldRef::Class(field_id) = field {
            let (class_id, _, _) = self.emitter.field(field_id)?;
            if instruction
                .traps
                .iter()
                .any(|trap| trap.kind == l::TrapKind::JsonResultValue)
            {
                let ok_id = self
                    .emitter
                    .class(class_id)?
                    .fields
                    .iter()
                    .find(|field| field.source_name == "ok" && field.ty == Type::Bool)
                    .map(|field| field.id)
                    .ok_or_else(|| internal("JsonResult class has no ok field"))?;
                let condition = format!(
                    "((({}*)({}))->d{})",
                    self.emitter.class_name(class_id),
                    operands[0],
                    ok_id.0
                );
                for trap in &instruction.traps {
                    if trap.kind == l::TrapKind::JsonResultValue {
                        self.consume(trap);
                        self.emit_guard(out, &condition, trap)?;
                    }
                }
            }
            let expression = match &operand_types[0] {
                l::ValueType::Address(address) if matches!(&address.pointee, Type::Class(id) if self.emitter.is_value_class(*id)?) =>
                {
                    format!("({})->d{}", operands[0], field_id.0)
                }
                l::ValueType::Data(Type::Class(id)) if self.emitter.is_value_class(*id)? => {
                    format!("({}).d{}", operands[0], field_id.0)
                }
                l::ValueType::Data(Type::Class(_)) => format!(
                    "((({}*)({}))->d{})",
                    self.emitter.class_name(class_id),
                    operands[0],
                    field_id.0
                ),
                other => return Err(internal(format!("field load base is invalid: {other:?}"))),
            };
            for trap in &instruction.traps {
                match &trap.kind {
                    l::TrapKind::DevOnlyLifetime => self.consume(trap),
                    l::TrapKind::WireEnumValue(alias) => {
                        self.consume(trap);
                        self.emit_wire_validation(out, &expression, *alias, trap)?;
                    }
                    l::TrapKind::JsonResultValue => {}
                    other => return Err(internal(format!("field load trap {other:?} is invalid"))),
                }
            }
            return self.assign(out, result, &expression);
        }
        let expression = match field {
            l::FieldRef::IterDone => format!("({}).done", operands[0]),
            l::FieldRef::IterValue => format!("({}).value", operands[0]),
            l::FieldRef::Class(_) => unreachable!(),
        };
        self.assign(out, result, &expression)
    }

    fn emit_wire_validation(
        &mut self,
        out: &mut String,
        value: &str,
        alias: subscript_compiler::types::StringAliasId,
        trap: &l::Trap,
    ) -> Result<(), String> {
        let definition = self
            .emitter
            .module
            .string_aliases
            .get(alias.0)
            .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
        let wires = definition
            .wire_values
            .as_ref()
            .ok_or_else(|| internal("wire validation targets a plain string alias"))?;
        let valid = wires
            .iter()
            .map(|wire| format!("({value}) == {wire}"))
            .collect::<Vec<_>>()
            .join(" || ");
        let pos = self.emitter.pos_id(&trap.pos);
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_trap_wire_enum",
            &[
                "void*".into(),
                "const unsigned char*".into(),
                "uint64_t".into(),
                "int32_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!(
                    "(const unsigned char*){}",
                    c_string_literal(definition.source_name.as_bytes())
                ),
                format!("{}ull", definition.source_name.len()),
                value.into(),
                format!("{pos}u"),
            ],
        );
        let condition = if valid.is_empty() { "0" } else { &valid };
        let _ = writeln!(out, "    if (!({condition})) {{ {call}; goto unwind; }}");
        Ok(())
    }

    fn emit_index_address(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        checked: bool,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("index address has no result"))?;
        let index = &operands[1];
        let (address, length) = match &operand_types[0] {
            l::ValueType::Data(Type::Array(element)) => {
                for trap in &instruction.traps {
                    if trap.kind == l::TrapKind::DevOnlyLifetime {
                        self.consume(trap);
                    }
                }
                let header = self.fresh();
                let _ = writeln!(
                    out,
                    "    SsArrayHeader* {header} = (SsArrayHeader*)({});",
                    operands[0]
                );
                let length = checked.then(|| {
                    let length = self.fresh();
                    let _ = writeln!(out, "    uint64_t {length} = {header}->len;");
                    length
                });
                (
                    format!(
                        "({}*)({header}->data + (int64_t)({index}) * (int64_t)({header}->elem_size))",
                        self.emitter.ctype(element)?
                    ),
                    length,
                )
            }
            l::ValueType::Data(Type::FixedArray(_element, count)) => (
                format!("&((({}).a)[{index}])", operands[0]),
                checked.then(|| count.to_string()),
            ),
            l::ValueType::Address(address) => match &address.pointee {
                Type::FixedArray(_element, count) => (
                    format!("&((({})->a)[{index}])", operands[0]),
                    checked.then(|| count.to_string()),
                ),
                other => return Err(internal(format!("indexed address points to {other:?}"))),
            },
            other => return Err(internal(format!("indexed base is invalid: {other:?}"))),
        };
        if checked {
            let trap = instruction
                .traps
                .iter()
                .find(|trap| matches!(trap.kind, l::TrapKind::IndexRead | l::TrapKind::IndexWrite))
                .ok_or_else(|| internal("checked index has no bounds trap"))?
                .clone();
            self.consume(&trap);
            let length = length
                .as_ref()
                .ok_or_else(|| internal("checked index has no captured length"))?;
            let pos = self.emitter.pos_id(&trap.pos);
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_trap_index_out_of_bounds",
                &[
                    "void*".into(),
                    "int32_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    index.clone(),
                    format!("(uint32_t)({length})"),
                    format!("{pos}u"),
                ],
            );
            let _ = writeln!(
                out,
                "    if ((int64_t)({index}) < 0 || (uint64_t)({index}) >= (uint64_t)({length})) {{ {call}; goto unwind; }}"
            );
        }
        if !self.folded_addresses.contains(&result_id) {
            let destination = result.ok_or_else(|| internal("index address has no destination"))?;
            let _ = writeln!(out, "    {destination} = {address};");
        }
        Ok(())
    }

    fn emit_length(
        &mut self,
        out: &mut String,
        operand: &str,
        operand_type: &l::ValueType,
        result: Option<String>,
    ) -> Result<(), String> {
        let expression = match operand_type {
            l::ValueType::Data(Type::Array(_)) => {
                format!("(int32_t)(((SsArrayHeader*)({operand}))->len)")
            }
            l::ValueType::Data(Type::Str) => self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_str_len",
                &["void*".into(), "const void*".into()],
                &["ctx".into(), operand.into()],
            ),
            l::ValueType::Data(Type::FixedArray(_, count)) => count.to_string(),
            other => return Err(internal(format!("length operand is invalid: {other:?}"))),
        };
        self.assign(out, result, &expression)
    }
}

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    fn materialize(&mut self, out: &mut String, value: &str, ty: &Type) -> Result<String, String> {
        let temporary = self.fresh();
        let _ = writeln!(
            out,
            "    {} {temporary} = {value};",
            self.emitter.ctype(ty)?
        );
        Ok(format!("&{temporary}"))
    }

    fn emit_array_literal(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("array literal has no result"))?;
        let result_type = data_type(self.value_type(result_id)?)?.clone();
        let destination = result.ok_or_else(|| internal("array literal result is missing"))?;
        match result_type {
            Type::FixedArray(element, count) => {
                if operands.len() != count as usize {
                    return Err(internal("fixed array literal arity mismatch"));
                }
                let initializer = format!(
                    "({}){{ .a = {{ {} }} }}",
                    self.emitter.fixed_array_name(&element, count)?,
                    operands.join(", ")
                );
                self.assign(out, Some(destination), &initializer)?;
                for trap in &instruction.traps {
                    self.consume(trap);
                }
                Ok(())
            }
            Type::Array(element) => {
                let mut traps = instruction.traps.iter();
                let initial = traps
                    .next()
                    .ok_or_else(|| internal("array literal has no allocation trap"))?;
                self.consume(initial);
                let pos = self.emitter.pos_id(&initial.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_new",
                    &["void*".into(), "uint64_t".into(), "uint32_t".into()],
                    &[
                        "ctx".into(),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(&element)?),
                        format!("{pos}u"),
                    ],
                );
                self.assign(out, Some(destination.clone()), &call)?;
                self.emit_pending_check(out);
                for operand in operands {
                    let trap = traps
                        .next()
                        .ok_or_else(|| internal("array push has no allocation trap"))?;
                    self.consume(trap);
                    let pos = self.emitter.pos_id(&trap.pos);
                    let pointer = self.materialize(out, operand, &element)?;
                    let call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_push",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            destination.clone(),
                            pointer,
                            format!("{pos}u"),
                        ],
                    );
                    let _ = writeln!(out, "    (void){call};");
                    self.emit_pending_check(out);
                }
                if traps.next().is_some() {
                    return Err(internal("array literal has unused traps"));
                }
                Ok(())
            }
            other => Err(internal(format!("array literal result is {other:?}"))),
        }
    }

    fn emit_spread_array(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        spreads: &[Option<l::SpreadKind>],
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("spread literal has no result"))?;
        let Type::Array(element) = data_type(self.value_type(result_id)?)? else {
            return Err(internal("spread literal result is not an array"));
        };
        let element = (**element).clone();
        let destination = result.ok_or_else(|| internal("spread result is missing"))?;
        let mut traps = instruction.traps.iter();
        let initial = traps
            .next()
            .ok_or_else(|| internal("spread literal has no allocation trap"))?;
        self.consume(initial);
        let pos = self.emitter.pos_id(&initial.pos);
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_array_new",
            &["void*".into(), "uint64_t".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({})", self.emitter.ctype(&element)?),
                format!("{pos}u"),
            ],
        );
        let _ = writeln!(out, "    {destination} = {call};");
        self.emit_pending_check(out);
        for ((spread, operand), operand_type) in spreads.iter().zip(operands).zip(operand_types) {
            let trap = traps
                .next()
                .ok_or_else(|| internal("spread part has no allocation trap"))?;
            self.consume(trap);
            let pos = self.emitter.pos_id(&trap.pos);
            let call = match spread {
                None => {
                    let pointer = self.materialize(out, operand, &element)?;
                    self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_push",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            destination.clone(),
                            pointer,
                            format!("{pos}u"),
                        ],
                    )
                }
                Some(l::SpreadKind::Array) => self.emitter.runtime_call(
                    "void",
                    "subscript_rt_array_spread_array",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        destination.clone(),
                        operand.clone(),
                        format!("{pos}u"),
                    ],
                ),
                Some(l::SpreadKind::FixedArray) => {
                    let l::ValueType::Data(Type::FixedArray(_, count)) = operand_type else {
                        return Err(internal("fixed spread source type is invalid"));
                    };
                    self.emitter.runtime_call(
                        "void",
                        "subscript_rt_array_spread_fixed",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "uint64_t".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            destination.clone(),
                            format!("&({operand})"),
                            format!("{count}ull"),
                            format!("{pos}u"),
                        ],
                    )
                }
                Some(l::SpreadKind::MapKeys | l::SpreadKind::SetValues) => {
                    self.emitter.runtime_call(
                        "void",
                        "subscript_rt_array_spread_assoc",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "void*".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            destination.clone(),
                            operand.clone(),
                            format!("{pos}u"),
                        ],
                    )
                }
                Some(l::SpreadKind::StringCodePoints) => self.emitter.runtime_call(
                    "void",
                    "subscript_rt_array_spread_string",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        destination.clone(),
                        operand.clone(),
                        format!("{pos}u"),
                    ],
                ),
            };
            let _ = writeln!(out, "    (void){call};");
            self.emit_pending_check(out);
        }
        if traps.next().is_some() {
            return Err(internal("spread literal has unused traps"));
        }
        Ok(())
    }

    fn emit_template(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        parts: &[l::TemplatePart],
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("template has no result"))?;
        let mut trap_index = 0usize;
        let mut accumulated: Option<String> = None;
        for part in parts {
            let piece = match part {
                l::TemplatePart::Text(text) => {
                    let trap = instruction
                        .traps
                        .get(trap_index)
                        .ok_or_else(|| internal("template text has no allocation trap"))?;
                    trap_index += 1;
                    self.consume(trap);
                    let pos = self.emitter.pos_id(&trap.pos);
                    self.emitter.runtime_call(
                        "void*",
                        "subscript_rt_str_lit",
                        &[
                            "void*".into(),
                            "const unsigned char*".into(),
                            "uint64_t".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            format!(
                                "(const unsigned char*){}",
                                c_string_literal(text.as_bytes())
                            ),
                            format!("{}ull", text.len()),
                            format!("{pos}u"),
                        ],
                    )
                }
                l::TemplatePart::Operand(index) => {
                    let index = *index as usize;
                    let ty = data_type(&operand_types[index])?;
                    if *ty == Type::Str {
                        operands[index].clone()
                    } else {
                        let trap = instruction
                            .traps
                            .get(trap_index)
                            .ok_or_else(|| internal("template format has no allocation trap"))?;
                        trap_index += 1;
                        self.consume(trap);
                        self.format_value(ty, &operands[index], trap)?
                    }
                }
            };
            let piece_name = self.fresh();
            let _ = writeln!(out, "    void* {piece_name} = {piece};");
            self.emit_pending_check(out);
            accumulated = Some(if let Some(previous) = accumulated {
                let trap = instruction
                    .traps
                    .get(trap_index)
                    .ok_or_else(|| internal("template concat has no allocation trap"))?;
                trap_index += 1;
                self.consume(trap);
                let pos = self.emitter.pos_id(&trap.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_str_concat",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &["ctx".into(), previous, piece_name, format!("{pos}u")],
                );
                let concat = self.fresh();
                let _ = writeln!(out, "    void* {concat} = {call};");
                self.emit_pending_check(out);
                concat
            } else {
                piece_name
            });
        }
        if trap_index != instruction.traps.len() {
            return Err(internal(format!(
                "template consumed {trap_index} of {} traps",
                instruction.traps.len()
            )));
        }
        self.assign(
            out,
            Some(destination),
            accumulated.as_deref().unwrap_or("NULL"),
        )
    }

    fn format_value(&mut self, ty: &Type, value: &str, trap: &l::Trap) -> Result<String, String> {
        let pos = self.emitter.pos_id(&trap.pos);
        if let Type::StringAlias(alias) = ty {
            let definition = self
                .emitter
                .module
                .string_aliases
                .get(alias.0)
                .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
            let index = if let Some(wires) = &definition.wire_values {
                let expression = wires
                    .iter()
                    .enumerate()
                    .rev()
                    .fold("0".to_string(), |else_, (index, wire)| {
                        format!("(({value}) == {wire} ? {index} : ({else_}))")
                    });
                expression
            } else {
                format!("(uint32_t)({value})")
            };
            return Ok(self.emitter.runtime_call(
                "void*",
                "subscript_rt_str_lit",
                &[
                    "void*".into(),
                    "const unsigned char*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    format!("sub_alias_{}[{index}].data", alias.0),
                    format!("sub_alias_{}[{index}].len", alias.0),
                    format!("{pos}u"),
                ],
            ));
        }
        let (name, ctype, argument) = match ty {
            Type::I8 | Type::I16 | Type::I32 | Type::Enum(_) => (
                "subscript_rt_fmt_i32",
                "int32_t",
                format!("(int32_t)({value})"),
            ),
            Type::U8 | Type::U16 | Type::U32 => (
                "subscript_rt_fmt_u32",
                "uint32_t",
                format!("(uint32_t)({value})"),
            ),
            Type::I64 | Type::Date => ("subscript_rt_fmt_i64", "int64_t", value.into()),
            Type::U64 => ("subscript_rt_fmt_u64", "uint64_t", value.into()),
            Type::F32 => ("subscript_rt_fmt_f32", "float", value.into()),
            Type::F64 => ("subscript_rt_fmt_f64", "double", value.into()),
            Type::F16 => {
                let wide = self.emitter.runtime_call(
                    "double",
                    "subscript_rt_f16_to_f64",
                    &["uint16_t".into()],
                    &[value.into()],
                );
                ("subscript_rt_fmt_f64", "double", wide)
            }
            Type::Bool => (
                "subscript_rt_fmt_bool",
                "uint32_t",
                format!("(uint32_t)({value})"),
            ),
            other => return Err(internal(format!("cannot format {other:?}"))),
        };
        Ok(self.emitter.runtime_call(
            "void*",
            name,
            &["void*".into(), ctype.into(), "uint32_t".into()],
            &["ctx".into(), argument, format!("{pos}u")],
        ))
    }

    fn emit_closure(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        function: l::FunctionId,
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("closure has no result"))?;
        let target = self.emitter.function(function)?;
        let captures = capture_parameters(target)
            .map(|parameter| parameter.value)
            .collect::<Vec<_>>();
        if captures.is_empty() {
            let _ = writeln!(
                out,
                "    {destination} = (SubFn){{ (void*)&sub_f{}, NULL }};",
                function.0
            );
            return Ok(());
        }
        let environment = if self.coroutine {
            let result_id = instruction
                .result
                .ok_or_else(|| internal("closure has no id"))?;
            format!("&frame->env_v{}", result_id.0)
        } else {
            let temporary = self.fresh();
            let _ = writeln!(out, "    SubEnv{} {temporary} = {{0}};", function.0);
            format!("&{temporary}")
        };
        for ((capture, operand), _) in captures.iter().zip(operands).zip(0..) {
            let _ = writeln!(out, "    ({environment})->c{} = {operand};", capture.0);
        }
        let _ = writeln!(
            out,
            "    {destination} = (SubFn){{ (void*)&sub_f{}, {environment} }};",
            function.0
        );
        Ok(())
    }

    fn emit_iterator_create(
        &mut self,
        out: &mut String,
        kind: l::ForOfKind,
        subject: &str,
        subject_type: &l::ValueType,
        result: Option<String>,
    ) -> Result<(), String> {
        let (subject, bound) = match (kind, subject_type) {
            (l::ForOfKind::FixedArrayValues, l::ValueType::Data(Type::FixedArray(_, count))) => {
                (format!("(void*)&({subject})"), format!("{count}ull"))
            }
            (l::ForOfKind::FixedArrayValues, other) => {
                return Err(internal(format!(
                    "fixed-array iterator source is {other:?}"
                )))
            }
            _ => (format!("(void*)({subject})"), "0ull".into()),
        };
        self.assign(
            out,
            result,
            &format!("(SubIter){{ {subject}, 0ull, {bound} }}"),
        )
    }

    fn iterator_type<'a>(
        &self,
        operand_type: &'a l::ValueType,
    ) -> Result<&'a l::IteratorType, String> {
        match operand_type {
            l::ValueType::Iterator(iterator) => Ok(iterator),
            other => Err(internal(format!("iterator operand has type {other:?}"))),
        }
    }

    fn emit_iterator_bound(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        iterator: &str,
        iterator_type: &l::ValueType,
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator_type = self.iterator_type(iterator_type)?;
        let expression = match iterator_type.kind {
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys => self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_array_len",
                &["void*".into(), "const void*".into()],
                &["ctx".into(), format!("({iterator}).subject")],
            ),
            l::ForOfKind::FixedArrayValues => {
                format!("({iterator}).bound")
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let pos = self.emitter.pos_id(&instruction.pos);
                self.emitter.runtime_call(
                    "uint64_t",
                    "subscript_rt_assoc_iter_begin",
                    &["void*".into(), "void*".into(), "uint32_t".into()],
                    &[
                        "ctx".into(),
                        format!("({iterator}).subject"),
                        format!("{pos}u"),
                    ],
                )
            }
            l::ForOfKind::StringCodePoints => self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_str_len",
                &["void*".into(), "const void*".into()],
                &["ctx".into(), format!("({iterator}).subject")],
            ),
        };
        self.assign(out, result, &format!("(int32_t)({expression})"))
    }

    fn emit_iterator_has_next(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator = self.iterator_type(&operand_types[0])?.clone();
        let destination = result.ok_or_else(|| internal("iterator condition has no result"))?;
        match iterator.kind {
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys => {
                let length = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_array_len",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), format!("({}).subject", operands[0])],
                );
                let _ = writeln!(out, "    {destination} = ((uint32_t)({}) < (uint32_t)({}) && (uint32_t)({}) < (uint32_t)({length}));", operands[1], operands[2], operands[1]);
            }
            l::ForOfKind::FixedArrayValues | l::ForOfKind::StringCodePoints => {
                let _ = writeln!(
                    out,
                    "    {destination} = ((uint64_t)({}).position < (uint32_t)({}));",
                    operands[0], operands[2]
                );
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let candidate = self.fresh();
                let active = self.fresh();
                let temporary = self.fresh();
                let select = i32::from(iterator.kind == l::ForOfKind::MapValues);
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_assoc_iter_copy",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint64_t".into(),
                        "uint32_t".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("({}).subject", operands[0]),
                        candidate.clone(),
                        format!("{select}u"),
                        format!("&{temporary}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(
                    out,
                    "    {} {temporary} = {{0}};",
                    self.emitter.ctype(&iterator.element)?
                );
                let _ = writeln!(
                    out,
                    "    uint64_t {candidate} = (uint64_t)({}.position);",
                    operands[0]
                );
                let _ = writeln!(out, "    int32_t {active} = 0;");
                let _ = writeln!(out, "    while ({candidate} < (uint32_t)({})) {{ {active} = {call}; if ({active}) break; {candidate}++; }}", operands[2]);
                let _ = writeln!(out, "    {}.position = {candidate};", operands[0]);
                let _ = writeln!(out, "    {destination} = {active};");
            }
        }
        Ok(())
    }

    fn emit_iterator_value(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator = self.iterator_type(&operand_types[0])?.clone();
        let destination = result.ok_or_else(|| internal("iterator value has no result"))?;
        match iterator.kind {
            l::ForOfKind::ArrayKeys => {
                let _ = writeln!(out, "    {destination} = (int32_t)({});", operands[1]);
            }
            l::ForOfKind::ArrayValues => {
                let data = self.emitter.runtime_call(
                    "const void*",
                    "subscript_rt_array_data",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), format!("({}).subject", operands[0])],
                );
                let _ = writeln!(
                    out,
                    "    {destination} = (({}*)({data}))[{}];",
                    self.emitter.ctype(&iterator.element)?,
                    operands[1]
                );
            }
            l::ForOfKind::FixedArrayValues => {
                let _ = writeln!(
                    out,
                    "    {destination} = (({}*)({}.subject))[{}];",
                    self.emitter.ctype(&iterator.element)?,
                    operands[0],
                    operands[1]
                );
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let select = i32::from(iterator.kind == l::ForOfKind::MapValues);
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_assoc_iter_copy",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint64_t".into(),
                        "uint32_t".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("({}).subject", operands[0]),
                        format!("({}).position", operands[0]),
                        format!("{select}u"),
                        format!("&{destination}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    (void){call};");
                self.emit_pending_check(out);
            }
            l::ForOfKind::StringCodePoints => {
                let next = self.fresh();
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_str_iter_code_point",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "int32_t".into(),
                        "int32_t*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("({}).subject", operands[0]),
                        format!("(int32_t)({}.position)", operands[0]),
                        format!("&{next}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    int32_t {next} = 0;\n    {destination} = {call};\n    {}.position = (uint64_t){next};", operands[0]);
                self.emit_pending_check(out);
            }
        }
        Ok(())
    }

    fn emit_iterator_advance(
        &self,
        out: &mut String,
        _instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator = self.iterator_type(&operand_types[0])?;
        let destination = result.ok_or_else(|| internal("iterator advance has no result"))?;
        let next = match iterator.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::FixedArrayValues => format!("({}).position + 1ull", operands[0]),
            l::ForOfKind::StringCodePoints => format!("({}).position", operands[0]),
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                format!("({}).position + 1ull", operands[0])
            }
        };
        let _ = writeln!(
            out,
            "    {destination} = {};\n    {destination}.position = {next};",
            operands[0]
        );
        Ok(())
    }
}

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    fn emit_call(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if matches!(target.kind, l::CallTargetKind::Method(_)) {
            for trap in &instruction.traps {
                if trap.kind == l::TrapKind::DevOnlyLifetime {
                    self.consume(trap);
                }
            }
        }
        match &target.kind {
            l::CallTargetKind::Function(function) => {
                self.emit_script_call(out, *function, operands, result)?;
                self.consume_call_traps(out, &instruction.traps)
            }
            l::CallTargetKind::Method(method) => {
                let function = self.emitter.method_function(*method)?;
                self.emit_script_call(out, function, operands, result)?;
                self.consume_call_traps(out, &instruction.traps)
            }
            l::CallTargetKind::Indirect => {
                let callable = operands
                    .first()
                    .ok_or_else(|| internal("indirect call has no callable"))?;
                let l::ValueType::Data(Type::Func(signature)) = &operand_types[0] else {
                    return Err(internal("indirect call operand is not a function"));
                };
                let mut parameter_types = vec!["void*".to_string(), "void*".to_string()];
                parameter_types.extend(
                    signature
                        .params
                        .iter()
                        .map(|ty| self.emitter.ctype(ty))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                let args = operands.iter().skip(1).cloned().collect::<Vec<_>>();
                let separator = if args.is_empty() { "" } else { ", " };
                let expression = format!(
                    "(({} (*)({}))({callable}.code))(ctx, {callable}.env{separator}{})",
                    self.emitter.ctype(&signature.ret)?,
                    parameter_types.join(", "),
                    args.join(", ")
                );
                if let Some(result) = result {
                    let _ = writeln!(out, "    {result} = {expression};");
                } else {
                    let _ = writeln!(out, "    {expression};");
                }
                self.consume_call_traps(out, &instruction.traps)
            }
            l::CallTargetKind::Foreign(function) => {
                self.emit_foreign_call(out, instruction, *function, operands, operand_types, result)
            }
            l::CallTargetKind::Intrinsic(intrinsic) => self.emit_intrinsic(
                out,
                instruction,
                target,
                intrinsic,
                operands,
                operand_types,
                result,
            ),
            l::CallTargetKind::BuiltinMethod(method) => self.emit_builtin_method(
                out,
                instruction,
                target,
                *method,
                operands,
                operand_types,
                result,
            ),
        }
    }

    fn emit_script_call(
        &self,
        out: &mut String,
        function: l::FunctionId,
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let separator = if operands.is_empty() { "" } else { ", " };
        let expression = format!("sub_f{}(ctx{separator}{})", function.0, operands.join(", "));
        if let Some(result) = result {
            let _ = writeln!(out, "    {result} = {expression};");
        } else {
            let _ = writeln!(out, "    {expression};");
        }
        Ok(())
    }

    fn consume_call_traps(&mut self, out: &mut String, traps: &[l::Trap]) -> Result<(), String> {
        let mut checked = false;
        for trap in traps {
            match trap.kind {
                l::TrapKind::Call | l::TrapKind::Allocation => {
                    self.consume(trap);
                    checked = true;
                }
                l::TrapKind::DevOnlyLifetime => {}
                ref other => return Err(internal(format!("script call carries trap {other:?}"))),
            }
        }
        if checked {
            self.emit_pending_check(out);
        }
        Ok(())
    }

    fn emit_foreign_call(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        function: l::ForeignFunctionId,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let declaration = self
            .emitter
            .module
            .foreign_functions
            .get(function.0 as usize)
            .filter(|declaration| declaration.id == function)
            .cloned()
            .ok_or_else(|| internal(format!("foreign function {} is missing", function.0)))?;
        if !self
            .emitter
            .foreign_symbols
            .contains(&declaration.source_name)
        {
            self.emitter
                .foreign_symbols
                .push(declaration.source_name.clone());
        }
        let needs_scratch = declaration.parameters.iter().any(|parameter| {
            boundary_type_requires_recursive_build(self.emitter.module, &parameter.ty)
        });
        let scratch_mark = if needs_scratch {
            let mark = self.fresh();
            let call = self.emitter.runtime_call(
                "uint64_t",
                "subscript_rt_boundary_scratch_mark",
                &["void*".into()],
                &["ctx".into()],
            );
            let _ = writeln!(out, "    uint64_t {mark} = {call};");
            Some(mark)
        } else {
            None
        };
        let boundary_position = self.emitter.pos_id(&instruction.pos);
        let mut arguments = Vec::new();
        let mut boundary_writebacks = Vec::new();
        let mut cursor = 0usize;
        for parameter in &declaration.parameters {
            if let Type::Array(element) = &parameter.ty {
                let data = operands.get(cursor).ok_or_else(|| {
                    internal(format!(
                        "foreign call `{}` array parameter `{}` has no data snapshot",
                        declaration.source_name, parameter.source_name
                    ))
                })?;
                let count = operands.get(cursor + 1).ok_or_else(|| {
                    internal(format!(
                        "foreign call `{}` array parameter `{}` has no count snapshot",
                        declaration.source_name, parameter.source_name
                    ))
                })?;
                let expected = l::ValueType::Address(l::AddressType {
                    pointee: element.as_ref().clone(),
                    array_base: None,
                });
                if operand_types.get(cursor) != Some(&expected)
                    || operand_types.get(cursor + 1) != Some(&l::ValueType::Data(Type::I32))
                {
                    return Err(internal(format!(
                        "foreign call `{}` array parameter `{}` snapshot types disagree with its declaration",
                        declaration.source_name, parameter.source_name
                    )));
                }
                let (data, count) = match element.as_ref() {
                    Type::Class(class)
                        if self.emitter.is_value_class(*class)?
                            && boundary_class_requires_recursive_build(
                                self.emitter.module,
                                *class,
                            ) =>
                    {
                        self.marshal_boundary_array(out, *class, data, count, boundary_position)?
                    }
                    _ => (data.clone(), count.clone()),
                };
                match parameter.foreign_provenance.as_ref() {
                    Some(l::ForeignTypeProvenance::Descriptor {
                        aggregate,
                        element,
                        element_const,
                    }) => {
                        let pointer = if *element_const {
                            format!("(const {element}*)({data})")
                        } else {
                            format!("({element}*)({data})")
                        };
                        arguments
                            .push(format!("(({aggregate}){{ {pointer}, (size_t)({count}) }})"));
                    }
                    Some(l::ForeignTypeProvenance::ScalarPair {
                        element,
                        element_const,
                    }) => {
                        arguments.push(format!("(size_t)({count})"));
                        arguments.push(if *element_const {
                            format!("(const {element}*)({data})")
                        } else {
                            format!("({element}*)({data})")
                        });
                    }
                    other => {
                        return Err(internal(format!(
                            "foreign call `{}` array parameter `{}` has provenance {other:?}",
                            declaration.source_name, parameter.source_name
                        )))
                    }
                }
                cursor += 2;
                continue;
            }
            let value = operands.get(cursor).ok_or_else(|| {
                internal(format!(
                    "foreign parameter `{}` has no operand",
                    parameter.source_name
                ))
            })?;
            if operand_types.get(cursor).and_then(|ty| data_type(ty).ok()) != Some(&parameter.ty) {
                return Err(internal(format!(
                    "foreign parameter `{}` operand type disagrees with its declaration",
                    parameter.source_name
                )));
            }
            arguments.push(
                self.marshal_foreign_value(
                    out,
                    &parameter.ty,
                    parameter.foreign_provenance.as_ref(),
                    value,
                    boundary_position,
                    &mut boundary_writebacks,
                )
                .map_err(|error| {
                    format!(
                        "{error}; foreign emission site `{}.{}`",
                        declaration.source_name, parameter.source_name
                    )
                })?,
            );
            cursor += 1;
        }
        if cursor != operands.len() {
            return Err(internal(format!(
                "foreign call `{}` has inconsistent arity",
                declaration.source_name
            )));
        }
        let call = format!("{}({})", declaration.source_name, arguments.join(", "));
        match &declaration.return_type {
            Type::Void => {
                let _ = writeln!(out, "    {call};");
            }
            Type::Class(class) if self.emitter.is_value_class(*class)? => {
                let destination = result
                    .clone()
                    .ok_or_else(|| internal("foreign struct return has no result"))?;
                let header = self.fresh();
                let _ = writeln!(out, "    {} {header} = {call};\n    memcpy(&{destination}, &{header}, sizeof {destination});", self.emitter.class(*class)?.source_name);
            }
            _ => self.assign(out, result.clone(), &call)?,
        }
        if !boundary_writebacks.is_empty() {
            out.push_str("    if (*(const uint32_t*)ctx == 0u) {\n");
            for writeback in boundary_writebacks {
                self.emit_boundary_writeback(out, writeback, boundary_position)?;
            }
            out.push_str("    }\n");
        }
        if let Some(mark) = scratch_mark {
            let release = self.emitter.runtime_call(
                "void",
                "subscript_rt_boundary_scratch_release",
                &["void*".into(), "uint64_t".into()],
                &["ctx".into(), mark],
            );
            let _ = writeln!(out, "    {release};");
        }
        let mut checked = false;
        for trap in &instruction.traps {
            match &trap.kind {
                l::TrapKind::Call | l::TrapKind::Allocation => {
                    self.consume(trap);
                    checked = true;
                }
                l::TrapKind::WireEnumValue(alias) => {
                    self.consume(trap);
                    let destination = result
                        .as_ref()
                        .ok_or_else(|| internal("wire-enum foreign return has no result"))?;
                    self.emit_wire_validation(out, destination, *alias, trap)?;
                }
                l::TrapKind::DevOnlyLifetime => self.consume(trap),
                other => return Err(internal(format!("foreign call carries trap {other:?}"))),
            }
        }
        if checked {
            self.emit_pending_check(out);
        }
        Ok(())
    }

    fn marshal_foreign_value(
        &mut self,
        out: &mut String,
        ty: &Type,
        provenance: Option<&l::ForeignTypeProvenance>,
        value: &str,
        position: u32,
        writebacks: &mut Vec<BoundaryPtrWriteback>,
    ) -> Result<String, String> {
        match ty {
            Type::Str => {
                let Some(l::ForeignTypeProvenance::StringView { aggregate }) = provenance else {
                    return Err(internal(
                        "foreign string parameter has no string-view provenance",
                    ));
                };
                let data = self.emitter.runtime_call(
                    "const void*",
                    "subscript_rt_str_data",
                    &["const void*".into(), "const void*".into()],
                    &["ctx".into(), value.into()],
                );
                let len = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_str_len",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), value.into()],
                );
                Ok(format!(
                    "(({aggregate}){{ (const char*)({data}), (size_t)({len}) }})"
                ))
            }
            Type::Class(class) if self.emitter.is_value_class(*class)? => {
                self.marshal_boundary_struct(out, *class, value, position)
            }
            Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(class) if self.emitter.is_value_class(*class).unwrap_or(false)) =>
            {
                let Type::Class(class) = inner.as_ref() else {
                    unreachable!()
                };
                let (argument, writeback) =
                    self.marshal_boundary_pointer(out, *class, value, position, false)?;
                if let Some(writeback) = writeback {
                    writebacks.push(writeback);
                }
                Ok(argument)
            }
            _ => Ok(value.into()),
        }
    }

    fn marshal_boundary_struct(
        &mut self,
        out: &mut String,
        class: ClassId,
        value: &str,
        position: u32,
    ) -> Result<String, String> {
        let definition = self.emitter.class(class)?.clone();
        let temporary = self.fresh();
        let _ = writeln!(
            out,
            "    {} {temporary} = {value};",
            self.emitter.class_name(class)
        );
        let mut parts = Vec::new();
        let mut index = 0usize;
        while index < definition.fields.len() {
            let field = &definition.fields[index];
            let access = format!("{temporary}.d{}", field.id.0);
            match &field.ty {
                Type::Func(_) => {
                    let Some(l::ForeignTypeProvenance::Callback { typedef_name }) =
                        field.foreign_provenance.as_ref()
                    else {
                        return Err(internal(format!(
                            "boundary callback field `{}.{}` has no typedef provenance",
                            definition.source_name, field.source_name
                        )));
                    };
                    let userdata = definition
                        .fields
                        .get(index + 1)
                        .ok_or_else(|| internal("boundary callback has no userdata field"))?;
                    let second = definition
                        .fields
                        .get(index + 2)
                        .filter(|field| is_userdata_slot(&field.ty));
                    let bind = self.emitter.runtime_call(
                        "void*",
                        "subscript_rt_cb_bind",
                        &[
                            "void*".into(),
                            "const void*".into(),
                            "const void*".into(),
                            "void*".into(),
                            "void*".into(),
                        ],
                        &[
                            "ctx".into(),
                            format!("{access}.code"),
                            format!("{access}.env"),
                            format!("{temporary}.d{}", userdata.id.0),
                            second.map_or_else(
                                || "NULL".into(),
                                |field| format!("{temporary}.d{}", field.id.0),
                            ),
                        ],
                    );
                    parts.push(format!("({typedef_name})&subscript_rt_cb_trampoline"));
                    parts.push(bind);
                    if second.is_some() {
                        parts.push("NULL".into());
                        index += 3;
                    } else {
                        index += 2;
                    }
                }
                Type::Array(element) => {
                    let count_call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_len",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), access.clone()],
                    );
                    let data_call = self.emitter.runtime_call(
                        "const void*",
                        "subscript_rt_array_data",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), access],
                    );
                    let (data, count) = match element.as_ref() {
                        Type::Class(element_class)
                            if self.emitter.is_value_class(*element_class)?
                                && boundary_class_requires_recursive_build(
                                    self.emitter.module,
                                    *element_class,
                                ) =>
                        {
                            self.marshal_boundary_array(
                                out,
                                *element_class,
                                &data_call,
                                &count_call,
                                position,
                            )?
                        }
                        _ => (data_call, count_call),
                    };
                    parts.push(format!("(size_t)({count})"));
                    parts.push(format!("(void*)({data})"));
                    index += 1;
                }
                Type::Str => {
                    let data = self.emitter.runtime_call(
                        "const void*",
                        "subscript_rt_str_data",
                        &["const void*".into(), "const void*".into()],
                        &["ctx".into(), access.clone()],
                    );
                    let len = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_str_len",
                        &["void*".into(), "const void*".into()],
                        &["ctx".into(), access],
                    );
                    parts.push(format!("{{ (const char*)({data}), (size_t)({len}) }}"));
                    index += 1;
                }
                Type::Class(nested) if self.emitter.is_value_class(*nested)? => {
                    parts.push(self.marshal_boundary_struct(out, *nested, &access, position)?);
                    index += 1;
                }
                Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(nested) if self.emitter.is_value_class(*nested).unwrap_or(false)) =>
                {
                    let Type::Class(nested) = inner.as_ref() else {
                        unreachable!()
                    };
                    parts.push(
                        self.marshal_boundary_pointer(out, *nested, &access, position, true)?
                            .0,
                    );
                    index += 1;
                }
                _ => {
                    parts.push(access);
                    index += 1;
                }
            }
        }
        Ok(format!(
            "(({}){{ {} }})",
            definition.source_name,
            parts.join(", ")
        ))
    }

    fn marshal_boundary_pointer(
        &mut self,
        out: &mut String,
        class: ClassId,
        pointer: &str,
        position: u32,
        force_rebuild: bool,
    ) -> Result<(String, Option<BoundaryPtrWriteback>), String> {
        if !boundary_class_needs_scratch(self.emitter.module, class, &mut HashSet::new())
            && !(force_rebuild
                && boundary_class_requires_recursive_build(self.emitter.module, class))
        {
            return Ok((
                format!("(({}*)({pointer}))", self.emitter.class(class)?.source_name),
                None,
            ));
        }
        let source = self.fresh();
        let language_type = self.emitter.class_name(class);
        let _ = writeln!(
            out,
            "    {language_type}* {source} = ({language_type}*)({pointer});"
        );
        let header = self.fresh();
        let header_type = self.emitter.class(class)?.source_name.clone();
        let _ = writeln!(
            out,
            "    {header_type}* {header} = NULL;\n    if ({source} != NULL) {{"
        );
        let allocation = self.emitter.runtime_call(
            "void*",
            "subscript_rt_boundary_scratch_alloc",
            &["void*".into(), "uint64_t".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({header_type})"),
                format!("{position}u"),
            ],
        );
        let _ = writeln!(
            out,
            "        {header} = ({header_type}*){allocation};\n        if (*(const uint32_t*)ctx != 0u) goto unwind;"
        );
        let value = self.marshal_boundary_struct(out, class, &format!("*{source}"), position)?;
        let _ = writeln!(out, "        *{header} = {value};\n    }}");
        Ok((
            header.clone(),
            Some(BoundaryPtrWriteback {
                class,
                source,
                scratch: header,
            }),
        ))
    }

    fn marshal_boundary_array(
        &mut self,
        out: &mut String,
        element_class: ClassId,
        source: &str,
        count: &str,
        position: u32,
    ) -> Result<(String, String), String> {
        let language_type = self.emitter.class_name(element_class);
        let header_type = self.emitter.class(element_class)?.source_name.clone();
        let source_temporary = self.fresh();
        let count_temporary = self.fresh();
        let scratch = self.fresh();
        let index = self.fresh();
        let _ = writeln!(
            out,
            "    size_t {count_temporary} = (size_t)({count});\n    const {language_type}* {source_temporary} = (const {language_type}*)({source});"
        );
        let allocation = self.emitter.runtime_call(
            "void*",
            "subscript_rt_boundary_scratch_alloc",
            &["void*".into(), "uint64_t".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("(uint64_t)({count_temporary} * sizeof({header_type}))"),
                format!("{position}u"),
            ],
        );
        let _ = writeln!(
            out,
            "    {header_type}* {scratch} = ({header_type}*){allocation};\n    if (*(const uint32_t*)ctx != 0u) goto unwind;\n    for (size_t {index} = 0; {index} < {count_temporary}; {index}++) {{"
        );
        let value = self.marshal_boundary_struct(
            out,
            element_class,
            &format!("{source_temporary}[{index}]"),
            position,
        )?;
        let _ = writeln!(out, "        {scratch}[{index}] = {value};\n    }}");
        Ok((scratch, count_temporary))
    }

    fn emit_boundary_writeback(
        &mut self,
        out: &mut String,
        writeback: BoundaryPtrWriteback,
        position: u32,
    ) -> Result<(), String> {
        let class = self.emitter.class(writeback.class)?.clone();
        let _ = writeln!(out, "    if ({} != NULL) {{", writeback.source);
        for field in &class.fields {
            let language = format!("{}->d{}", writeback.source, field.id.0);
            let header = format!("{}->{}", writeback.scratch, field.source_name);
            match &field.ty {
                Type::Array(_) | Type::Func(_) => {}
                Type::Nullable(inner) if matches!(inner.as_ref(), Type::Class(class) if self.emitter.is_value_class(*class)?) =>
                    {}
                Type::Str => {
                    let view = self.fresh();
                    let _ = writeln!(out, "        subscript_callback_string_view {view}; memcpy(&{view}, &{header}, sizeof {view});");
                    let value = self.emitter.runtime_call(
                        "void*",
                        "subscript_rt_str_from_view",
                        &[
                            "void*".into(),
                            "const unsigned char*".into(),
                            "uint64_t".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            format!("{view}.data"),
                            format!("(uint64_t){view}.len"),
                            format!("{position}u"),
                        ],
                    );
                    let _ = writeln!(out, "        {language} = {value};");
                }
                Type::Class(nested) if self.emitter.is_value_class(*nested)? => {
                    if !boundary_class_needs_scratch(
                        self.emitter.module,
                        *nested,
                        &mut HashSet::new(),
                    ) {
                        let _ = writeln!(
                            out,
                            "        memcpy(&{language}, &{header}, sizeof {language});"
                        );
                    }
                }
                _ => {
                    let _ = writeln!(out, "        {language} = {header};");
                }
            }
        }
        out.push_str("    }\n");
        Ok(())
    }

    fn emit_builtin_method(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        method: l::BuiltinMethod,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        match method {
            l::BuiltinMethod::ArrayPush => {
                let l::ValueType::Data(Type::Array(element)) = &operand_types[0] else {
                    return Err(internal("array push receiver is not an array"));
                };
                let pointer = self.materialize(out, &operands[1], element)?;
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_array_push",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        operands[0].clone(),
                        pointer,
                        format!("{pos}u"),
                    ],
                );
                self.assign(out, result, &call)?;
                self.consume_runtime_traps(out, &instruction.traps, true)
            }
            l::BuiltinMethod::ArrayPop => {
                let l::ValueType::Data(Type::Array(element)) = &operand_types[0] else {
                    return Err(internal("array pop receiver is not an array"));
                };
                let destination = result.ok_or_else(|| internal("array pop has no result"))?;
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_array_pop",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        operands[0].clone(),
                        format!("&{destination}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = element;
                let _ = writeln!(out, "    {call};");
                self.consume_runtime_traps(out, &instruction.traps, true)
            }
            l::BuiltinMethod::StringSlice => self.emit_simple_runtime_intrinsic(
                out,
                instruction,
                target,
                "subscript_rt_str_slice",
                operands,
                operand_types,
                true,
                result,
            ),
            l::BuiltinMethod::GeneratorNext => {
                let destination = result.ok_or_else(|| internal("Generator.next has no result"))?;
                let l::ValueType::Data(Type::IterResult(_)) = target
                    .return_type
                    .as_ref()
                    .ok_or_else(|| internal("Generator.next has no result type"))?
                else {
                    return Err(internal("Generator.next result is not IterResult"));
                };
                let _ = writeln!(
                    out,
                    "    {destination} = ({}){{0}};",
                    self.emitter
                        .value_ctype(target.return_type.as_ref().unwrap())?
                );
                let _ = writeln!(out, "    {destination}.done = ((SubCoroutinePrefix*)({}))->resume(ctx, {}, &{destination}.value);", operands[0], operands[0]);
                self.consume_runtime_traps(out, &instruction.traps, true)
            }
        }
    }

    fn consume_runtime_traps(
        &mut self,
        out: &mut String,
        traps: &[l::Trap],
        pending: bool,
    ) -> Result<(), String> {
        let mut check = false;
        for trap in traps {
            match trap.kind {
                l::TrapKind::DevOnlyLifetime | l::TrapKind::DevReloadOnlyStaleCoroutine => {
                    self.consume(trap)
                }
                l::TrapKind::Allocation | l::TrapKind::Call => {
                    self.consume(trap);
                    check = true;
                }
                ref other => return Err(internal(format!("runtime call carries trap {other:?}"))),
            }
        }
        if pending && check {
            self.emit_pending_check(out);
        }
        Ok(())
    }

    fn emit_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        intrinsic: &l::Intrinsic,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let name = self.emitter.operation_name(intrinsic)?.to_string();
        match intrinsic.family {
            l::IntrinsicFamily::Ambient => match name.as_str() {
                "Print" => self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    "subscript_rt_print",
                    operands,
                    operand_types,
                    false,
                    result,
                ),
                "Collect" => self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    "subscript_rt_collect",
                    operands,
                    operand_types,
                    false,
                    result,
                ),
                "UnsafeDelete" => self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    "subscript_rt_delete",
                    operands,
                    operand_types,
                    true,
                    result,
                ),
                "Unreachable" => {
                    let trap = instruction
                        .traps
                        .iter()
                        .find(|trap| trap.kind == l::TrapKind::Unreachable)
                        .ok_or_else(|| internal("Unreachable has no trap"))?
                        .clone();
                    self.consume(&trap);
                    let pos = self.emitter.pos_id(&trap.pos);
                    let call = self.emitter.runtime_call(
                        "void",
                        "subscript_rt_trap",
                        &["void*".into(), "uint32_t".into(), "uint32_t".into()],
                        &[
                            "ctx".into(),
                            format!("{}u", TrapKind::UnreachableReached as u32),
                            format!("{pos}u"),
                        ],
                    );
                    let _ = writeln!(out, "    {call};\n    goto unwind;");
                    Ok(())
                }
                other => Err(internal(format!("unknown Ambient intrinsic {other}"))),
            },
            l::IntrinsicFamily::Math => {
                let symbol = math_symbol(&name)?;
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    false,
                    result,
                )
            }
            l::IntrinsicFamily::Number => {
                let symbol = number_symbol(&name)?;
                let position = !matches!(
                    name.as_str(),
                    "IsNaN" | "IsFinite" | "IsInteger" | "IsSafeInteger"
                );
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    position,
                    result,
                )
            }
            l::IntrinsicFamily::Date => self.emit_date_intrinsic(
                out,
                instruction,
                target,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Json => {
                let symbol = json_symbol(&name)?;
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    true,
                    result,
                )
            }
            l::IntrinsicFamily::String => {
                let symbol = string_symbol(&name)?;
                let position = !matches!(
                    name.as_str(),
                    "IndexOf" | "LastIndexOf" | "Includes" | "StartsWith" | "EndsWith"
                );
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    position,
                    result,
                )
            }
            l::IntrinsicFamily::Regex => {
                let symbol = regex_symbol(&name)?;
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    true,
                    result,
                )
            }
            l::IntrinsicFamily::Array => self.emit_array_intrinsic(
                out,
                instruction,
                target,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Map => self.emit_map_intrinsic(
                out,
                instruction,
                target,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Set => self.emit_set_intrinsic(
                out,
                instruction,
                target,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::ContextBytes => self.emit_context_bytes(
                out,
                instruction,
                target,
                intrinsic,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Worker => self.emit_worker_intrinsic(
                out,
                instruction,
                target,
                intrinsic,
                &name,
                operands,
                operand_types,
                result,
            ),
        }
    }

    fn emit_simple_runtime_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        symbol: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        position: bool,
        result: Option<String>,
    ) -> Result<(), String> {
        let return_type = target
            .return_type
            .as_ref()
            .map(|ty| self.emitter.value_ctype(ty))
            .transpose()?
            .unwrap_or_else(|| "void".into());
        let mut argument_types = vec!["void*".to_string()];
        argument_types.extend(
            operand_types
                .iter()
                .map(|ty| self.emitter.value_ctype(ty))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let mut arguments = vec!["ctx".to_string()];
        arguments.extend_from_slice(operands);
        if position {
            argument_types.push("uint32_t".into());
            arguments.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
        }
        let call = self
            .emitter
            .runtime_call(&return_type, symbol, &argument_types, &arguments);
        if let Some(result) = result {
            let _ = writeln!(out, "    {result} = {call};");
        } else {
            let _ = writeln!(out, "    {call};");
        }
        self.consume_runtime_traps(out, &instruction.traps, true)
    }

    fn emit_date_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if let Some(field) = match name {
            "GetUtcFullYear" => Some(0),
            "GetUtcMonth" => Some(1),
            "GetUtcDate" => Some(2),
            "GetUtcDay" => Some(3),
            "GetUtcHours" => Some(4),
            "GetUtcMinutes" => Some(5),
            "GetUtcSeconds" => Some(6),
            "GetUtcMilliseconds" => Some(7),
            _ => None,
        } {
            let destination = result.ok_or_else(|| internal("Date getter has no result"))?;
            let call = self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_date_get",
                &["void*".into(), "int64_t".into(), "uint32_t".into()],
                &["ctx".into(), operands[0].clone(), format!("{field}u")],
            );
            let _ = writeln!(out, "    {destination} = {call};");
            return self.consume_runtime_traps(out, &instruction.traps, true);
        }
        let (symbol, position) = match name {
            "New" => ("subscript_rt_date_new", true),
            "Utc" => ("subscript_rt_date_utc", true),
            "Now" => ("subscript_rt_date_now", false),
            "ToIso" => ("subscript_rt_date_to_iso", true),
            other => return Err(internal(format!("unknown Date intrinsic {other}"))),
        };
        self.emit_simple_runtime_intrinsic(
            out,
            instruction,
            target,
            symbol,
            operands,
            operand_types,
            position,
            result,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_array_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let receiver_type = operand_types
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver type")))?;
        let (element, fixed_count) = match receiver_type {
            l::ValueType::Data(Type::Array(element)) => (element.as_ref(), None),
            l::ValueType::Data(Type::FixedArray(element, count)) => {
                (element.as_ref(), Some(*count))
            }
            other => return Err(internal(format!("Array.{name} receiver is {other:?}"))),
        };
        let receiver = operands
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver")))?;
        let receiver = if fixed_count.is_some() {
            format!("(const void*)({receiver}).a")
        } else {
            receiver.clone()
        };
        let symbol = array_symbol(name, fixed_count.is_some())?;
        let result_type = target.return_type.as_ref().map(data_type).transpose()?;
        let return_ctype = target
            .return_type
            .as_ref()
            .map(|ty| self.emitter.value_ctype(ty))
            .transpose()?
            .unwrap_or_else(|| "void".into());
        let argument = |index: usize| {
            operands
                .get(index)
                .cloned()
                .ok_or_else(|| internal(format!("Array.{name} operand {index} is missing")))
        };
        let indexed = || -> Result<u32, String> {
            let expected = match name {
                "ForEach" | "Map" | "Filter" | "Some" | "Every" | "FindIndex" => 2,
                "Reduce" | "ReduceRight" => 3,
                other => return Err(internal(format!("Array.{other} has no indexed callback"))),
            };
            let Some(l::ValueType::Data(Type::Func(function))) = operand_types.get(1) else {
                return Err(internal(format!("Array.{name} callback type is missing")));
            };
            match function.params.len() {
                arity if arity + 1 == expected => Ok(0),
                arity if arity == expected => Ok(1),
                arity => Err(internal(format!(
                    "Array.{name} callback arity {arity} escaped the checker"
                ))),
            }
        };
        let call = match name {
            "IndexOf" | "LastIndexOf" | "Includes" => {
                let pointer = self.materialize(out, &argument(1)?, element)?;
                self.emitter.runtime_call(
                    &return_ctype,
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver,
                        pointer,
                        format!("{}u", array_element_kind(self.emitter.module, element)?),
                    ],
                )
            }
            "Join" => {
                let position = self.emitter.pos_id(&instruction.pos);
                self.emitter.runtime_call(
                    &return_ctype,
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver,
                        argument(1)?,
                        format!("{}u", array_format_kind(element)?),
                        format!("{position}u"),
                    ],
                )
            }
            "Slice" | "Concat" | "Splice" | "Unshift" => {
                let mut types = vec!["void*".into(), "void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                match name {
                    "Slice" | "Splice" => {
                        types.extend(["int32_t".into(), "int32_t".into()]);
                        args.extend([argument(1)?, argument(2)?]);
                    }
                    "Concat" => {
                        types.push("void*".into());
                        args.push(argument(1)?);
                    }
                    "Unshift" => {
                        types.push("const void*".into());
                        args.push(self.materialize(out, &argument(1)?, element)?);
                    }
                    _ => unreachable!(),
                }
                types.push("uint32_t".into());
                args.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
                self.emitter
                    .runtime_call(&return_ctype, symbol, &types, &args)
            }
            "Fill" => {
                let pointer = self.materialize(out, &argument(1)?, element)?;
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "int32_t".into(),
                        "int32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        pointer,
                        argument(2)?,
                        argument(3)?,
                    ],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true);
            }
            "Reverse" => {
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true);
            }
            "Shift" => {
                let destination = result.ok_or_else(|| internal("Array.Shift has no result"))?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver,
                        format!("&{destination}"),
                        format!("{position}u"),
                    ],
                );
                let _ = writeln!(out, "    {call};");
                return self.consume_runtime_traps(out, &instruction.traps, true);
            }
            "CopyWithin" => {
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "int32_t".into(),
                        "int32_t".into(),
                        "int32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        argument(1)?,
                        argument(2)?,
                        argument(3)?,
                    ],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true);
            }
            "ForEach" | "Filter" | "Some" | "Every" | "FindIndex" => {
                let callback = argument(1)?;
                let mut types = vec!["void*".into(), "const void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                if let Some(count) = fixed_count {
                    types.extend(["uint64_t".into(), "uint64_t".into()]);
                    args.extend([
                        format!("{count}ull"),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(element)?),
                    ]);
                }
                types.extend([
                    "const void*".into(),
                    "const void*".into(),
                    "uint32_t".into(),
                ]);
                args.extend([
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("{}u", array_element_kind(self.emitter.module, element)?),
                ]);
                if name == "Filter" {
                    types.push("uint32_t".into());
                    args.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
                }
                types.push("uint32_t".into());
                args.push(format!("{}u", indexed()?));
                self.emitter
                    .runtime_call(&return_ctype, symbol, &types, &args)
            }
            "Sort" => {
                let callback = argument(1)?;
                let call = self.emitter.runtime_call(
                    "void",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        format!("{callback}.code"),
                        format!("{callback}.env"),
                        format!("{}u", array_element_kind(self.emitter.module, element)?),
                    ],
                );
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &receiver)?;
                return self.consume_runtime_traps(out, &instruction.traps, true);
            }
            "Map" => {
                let callback = argument(1)?;
                let Type::Array(result_element) =
                    result_type.ok_or_else(|| internal("Array.Map result type is missing"))?
                else {
                    return Err(internal("Array.Map result is not an array"));
                };
                let mut types = vec!["void*".into(), "const void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                if let Some(count) = fixed_count {
                    types.extend(["uint64_t".into(), "uint64_t".into()]);
                    args.extend([
                        format!("{count}ull"),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(element)?),
                    ]);
                }
                types.extend([
                    "const void*".into(),
                    "const void*".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ]);
                args.extend([
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("{}u", array_element_kind(self.emitter.module, element)?),
                    format!(
                        "{}u",
                        array_element_kind(self.emitter.module, result_element)?
                    ),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(result_element)?),
                    format!("{}u", self.emitter.pos_id(&instruction.pos)),
                    format!("{}u", indexed()?),
                ]);
                self.emitter
                    .runtime_call(&return_ctype, symbol, &types, &args)
            }
            "Reduce" | "ReduceRight" => {
                let callback = argument(1)?;
                let accumulator =
                    result_type.ok_or_else(|| internal("Array.Reduce result type is missing"))?;
                let temporary = self.fresh();
                let _ = writeln!(
                    out,
                    "    {} {temporary} = {};",
                    self.emitter.ctype(accumulator)?,
                    argument(2)?
                );
                let mut types = vec!["void*".into(), "const void*".into()];
                let mut args = vec!["ctx".into(), receiver];
                if let Some(count) = fixed_count {
                    types.extend(["uint64_t".into(), "uint64_t".into()]);
                    args.extend([
                        format!("{count}ull"),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(element)?),
                    ]);
                }
                types.extend([
                    "const void*".into(),
                    "const void*".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                    "uint64_t".into(),
                    "void*".into(),
                    "uint32_t".into(),
                ]);
                args.extend([
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("{}u", array_element_kind(self.emitter.module, element)?),
                    format!("{}u", array_element_kind(self.emitter.module, accumulator)?),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(accumulator)?),
                    format!("&{temporary}"),
                    format!("{}u", indexed()?),
                ]);
                let call = self.emitter.runtime_call("void", symbol, &types, &args);
                let _ = writeln!(out, "    {call};");
                self.assign(out, result, &temporary)?;
                return self.consume_runtime_traps(out, &instruction.traps, true);
            }
            other => return Err(internal(format!("unknown Array intrinsic {other}"))),
        };
        if let Some(result) = result {
            let _ = writeln!(out, "    {result} = {call};");
        } else {
            let _ = writeln!(out, "    {call};");
        }
        self.consume_runtime_traps(out, &instruction.traps, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_map_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if name == "GroupBy" {
            let Some(l::ValueType::Data(Type::Array(element))) = operand_types.first() else {
                return Err(internal("Map.GroupBy items type is not an array"));
            };
            let Some(l::ValueType::Data(Type::Map(key, value))) = target.return_type.as_ref()
            else {
                return Err(internal("Map.GroupBy result is not a map"));
            };
            if !matches!(value.as_ref(), Type::Array(group_element) if group_element == element) {
                return Err(internal(
                    "Map.GroupBy result value is not the source element array",
                ));
            }
            let callback = operands
                .get(1)
                .ok_or_else(|| internal("Map.GroupBy callback is missing"))?;
            let bridge = self.emitter.define_group_bridge(element, key)?;
            let position = self.emitter.pos_id(&instruction.pos);
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_map_group_by",
                &[
                    "void*".into(),
                    "void*".into(),
                    "const void*".into(),
                    "const void*".into(),
                    "const void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    operands[0].clone(),
                    format!("{callback}.code"),
                    format!("{callback}.env"),
                    format!("(const void*)&{bridge}"),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(key)?),
                    format!("{}u", association_key_kind(self.emitter.module, key)?),
                    format!("{position}u"),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true);
        }
        let (key, value) = if name == "New" {
            let Some(l::ValueType::Data(Type::Map(key, value))) = target.return_type.as_ref()
            else {
                return Err(internal("Map.New result is not a map"));
            };
            (key.as_ref(), value.as_ref())
        } else {
            let Some(l::ValueType::Data(Type::Map(key, value))) = operand_types.first() else {
                return Err(internal(format!("Map.{name} receiver is not a map")));
            };
            (key.as_ref(), value.as_ref())
        };
        if name == "New" {
            let position = self.emitter.pos_id(&instruction.pos);
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_map_new",
                &[
                    "void*".into(),
                    "uint64_t".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(key)?),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(value)?),
                    format!("{}u", association_key_kind(self.emitter.module, key)?),
                    format!("{position}u"),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true);
        }
        let receiver = operands
            .first()
            .ok_or_else(|| internal(format!("Map.{name} receiver is missing")))?;
        let argument = |index: usize| {
            operands
                .get(index)
                .cloned()
                .ok_or_else(|| internal(format!("Map.{name} operand {index} is missing")))
        };
        match name {
            "Size" => {
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_map_size",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                self.assign(out, result, &call)?;
            }
            "Get" | "GetOr" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let destination =
                    result.ok_or_else(|| internal(format!("Map.{name} has no result")))?;
                if name == "Get" {
                    let call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_map_get",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "void*".into(),
                        ],
                        &[
                            "ctx".into(),
                            receiver.clone(),
                            key_pointer,
                            format!("&{destination}"),
                        ],
                    );
                    let _ = writeln!(out, "    (void){call};");
                } else {
                    let fallback = self.materialize(out, &argument(2)?, value)?;
                    let call = self.emitter.runtime_call(
                        "void",
                        "subscript_rt_map_get_or",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "const void*".into(),
                            "void*".into(),
                        ],
                        &[
                            "ctx".into(),
                            receiver.clone(),
                            key_pointer,
                            fallback,
                            format!("&{destination}"),
                        ],
                    );
                    let _ = writeln!(out, "    {call};");
                }
            }
            "Set" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let value_pointer = self.materialize(out, &argument(2)?, value)?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_map_set",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        key_pointer,
                        value_pointer,
                        format!("{position}u"),
                    ],
                );
                let _ = writeln!(out, "    (void){call};");
                self.assign(out, result, receiver)?;
            }
            "Has" | "Delete" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let symbol = if name == "Has" {
                    "subscript_rt_map_has"
                } else {
                    "subscript_rt_map_delete"
                };
                let call = self.emitter.runtime_call(
                    "int32_t",
                    symbol,
                    &["void*".into(), "void*".into(), "const void*".into()],
                    &["ctx".into(), receiver.clone(), key_pointer],
                );
                self.assign(out, result, &format!("({call} != 0)"))?;
            }
            "Clear" => {
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_map_clear",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                let _ = writeln!(out, "    {call};");
            }
            "ForEach" => {
                let callback = argument(1)?;
                let bridge = self.emitter.define_assoc_bridge(key, Some(value))?;
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_map_for_each",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        format!("{callback}.code"),
                        format!("{callback}.env"),
                        format!("(const void*)&{bridge}"),
                    ],
                );
                let _ = writeln!(out, "    {call};");
            }
            other => return Err(internal(format!("unknown Map intrinsic {other}"))),
        }
        self.consume_runtime_traps(out, &instruction.traps, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_set_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let key = if name == "New" {
            target.return_type.as_ref().and_then(|ty| match ty {
                l::ValueType::Data(Type::Set(key)) => Some(key.as_ref()),
                _ => None,
            })
        } else {
            operand_types.first().and_then(|ty| match ty {
                l::ValueType::Data(Type::Set(key)) => Some(key.as_ref()),
                _ => None,
            })
        }
        .ok_or_else(|| internal(format!("Set.{name} has no key type")))?;
        if name == "New" {
            let position = self.emitter.pos_id(&instruction.pos);
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_set_new",
                &[
                    "void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    format!("(uint64_t)sizeof({})", self.emitter.ctype(key)?),
                    format!("{}u", association_key_kind(self.emitter.module, key)?),
                    format!("{position}u"),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true);
        }
        let receiver = operands
            .first()
            .ok_or_else(|| internal(format!("Set.{name} receiver is missing")))?;
        let argument = |index: usize| {
            operands
                .get(index)
                .cloned()
                .ok_or_else(|| internal(format!("Set.{name} operand {index} is missing")))
        };
        match name {
            "Size" => {
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_set_size",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                self.assign(out, result, &call)?;
            }
            "Add" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_set_add",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        key_pointer,
                        format!("{position}u"),
                    ],
                );
                let _ = writeln!(out, "    (void){call};");
                self.assign(out, result, receiver)?;
            }
            "Has" | "Delete" => {
                let key_pointer = self.materialize(out, &argument(1)?, key)?;
                let symbol = if name == "Has" {
                    "subscript_rt_set_has"
                } else {
                    "subscript_rt_set_delete"
                };
                let call = self.emitter.runtime_call(
                    "int32_t",
                    symbol,
                    &["void*".into(), "void*".into(), "const void*".into()],
                    &["ctx".into(), receiver.clone(), key_pointer],
                );
                self.assign(out, result, &format!("({call} != 0)"))?;
            }
            "Clear" => {
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_set_clear",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone()],
                );
                let _ = writeln!(out, "    {call};");
            }
            "ForEach" => {
                let callback = argument(1)?;
                let bridge = self.emitter.define_assoc_bridge(key, None)?;
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_set_for_each",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        format!("{callback}.code"),
                        format!("{callback}.env"),
                        format!("(const void*)&{bridge}"),
                    ],
                );
                let _ = writeln!(out, "    {call};");
            }
            "Union" | "Intersection" | "Difference" | "SymmetricDifference" => {
                let symbol = set_symbol(name)?;
                let position = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    symbol,
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        receiver.clone(),
                        argument(1)?,
                        format!("{position}u"),
                    ],
                );
                self.assign(out, result, &call)?;
            }
            "IsSubsetOf" | "IsSupersetOf" | "IsDisjointFrom" => {
                let call = self.emitter.runtime_call(
                    "int32_t",
                    set_symbol(name)?,
                    &["void*".into(), "void*".into(), "void*".into()],
                    &["ctx".into(), receiver.clone(), argument(1)?],
                );
                self.assign(out, result, &format!("({call} != 0)"))?;
            }
            other => return Err(internal(format!("unknown Set intrinsic {other}"))),
        }
        self.consume_runtime_traps(out, &instruction.traps, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_context_bytes(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        _target: &l::CallTarget,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[String],
        _operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let ty = intrinsic
            .type_argument
            .as_ref()
            .ok_or_else(|| internal(format!("Context.{name} has no type argument")))?;
        let ctype = self.emitter.ctype(ty)?;
        let pos = self.emitter.pos_id(&instruction.pos);
        match name {
            "BytesOf" => {
                let source = operands
                    .first()
                    .ok_or_else(|| internal("Context.BytesOf value is missing"))?;
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_from_bytes",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("&{source}"),
                        format!("(uint32_t)sizeof({ctype})"),
                        format!("{pos}u"),
                    ],
                );
                let destination =
                    result.ok_or_else(|| internal("Context.BytesOf has no result"))?;
                let _ = writeln!(out, "    {destination} = {call};");
                self.emit_pending_check(out);
                let data = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_data",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), destination],
                );
                emit_padding_zero(self.emitter.module, self.emitter, out, &data, ty)?;
            }
            "BytesInto" => {
                let source = operands
                    .first()
                    .ok_or_else(|| internal("Context.BytesInto value is missing"))?;
                let target = operands
                    .get(1)
                    .ok_or_else(|| internal("Context.BytesInto target is missing"))?;
                let offset = operands
                    .get(2)
                    .ok_or_else(|| internal("Context.BytesInto offset is missing"))?;
                let range = self.fresh();
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_byte_range",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        target.clone(),
                        offset.clone(),
                        format!("(uint32_t)sizeof({ctype})"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    void* {range} = {call};\n    if (*(const uint32_t*)ctx != 0u) goto unwind;\n    memcpy({range}, &{source}, sizeof({ctype}));");
                emit_padding_zero(self.emitter.module, self.emitter, out, &range, ty)?;
            }
            "FromBytes" => {
                let bytes = operands
                    .first()
                    .ok_or_else(|| internal("Context.FromBytes source is missing"))?;
                let offset = operands
                    .get(1)
                    .ok_or_else(|| internal("Context.FromBytes offset is missing"))?;
                let destination =
                    result.ok_or_else(|| internal("Context.FromBytes has no result"))?;
                let range = self.fresh();
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_byte_range",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        bytes.clone(),
                        offset.clone(),
                        format!("(uint32_t)sizeof({ctype})"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    void* {range} = {call};\n    if (*(const uint32_t*)ctx != 0u) goto unwind;\n    memcpy(&{destination}, {range}, sizeof({ctype}));");
            }
            other => return Err(internal(format!("unknown Context byte intrinsic {other}"))),
        }
        self.consume_runtime_traps(out, &instruction.traps, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_worker_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if name == "Spawn" {
            let index = intrinsic
                .worker_entry
                .ok_or_else(|| internal("Worker.Spawn has no worker entry"))?
                as usize;
            let entry = self
                .emitter
                .module
                .worker_entries
                .get(index)
                .ok_or_else(|| internal(format!("worker entry {index} is missing")))?;
            let call = self.emitter.runtime_call(
                "void*",
                "subscript_rt_worker_spawn",
                &[
                    "subscript_rt_context*".into(),
                    "subscript_rt_worker_init".into(),
                    "subscript_rt_worker_entry".into(),
                    "uint64_t".into(),
                    "uint64_t".into(),
                ],
                &[
                    "ctx".into(),
                    "subscript_init".into(),
                    format!("subscript_worker_entry{index}"),
                    format!("(uint64_t)sizeof({})", self.emitter.class_name(entry.input)),
                    format!(
                        "(uint64_t)sizeof({})",
                        self.emitter.class_name(entry.output)
                    ),
                ],
            );
            self.assign(out, result, &call)?;
            return self.consume_runtime_traps(out, &instruction.traps, true);
        }
        self.emit_simple_runtime_intrinsic(
            out,
            instruction,
            target,
            worker_symbol(name)?,
            operands,
            operand_types,
            false,
            result,
        )
    }
}

fn collect_aggregates(ty: &Type, result: &mut Vec<Type>) {
    match ty {
        Type::FixedArray(element, _) | Type::IterResult(element) => {
            collect_aggregates(element, result);
            if !result.contains(ty) {
                result.push(ty.clone());
            }
        }
        Type::Class(_) | Type::Func(_) | Type::Array(_) | Type::Nullable(_) => {}
        _ => {}
    }
}

fn runtime_header_declares(symbol: &str) -> bool {
    symbol.starts_with("subscript_rt_ctx_") || symbol.starts_with("subscript_rt_worker_")
}

fn is_userdata_slot(ty: &Type) -> bool {
    matches!(ty, Type::Object) || matches!(ty, Type::Nullable(inner) if **inner == Type::Object)
}

fn boundary_type_needs_scratch(
    module: &l::Module,
    ty: &Type,
    visiting: &mut HashSet<ClassId>,
) -> bool {
    match ty {
        Type::Str | Type::Array(_) => true,
        Type::Nullable(inner) => match inner.as_ref() {
            Type::Class(class) => boundary_class_needs_scratch(module, *class, visiting),
            other => boundary_type_needs_scratch(module, other, visiting),
        },
        Type::Class(class) => boundary_class_needs_scratch(module, *class, visiting),
        _ => false,
    }
}

fn boundary_type_requires_recursive_build(module: &l::Module, ty: &Type) -> bool {
    match ty {
        Type::Array(element) => matches!(element.as_ref(), Type::Class(class)
            if lir_class_is_value(module, *class)
                && boundary_class_requires_recursive_build(module, *class)),
        Type::Nullable(inner) => match inner.as_ref() {
            Type::Class(class) if lir_class_is_value(module, *class) => {
                boundary_class_requires_recursive_build(module, *class)
            }
            other => boundary_type_requires_recursive_build(module, other),
        },
        Type::Class(class) if lir_class_is_value(module, *class) => {
            boundary_class_requires_recursive_build(module, *class)
        }
        _ => false,
    }
}

fn boundary_class_needs_scratch(
    module: &l::Module,
    class: ClassId,
    visiting: &mut HashSet<ClassId>,
) -> bool {
    let Some(definition) = module.classes.get(class.0) else {
        return false;
    };
    if !definition.is_value || !visiting.insert(class) {
        return false;
    }
    let result = definition
        .fields
        .iter()
        .any(|field| boundary_type_needs_scratch(module, &field.ty, visiting));
    visiting.remove(&class);
    result
}

fn boundary_class_requires_recursive_build(module: &l::Module, class: ClassId) -> bool {
    boundary_class_needs_scratch(module, class, &mut HashSet::new())
        || boundary_class_contains_pointer_member(module, class, &mut HashSet::new())
}

fn boundary_class_contains_pointer_member(
    module: &l::Module,
    class: ClassId,
    visiting: &mut HashSet<ClassId>,
) -> bool {
    let Some(definition) = module.classes.get(class.0) else {
        return false;
    };
    if !definition.is_value || !visiting.insert(class) {
        return false;
    }
    let result = definition.fields.iter().any(|field| match &field.ty {
        Type::Nullable(inner) => {
            matches!(inner.as_ref(), Type::Class(inner) if lir_class_is_value(module, *inner))
        }
        Type::Class(inner) if lir_class_is_value(module, *inner) => {
            boundary_class_contains_pointer_member(module, *inner, visiting)
        }
        Type::Array(element) => match element.as_ref() {
            Type::Class(inner) if lir_class_is_value(module, *inner) => {
                boundary_class_contains_pointer_member(module, *inner, visiting)
            }
            _ => false,
        },
        _ => false,
    });
    visiting.remove(&class);
    result
}

fn lir_class_is_value(module: &l::Module, class: ClassId) -> bool {
    module
        .classes
        .get(class.0)
        .is_some_and(|definition| definition.is_value)
}

fn array_element_kind(module: &l::Module, ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::Bool
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Object
        | Type::Array(_)
        | Type::Map(..)
        | Type::Set(_) => 0,
        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Enum(_) | Type::Date => 5,
        Type::Class(class) if !lir_class_is_value(module, *class) => 0,
        Type::Nullable(inner) if !matches!(**inner, Type::Func(_)) => 0,
        Type::F32 => 1,
        Type::F64 => 2,
        Type::Str => 3,
        Type::F16 => 4,
        other => {
            return Err(internal(format!(
                "array element type {other:?} has no runtime kind"
            )))
        }
    })
}

fn array_format_kind(ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::I32 | Type::Enum(_) => 0,
        Type::U32 => 1,
        Type::I64 => 2,
        Type::U64 => 3,
        Type::F32 => 4,
        Type::F64 => 5,
        Type::Bool => 6,
        Type::Str => 7,
        Type::I8 => 8,
        Type::U8 => 9,
        Type::I16 => 10,
        Type::U16 => 11,
        Type::F16 => 12,
        other => {
            return Err(internal(format!(
                "array element {other:?} is not formattable"
            )))
        }
    })
}

fn association_key_kind(module: &l::Module, ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::I8
        | Type::U8
        | Type::I16
        | Type::U16
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::Bool
        | Type::Enum(_)
        | Type::Date => 0,
        Type::F32 => 1,
        Type::F64 => 2,
        Type::Str => 3,
        Type::Class(class) if !lir_class_is_value(module, *class) => 4,
        other => {
            return Err(internal(format!(
                "Map/Set key type {other:?} has no runtime kind"
            )))
        }
    })
}

fn array_symbol(name: &str, fixed: bool) -> Result<&'static str, String> {
    if fixed {
        return Ok(match name {
            "ForEach" => "subscript_rt_fixed_arr_for_each",
            "Map" => "subscript_rt_fixed_arr_map",
            "Filter" => "subscript_rt_fixed_arr_filter",
            "Reduce" => "subscript_rt_fixed_arr_reduce",
            "Some" => "subscript_rt_fixed_arr_some",
            "Every" => "subscript_rt_fixed_arr_every",
            "FindIndex" => "subscript_rt_fixed_arr_find_index",
            "ReduceRight" => "subscript_rt_fixed_arr_reduce_right",
            other => {
                return Err(internal(format!(
                    "Array.{other} is not a FixedArray method"
                )))
            }
        });
    }
    Ok(match name {
        "IndexOf" => "subscript_rt_arr_index_of",
        "LastIndexOf" => "subscript_rt_arr_last_index_of",
        "Includes" => "subscript_rt_arr_includes",
        "Join" => "subscript_rt_arr_join",
        "Slice" => "subscript_rt_arr_slice",
        "Fill" => "subscript_rt_arr_fill",
        "Reverse" => "subscript_rt_arr_reverse",
        "Concat" => "subscript_rt_arr_concat",
        "ForEach" => "subscript_rt_arr_for_each",
        "Map" => "subscript_rt_arr_map",
        "Filter" => "subscript_rt_arr_filter",
        "Reduce" => "subscript_rt_arr_reduce",
        "Some" => "subscript_rt_arr_some",
        "Every" => "subscript_rt_arr_every",
        "FindIndex" => "subscript_rt_arr_find_index",
        "Sort" => "subscript_rt_arr_sort",
        "ReduceRight" => "subscript_rt_arr_reduce_right",
        "Splice" => "subscript_rt_arr_splice",
        "Shift" => "subscript_rt_arr_shift",
        "Unshift" => "subscript_rt_arr_unshift",
        "CopyWithin" => "subscript_rt_arr_copy_within",
        other => return Err(internal(format!("unknown Array intrinsic {other}"))),
    })
}

fn set_symbol(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "Union" => "subscript_rt_set_union",
        "Intersection" => "subscript_rt_set_intersection",
        "Difference" => "subscript_rt_set_difference",
        "SymmetricDifference" => "subscript_rt_set_symmetric_difference",
        "IsSubsetOf" => "subscript_rt_set_is_subset_of",
        "IsSupersetOf" => "subscript_rt_set_is_superset_of",
        "IsDisjointFrom" => "subscript_rt_set_is_disjoint_from",
        other => return Err(internal(format!("unknown Set intrinsic {other}"))),
    })
}

fn worker_symbol(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "Post" => "subscript_rt_worker_post",
        "Poll" => "subscript_rt_worker_poll",
        "Close" => "subscript_rt_worker_close",
        "Join" => "subscript_rt_worker_join",
        "InboxWait" => "subscript_rt_worker_inbox_wait",
        "InboxPoll" => "subscript_rt_worker_inbox_poll",
        "OutboxPost" => "subscript_rt_worker_outbox_post",
        other => return Err(internal(format!("unknown Worker intrinsic {other}"))),
    })
}

fn emit_padding_zero(
    module: &l::Module,
    emitter: &Emitter<'_>,
    out: &mut String,
    pointer: &str,
    ty: &Type,
) -> Result<(), String> {
    match ty {
        Type::Class(id) if emitter.class(*id)?.is_value => {
            let class = emitter.class(*id)?;
            let ctype = emitter.class_name(*id);
            if let Some(first) = class.fields.first() {
                let _ = writeln!(
                    out,
                    "    memset((unsigned char*)({pointer}), 0, offsetof({ctype}, d{}));",
                    first.id.0
                );
            }
            for (index, field) in class.fields.iter().enumerate() {
                let field_pointer = format!(
                    "((unsigned char*)({pointer}) + offsetof({ctype}, d{}))",
                    field.id.0
                );
                emit_padding_zero(module, emitter, out, &field_pointer, &field.ty)?;
                let start = format!(
                    "offsetof({ctype}, d{}) + sizeof((({ctype}*)0)->d{})",
                    field.id.0, field.id.0
                );
                let end = class.fields.get(index + 1).map_or_else(
                    || format!("sizeof({ctype})"),
                    |next| format!("offsetof({ctype}, d{})", next.id.0),
                );
                let _ = writeln!(
                    out,
                    "    memset((unsigned char*)({pointer}) + ({start}), 0, ({end}) - ({start}));"
                );
            }
        }
        Type::FixedArray(element, count) => {
            let element_type = emitter.ctype(element)?;
            for index in 0..*count {
                let element_pointer =
                    format!("((unsigned char*)({pointer}) + {index}u * sizeof({element_type}))");
                emit_padding_zero(module, emitter, out, &element_pointer, element)?;
            }
        }
        _ => {
            let _ = module;
        }
    }
    Ok(())
}

fn is_unsigned(ty: &Type) -> bool {
    matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64)
}

fn binary_symbol(operator: l::BinaryOp) -> Result<&'static str, String> {
    Ok(match operator {
        l::BinaryOp::Add => "+",
        l::BinaryOp::Sub => "-",
        l::BinaryOp::Mul => "*",
        l::BinaryOp::Div => "/",
        l::BinaryOp::Rem => "%",
        l::BinaryOp::Eq => "==",
        l::BinaryOp::Ne => "!=",
        l::BinaryOp::Lt => "<",
        l::BinaryOp::Le => "<=",
        l::BinaryOp::Gt => ">",
        l::BinaryOp::Ge => ">=",
        l::BinaryOp::BitAnd => "&",
        l::BinaryOp::BitOr => "|",
        l::BinaryOp::BitXor => "^",
        l::BinaryOp::Shl | l::BinaryOp::Shr | l::BinaryOp::UShr => {
            return Err(internal("shift needs a typed expression"))
        }
    })
}

fn integer_width(ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 => 32,
        Type::I64 | Type::U64 => 64,
        other => return Err(internal(format!("integer width for {other:?}"))),
    })
}

fn unsigned_ctype(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => "uint8_t",
        Type::I16 | Type::U16 => "uint16_t",
        Type::I32 | Type::U32 => "uint32_t",
        Type::I64 | Type::U64 => "uint64_t",
        other => return Err(internal(format!("unsigned carrier for {other:?}"))),
    })
}

fn integer_ctype(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 => "int8_t",
        Type::U8 => "uint8_t",
        Type::I16 => "int16_t",
        Type::U16 => "uint16_t",
        Type::I32 => "int32_t",
        Type::U32 => "uint32_t",
        Type::I64 => "int64_t",
        Type::U64 => "uint64_t",
        other => return Err(internal(format!("integer carrier for {other:?}"))),
    })
}

fn shift_expression(
    operator: l::BinaryOp,
    ty: &Type,
    left: &str,
    right: &str,
) -> Result<String, String> {
    let amount = format!("(({right}) & {}u)", integer_width(ty)? - 1);
    let carrier = match operator {
        l::BinaryOp::Shl | l::BinaryOp::UShr => unsigned_ctype(ty)?,
        l::BinaryOp::Shr => integer_ctype(ty)?,
        other => return Err(internal(format!("shift expression for {other:?}"))),
    };
    let symbol = if operator == l::BinaryOp::Shl {
        "<<"
    } else {
        ">>"
    };
    Ok(format!(
        "(({})((({carrier})({left})) {symbol} {amount}))",
        integer_ctype(ty)?
    ))
}

fn float_to_int_helper(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 => "subscript_f2i8",
        Type::U8 => "subscript_f2u8",
        Type::I16 => "subscript_f2i16",
        Type::U16 => "subscript_f2u16",
        Type::I32 => "subscript_f2i32",
        Type::U32 => "subscript_f2u32",
        Type::I64 => "subscript_f2i64",
        Type::U64 => "subscript_f2u64",
        other => return Err(internal(format!("float conversion target {other:?}"))),
    })
}

fn math_symbol(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "Abs" => "subscript_rt_math_abs",
        "Acos" => "subscript_rt_math_acos",
        "Acosh" => "subscript_rt_math_acosh",
        "Asin" => "subscript_rt_math_asin",
        "Asinh" => "subscript_rt_math_asinh",
        "Atan" => "subscript_rt_math_atan",
        "Atanh" => "subscript_rt_math_atanh",
        "Cbrt" => "subscript_rt_math_cbrt",
        "Ceil" => "subscript_rt_math_ceil",
        "Cos" => "subscript_rt_math_cos",
        "Cosh" => "subscript_rt_math_cosh",
        "Exp" => "subscript_rt_math_exp",
        "Expm1" => "subscript_rt_math_expm1",
        "Floor" => "subscript_rt_math_floor",
        "Log" => "subscript_rt_math_log",
        "Log1p" => "subscript_rt_math_log1p",
        "Log10" => "subscript_rt_math_log10",
        "Log2" => "subscript_rt_math_log2",
        "Round" => "subscript_rt_math_round",
        "Sign" => "subscript_rt_math_sign",
        "Sin" => "subscript_rt_math_sin",
        "Sinh" => "subscript_rt_math_sinh",
        "Sqrt" => "subscript_rt_math_sqrt",
        "Tan" => "subscript_rt_math_tan",
        "Tanh" => "subscript_rt_math_tanh",
        "Trunc" => "subscript_rt_math_trunc",
        "Atan2" => "subscript_rt_math_atan2",
        "Hypot" => "subscript_rt_math_hypot",
        "Pow" => "subscript_rt_math_pow",
        "Max" => "subscript_rt_math_max",
        "Min" => "subscript_rt_math_min",
        "Random" => "subscript_rt_math_random",
        "Clz32" => "subscript_rt_math_clz32",
        "Imul" => "subscript_rt_math_imul",
        "Fround" => "subscript_rt_math_fround",
        "F32ToBits" => "subscript_rt_math_f32_to_bits",
        "F32FromBits" => "subscript_rt_math_f32_from_bits",
        other => return Err(internal(format!("unknown Math intrinsic {other}"))),
    })
}

fn number_symbol(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "IsNaN" => "subscript_rt_num_is_nan",
        "IsFinite" => "subscript_rt_num_is_finite",
        "IsInteger" => "subscript_rt_num_is_integer",
        "IsSafeInteger" => "subscript_rt_num_is_safe_integer",
        "ParseInt" => "subscript_rt_num_parse_int",
        "ParseFloat" => "subscript_rt_num_parse_float",
        "ToFixed" => "subscript_rt_num_to_fixed",
        "ToStringF32" => "subscript_rt_num_to_string_f32",
        "ToStringF64" => "subscript_rt_num_to_string_f64",
        "ToExponential" => "subscript_rt_num_to_exponential",
        "ToPrecision" => "subscript_rt_num_to_precision",
        other => return Err(internal(format!("unknown Number intrinsic {other}"))),
    })
}

fn json_symbol(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "Begin" => "subscript_rt_json_begin",
        "BeginTracked" => "subscript_rt_json_begin_tracked",
        "Finish" => "subscript_rt_json_finish",
        "Raw" => "subscript_rt_json_raw",
        "Str" => "subscript_rt_json_str",
        "I32" => "subscript_rt_json_i32",
        "U32" => "subscript_rt_json_u32",
        "I64" => "subscript_rt_json_i64",
        "U64" => "subscript_rt_json_u64",
        "F32" => "subscript_rt_json_f32",
        "F64" => "subscript_rt_json_f64",
        "Bool" => "subscript_rt_json_bool",
        "Date" => "subscript_rt_json_date",
        "Null" => "subscript_rt_json_null",
        "Visit" => "subscript_rt_json_visit",
        "Leave" => "subscript_rt_json_leave",
        "ParseBegin" => "subscript_rt_json_parse_begin",
        "ParseEnd" => "subscript_rt_json_parse_end",
        "ParseRoot" => "subscript_rt_json_parse_root",
        "ParseIsKind" => "subscript_rt_json_parse_is_kind",
        "ParseNumberFits" => "subscript_rt_json_parse_number_fits",
        "ParseNumber" => "subscript_rt_json_parse_number",
        "ParseInteger" => "subscript_rt_json_parse_integer",
        "ParseBool" => "subscript_rt_json_parse_bool",
        "ParseString" => "subscript_rt_json_parse_string",
        "ParseArrayLen" => "subscript_rt_json_parse_array_len",
        "ParseArrayGet" => "subscript_rt_json_parse_array_get",
        "ParseObjectGet" => "subscript_rt_json_parse_object_get",
        other => return Err(internal(format!("unknown JSON intrinsic {other}"))),
    })
}

fn string_symbol(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "Slice" => "subscript_rt_str_slice",
        "IndexOf" => "subscript_rt_str_index_of",
        "LastIndexOf" => "subscript_rt_str_last_index_of",
        "Includes" => "subscript_rt_str_includes",
        "StartsWith" => "subscript_rt_str_starts_with",
        "EndsWith" => "subscript_rt_str_ends_with",
        "CharCodeAt" => "subscript_rt_str_char_code_at",
        "Split" => "subscript_rt_str_split",
        "Trim" => "subscript_rt_str_trim",
        "TrimStart" => "subscript_rt_str_trim_start",
        "TrimEnd" => "subscript_rt_str_trim_end",
        "Repeat" => "subscript_rt_str_repeat",
        "PadStart" => "subscript_rt_str_pad_start",
        "PadEnd" => "subscript_rt_str_pad_end",
        "ToUpperCase" => "subscript_rt_str_to_upper",
        "ToLowerCase" => "subscript_rt_str_to_lower",
        "Replace" => "subscript_rt_str_replace",
        "ReplaceAll" => "subscript_rt_str_replace_all",
        "Substring" => "subscript_rt_str_substring",
        "Substr" => "subscript_rt_str_substr",
        "CharAt" => "subscript_rt_str_char_at",
        "CodePointAt" => "subscript_rt_str_code_point_at",
        "Concat" => "subscript_rt_str_method_concat",
        other => return Err(internal(format!("unknown String intrinsic {other}"))),
    })
}

fn regex_symbol(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "New" => "subscript_rt_regex_new",
        "Test" => "subscript_rt_regex_test",
        "Source" => "subscript_rt_regex_source",
        "Flags" => "subscript_rt_regex_flags",
        "Search" => "subscript_rt_regex_search",
        "Replace" => "subscript_rt_regex_replace",
        "ReplaceAll" => "subscript_rt_regex_replace_all",
        "Split" => "subscript_rt_regex_split",
        "MatchStart" => "subscript_rt_regex_match_start",
        "MatchEnd" => "subscript_rt_regex_match_end",
        other => return Err(internal(format!("unknown Regex intrinsic {other}"))),
    })
}

fn int_literal(value: i64, ty: &Type) -> String {
    match ty {
        Type::U8 => format!("((uint8_t){})", value as u8),
        Type::U16 => format!("((uint16_t){})", value as u16),
        Type::I8 => format!("((int8_t){value})"),
        Type::I16 => format!("((int16_t){value})"),
        Type::U32 => format!("{}u", value as u32),
        Type::U64 => format!("{}ull", value as u64),
        Type::I64 | Type::Date if value == i64::MIN => "(-9223372036854775807ll - 1)".into(),
        Type::I64 | Type::Date => format!("{value}ll"),
        _ if value == i64::from(i32::MIN) => "(-2147483647 - 1)".into(),
        _ => value.to_string(),
    }
}

fn float_literal(value: f64, ty: &Type) -> String {
    if *ty == Type::F32 {
        let value = value as f32;
        if value.is_nan() {
            return "((float)(0.0f/0.0f))".into();
        }
        if value.is_infinite() {
            return if value.is_sign_negative() {
                "((float)(-1.0f/0.0f))"
            } else {
                "((float)(1.0f/0.0f))"
            }
            .into();
        }
        let mut result = format!("{value:?}");
        if !result.contains(['.', 'e', 'E']) {
            result.push_str(".0");
        }
        result.push('f');
        result
    } else {
        if value.is_nan() {
            return "(0.0/0.0)".into();
        }
        if value.is_infinite() {
            return if value.is_sign_negative() {
                "(-1.0/0.0)"
            } else {
                "(1.0/0.0)"
            }
            .into();
        }
        let mut result = format!("{value:?}");
        if !result.contains(['.', 'e', 'E']) {
            result.push_str(".0");
        }
        result
    }
}

fn trap_runtime_kind(kind: &l::TrapKind) -> Result<TrapKind, String> {
    Ok(match kind {
        l::TrapKind::Allocation => TrapKind::AllocationFailure,
        l::TrapKind::Unreachable => TrapKind::UnreachableReached,
        l::TrapKind::DivisionByZero => TrapKind::DivisionByZero,
        l::TrapKind::IndexRead | l::TrapKind::IndexWrite => TrapKind::IndexOutOfBounds,
        l::TrapKind::JsonResultValue => TrapKind::JsonResultValue,
        l::TrapKind::NullNarrowing => TrapKind::NullNarrowing,
        l::TrapKind::ClassMismatch(_) => TrapKind::ClassMismatch,
        l::TrapKind::DevReloadOnlyStaleCoroutine => TrapKind::StaleCoroutine,
        l::TrapKind::WireEnumValue(_) => TrapKind::WireEnumUnknownValue,
        l::TrapKind::Call | l::TrapKind::DevOnlyLifetime => {
            return Err(internal(format!(
                "trap {kind:?} has no direct runtime kind"
            )))
        }
    })
}

fn c_string_literal(bytes: &[u8]) -> String {
    let mut result = String::from("\"");
    for byte in bytes {
        match byte {
            b'"' => result.push_str("\\\""),
            b'\\' => result.push_str("\\\\"),
            0x20..=0x7e => result.push(char::from(*byte)),
            other => {
                let _ = write!(result, "\\{other:03o}");
            }
        }
    }
    result.push('"');
    result
}

fn render_allocation_metadata_header() -> String {
    r#"/* DO NOT EDIT. Generated by subscript-codegen from the checked program. */
#ifndef SUBSCRIPT_ALLOCATION_METADATA_H
#define SUBSCRIPT_ALLOCATION_METADATA_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint32_t class_id;
    const char *name;
} subscript_alloc_class_info;

typedef struct {
    const char *file;
    uint32_t line;
    uint32_t column;
} subscript_alloc_position_info;

extern const subscript_alloc_class_info subscript_alloc_classes[];
extern const uint64_t subscript_alloc_class_count;
extern const subscript_alloc_position_info subscript_alloc_positions[];
extern const uint64_t subscript_alloc_position_count;

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* SUBSCRIPT_ALLOCATION_METADATA_H */
"#
    .into()
}

fn render_allocation_metadata_definitions(module: &l::Module, positions: &[Pos]) -> String {
    let mut out = String::from(
        "\n/* Allocation attribution tables. Generated from checked HIR and the\n\
* exact pos_id sequence above; consume through the generated\n\
* allocation metadata header. */\n\
typedef struct { uint32_t class_id; const char *name; } subscript_alloc_class_info;\n\
typedef struct { const char *file; uint32_t line; uint32_t column; } subscript_alloc_position_info;\n\n\
const subscript_alloc_class_info subscript_alloc_classes[] = {\n",
    );
    for (class_id, name) in [
        (rtc::CLASS_STRING, "string"),
        (rtc::CLASS_ARRAY, "Array"),
        (rtc::CLASS_ARRAY_DATA, "ArrayData"),
        (rtc::CLASS_GENERATOR, "GeneratorFrame"),
        (rtc::CLASS_MAP, "Map"),
        (rtc::CLASS_SET, "Set"),
        (rtc::CLASS_MAP_DATA, "MapData"),
        (rtc::CLASS_MAP_INDEX, "MapIndex"),
    ] {
        let _ = writeln!(
            out,
            "    {{ {class_id}u, {} }},",
            c_string_literal(name.as_bytes())
        );
    }
    for class in &module.classes {
        let _ = writeln!(
            out,
            "    {{ {}u, {} }},",
            class.id.0,
            c_string_literal(class.source_name.as_bytes())
        );
    }
    let _ = writeln!(
        out,
        "}};\nconst uint64_t subscript_alloc_class_count = {}u;\n",
        8 + module.classes.len()
    );
    out.push_str("const subscript_alloc_position_info subscript_alloc_positions[] = {\n");
    if positions.is_empty() {
        out.push_str("    { \"\", 0u, 0u },\n");
    } else {
        for pos in positions {
            let _ = writeln!(
                out,
                "    {{ {}, {}u, {}u }},",
                c_string_literal(pos.file.as_bytes()),
                pos.line,
                pos.col
            );
        }
    }
    let _ = writeln!(
        out,
        "}};\nconst uint64_t subscript_alloc_position_count = {}u;",
        positions.len()
    );
    out
}

const CALLBACK_VIEW: &str = r#"
typedef struct subscript_callback_string_view { const uint8_t* data; size_t len; } subscript_callback_string_view;
extern void subscript_rt_cb_trampoline(subscript_callback_string_view message, void* userdata1, void* userdata2);
"#;

const PREAMBLE: &str = concat!(
    include_str!("../../runtime/include/subscript_runtime.h"),
    r#"

/* Generated by subscript's LIR-to-C ship transcriber. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>

typedef uint8_t (*SubAsyncResume)(void*, void*, void*);
typedef struct { const unsigned char* data; uint64_t len; } SubStringAliasMember;
typedef struct { uint64_t len; uint64_t cap; uint64_t elem_size; unsigned char* data; } SsArrayHeader;
typedef struct { int32_t state; uint32_t reserved; SubAsyncResume resume; } SubCoroutinePrefix;

static int8_t subscript_f2i8(double v) { if (v != v) return 0; if (v <= -128.0) return -128; if (v >= 127.0) return 127; return (int8_t)v; }
static uint8_t subscript_f2u8(double v) { if (v != v || v <= 0.0) return 0; if (v >= 255.0) return 255; return (uint8_t)v; }
static int16_t subscript_f2i16(double v) { if (v != v) return 0; if (v <= -32768.0) return -32768; if (v >= 32767.0) return 32767; return (int16_t)v; }
static uint16_t subscript_f2u16(double v) { if (v != v || v <= 0.0) return 0; if (v >= 65535.0) return 65535; return (uint16_t)v; }
static int32_t subscript_f2i32(double v) { if (v != v) return 0; if (v <= -2147483648.0) return (-2147483647 - 1); if (v >= 2147483647.0) return 2147483647; return (int32_t)v; }
static uint32_t subscript_f2u32(double v) { if (v != v || v <= 0.0) return 0; if (v >= 4294967295.0) return 4294967295u; return (uint32_t)v; }
static int64_t subscript_f2i64(double v) { if (v != v) return 0; if (v <= -9223372036854775808.0) return (-9223372036854775807ll - 1); if (v >= 9223372036854775807.0) return 9223372036854775807ll; return (int64_t)v; }
static uint64_t subscript_f2u64(double v) { if (v != v || v <= 0.0) return 0; if (v >= 18446744073709551615.0) return 18446744073709551615ull; return (uint64_t)v; }
"#
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_lir_site_fails_with_function_and_site() {
        let pos = Pos::new("trap-probe.ts", 4, 9);
        let trap = l::Trap {
            kind: l::TrapKind::Call,
            pos: pos.clone(),
        };
        let function = l::Function {
            id: l::FunctionId(7),
            source_name: "probe".into(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: Vec::new(),
            return_type: Type::Void,
            locals: Vec::new(),
            values: Vec::new(),
            blocks: Vec::new(),
            entry: l::BlockId(0),
            pos,
        };
        let error = verify_trap_consumption(&function, &[trap.clone(), trap.clone()], &[trap])
            .expect_err("one consumed site cannot satisfy two LIR sites");
        assert!(error.contains("function 7 `probe`"), "{error}");
        assert!(error.contains("trap-probe.ts:4:9"), "{error}");
        assert!(
            error.contains("LIR carries 2 site(s), transcriber consumed 1"),
            "{error}"
        );
    }
}
