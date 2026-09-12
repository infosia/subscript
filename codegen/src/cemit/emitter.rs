//! Module-level emission: types, globals, prototypes, functions, and exports.

use super::*;

impl<'m> Emitter<'m> {
    pub(super) fn new(module: &'m l::Module) -> Result<Self, String> {
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
        Ok(Self {
            module,
            layouts: Layouts::build_lir(module)?,
            positions: Vec::new(),
            runtime_symbols: BTreeMap::new(),
            foreign_symbols: Vec::new(),
            field_owners,
            helper_prototypes: String::new(),
            helpers: String::new(),
            helper_count: 0,
            long_string_data: String::new(),
            long_string_symbols: HashMap::new(),
        })
    }

    // §99: this threshold selects a C representation, not a language limit.
    // Only language data uses this pool; names and metadata keep C literals.
    pub(super) fn language_string_pointer(&mut self, bytes: &[u8]) -> String {
        if bytes.len() <= 65_000 {
            return format!("(const unsigned char*){}", c_string_literal(bytes));
        }
        if let Some(symbol) = self.long_string_symbols.get(bytes) {
            return symbol.clone();
        }
        let symbol = format!("sub_long_string_{}", self.long_string_symbols.len());
        let _ = writeln!(
            self.long_string_data,
            "static const unsigned char {symbol}[] = {{"
        );
        for chunk in bytes.chunks(16) {
            self.long_string_data.push_str("    ");
            for byte in chunk {
                let _ = write!(self.long_string_data, "0x{byte:02x},");
            }
            self.long_string_data.push('\n');
        }
        self.long_string_data.push_str("};\n");
        self.long_string_symbols
            .insert(bytes.to_vec(), symbol.clone());
        symbol
    }

    pub(super) fn pos_id(&mut self, pos: &Pos) -> u32 {
        self.positions.push(pos.clone());
        (self.positions.len() - 1) as u32
    }

    pub(super) fn runtime_call(
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

    pub(super) fn class(&self, id: ClassId) -> Result<&l::Class, String> {
        self.module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .ok_or_else(|| internal(format!("class {} is missing", id.0)))
    }

    pub(super) fn field(&self, id: l::FieldId) -> Result<(ClassId, usize, &l::Field), String> {
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

    pub(super) fn function(&self, id: l::FunctionId) -> Result<&l::Function, String> {
        self.module
            .functions
            .get(id.0 as usize)
            .filter(|function| function.id == id)
            .ok_or_else(|| internal(format!("function {} is missing", id.0)))
    }

    pub(super) fn has_closure_environments(&self) -> bool {
        self.module.functions.iter().any(|function| {
            function.kind == l::FunctionKind::Lambda
                && capture_parameters(function).next().is_some()
        })
    }

    pub(super) fn method_function(&self, id: l::MethodId) -> Result<l::FunctionId, String> {
        self.module
            .classes
            .iter()
            .flat_map(|class| class.constructor.iter().chain(&class.methods))
            .find(|method| method.id == id)
            .map(|method| method.function)
            .ok_or_else(|| internal(format!("method {} is missing", id.0)))
    }

    pub(super) fn operation(
        &self,
        intrinsic: &l::Intrinsic,
    ) -> Result<&l::IntrinsicOperation, String> {
        self.module
            .intrinsic_operations
            .iter()
            .find(|operation| {
                operation.family == intrinsic.family && operation.operation == intrinsic.operation
            })
            .ok_or_else(|| {
                internal(format!(
                    "intrinsic {:?}.{} is missing from the module table",
                    intrinsic.family, intrinsic.operation
                ))
            })
    }

    pub(super) fn is_value_class(&self, id: ClassId) -> Result<bool, String> {
        Ok(self.class(id)?.is_value)
    }

    pub(super) fn class_name(&self, id: ClassId) -> String {
        format!("SubC{}", id.0)
    }

    pub(super) fn fixed_array_name(&self, element: &Type, count: u32) -> Result<String, String> {
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
            | Type::AsyncHandle(_)
            | Type::Nullable(_)
            | Type::Null
            | Type::Class(_) => "ptr".into(),
            other => return Err(internal(format!("type tag for {other:?}"))),
        })
    }

    pub(super) fn ctype(&self, ty: &Type) -> Result<String, String> {
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
            | Type::AsyncHandle(_)
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

    pub(super) fn value_ctype(&self, ty: &l::ValueType) -> Result<String, String> {
        match ty {
            l::ValueType::Data(ty) => self.ctype(ty),
            l::ValueType::Address(address) => Ok(format!("{}*", self.ctype(&address.pointee)?)),
            l::ValueType::Iterator(_) => Ok("SubIter".into()),
        }
    }

    pub(super) fn zero(&self, ty: &l::ValueType) -> Result<String, String> {
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

    pub(super) fn emit(mut self, require_main: bool) -> Result<CProgram, String> {
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
        source.push_str(&self.long_string_data);
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
            "typedef struct { void* subject; uint64_t position; uint64_t bound; uint64_t fixed; } SubIter;\n",
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
        let mut closure_environment_types = Vec::new();
        for function in &self.module.functions {
            if matches!(function.kind, l::FunctionKind::Lambda) {
                let captures = capture_parameters(function).collect::<Vec<_>>();
                if !captures.is_empty() {
                    closure_environment_types.push(function.id);
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
        if !closure_environment_types.is_empty() {
            out.push_str("typedef union SubEnvStorage {\n");
            for function in closure_environment_types {
                let _ = writeln!(out, "    SubEnv{} e{};", function.0, function.0);
            }
            out.push_str("} SubEnvStorage;\n");
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
        for local in &function.locals {
            if local.storage == l::LocalStorageClass::Frame {
                let _ = writeln!(out, "    {} l{};", self.value_ctype(&local.ty)?, local.id.0);
            }
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
            if matches!(
                kind,
                l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. }
            ) {
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
            }
        }
        if self.has_closure_environments() {
            for value in &function.values {
                if matches!(value.ty, l::ValueType::Data(Type::Func(_))) {
                    let _ = writeln!(out, "    SubEnvStorage env_v{};", value.id.0);
                }
            }
        }
        let _ = writeln!(out, "}} SubFrame{};", function.id.0);
        Ok(())
    }

    fn emit_globals(&mut self, out: &mut String) -> Result<(), String> {
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
                let data = self.language_string_pointer(member.as_bytes());
                let _ = writeln!(out, "    {{ {data}, {}ull }},", member.len());
            }
            out.push_str("};\n");
        }
        let mut message_classes = Vec::new();
        for entry in &self.module.worker_entries {
            for class in [entry.input, entry.output] {
                if !message_classes.contains(&class) {
                    message_classes.push(class);
                }
            }
        }
        for class in message_classes {
            let offsets = self.layouts.worker_message_string_slot_offsets(class.0)?;
            if !offsets.is_empty() {
                let _ = writeln!(
                    out,
                    "static const uint64_t sub_worker_string_offsets_{}[] = {{ {} }};",
                    class.0,
                    offsets
                        .iter()
                        .map(|offset| format!("{offset}ull"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            let offset_pointer = if offsets.is_empty() {
                "NULL".to_string()
            } else {
                format!("sub_worker_string_offsets_{}", class.0)
            };
            let _ = writeln!(
                out,
                "static const subscript_rt_worker_message_descriptor sub_worker_message_descriptor_{} = {{ (uint64_t)sizeof({}), {}ull, {} }};",
                class.0,
                self.class_name(class),
                offsets.len(),
                offset_pointer
            );
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
        if function.is_async {
            let result_size = if function.return_type == Type::Void {
                "0u".to_string()
            } else {
                format!("(uint64_t)sizeof({})", self.ctype(&function.return_type)?)
            };
            let register = self.runtime_call(
                "void",
                "subscript_rt_async_register",
                &["void*".into(), "void*".into(), "uint64_t".into()],
                &["ctx".into(), "frame".into(), result_size],
            );
            let _ = writeln!(out, "    {register};");
        }
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
            if type_contains_managed(&self.layouts, &global.ty)? {
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
            if Some(function.id) != self.module.entry {
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

    pub(super) fn define_assoc_bridge(
        &mut self,
        key: &Type,
        value: Option<&Type>,
    ) -> Result<String, String> {
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

    pub(super) fn define_group_bridge(
        &mut self,
        element: &Type,
        key: &Type,
    ) -> Result<String, String> {
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
