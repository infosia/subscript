//! Host completion facts derived from the checked signature and target layout.

use super::*;

impl FunctionBuilder<'_, '_> {
    pub(super) fn host_completion_kind(
        &self,
        function: l::ForeignFunctionId,
        expr: &hir::Expr,
    ) -> Result<l::InstructionKind, LowerError> {
        let Type::AsyncHandle(result) = &expr.ty else {
            return Err(self.error(&expr.pos, "host completion result is not a Promise"));
        };
        let layouts = crate::layout::Layouts::build(self.lowering.hir)
            .map_err(|message| self.error(&expr.pos, message))?;
        let c_result = self
            .lowering
            .hir
            .foreign_fns
            .get(function.0 as usize)
            .and_then(|f| f.completion_result.as_deref())
            .ok_or_else(|| self.error(&expr.pos, "host completion C result is missing"))?;
        let (size, _) = c_result_layout(c_result, result, self.lowering.hir)
            .map_err(|message| self.error(&expr.pos, message))?;
        let (error_id, error) = self
            .lowering
            .hir
            .classes
            .iter()
            .enumerate()
            .find(|(_, c)| c.name == "Error")
            .ok_or_else(|| self.error(&expr.pos, "host completion has no Error class"))?;
        let layout = layouts
            .class(error_id)
            .map_err(|message| self.error(&expr.pos, message))?;
        let field = |name: &str, ty: Type| -> Result<u64, LowerError> {
            let index = error
                .fields
                .iter()
                .position(|f| f.name == name && f.ty == ty)
                .ok_or_else(|| {
                    self.error(
                        &expr.pos,
                        format!("host completion Error field `{name}` is invalid"),
                    )
                })?;
            layout
                .field_offsets
                .get(index)
                .copied()
                .map(u64::from)
                .ok_or_else(|| {
                    self.error(&expr.pos, "host completion Error field offset is missing")
                })
        };
        Ok(l::InstructionKind::HostCompletion {
            function,
            result_size: u64::from(size),
            is_void: self
                .lowering
                .hir
                .foreign_fns
                .get(function.0 as usize)
                .and_then(|f| f.completion_result.as_deref())
                == Some("void"),
            error_metadata: [
                u64::from(layout.size),
                error_id as u64,
                field(hir::ERROR_KIND_FIELD, Type::U32)?,
                field("name", Type::Str)?,
                field("message", Type::Str)?,
                0,
            ],
        })
    }
}

pub(super) fn verify_host_completion(
    module: &l::Module,
    function: &l::Function,
    instruction: &l::Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let l::InstructionKind::HostCompletion {
        function: id,
        result_size,
        is_void,
        error_metadata: metadata,
    } = &instruction.kind
    else {
        return;
    };
    let (id, result_size, is_void) = (*id, *result_size, *is_void);
    let bad = |message: &str, errors: &mut Vec<VerifyError>| {
        errors.push(super::verify::finding(function, message));
    };
    let Some(foreign) = module
        .foreign_functions
        .get(id.0 as usize)
        .filter(|f| f.id == id)
    else {
        bad("host completion foreign function is missing", errors);
        return;
    };
    if foreign.parameters.last().is_none_or(|p| {
        p.ty != Type::Void
            || p.foreign_provenance != Some(l::ForeignTypeProvenance::CompletionEndpoint)
    }) || foreign
        .parameters
        .iter()
        .take(foreign.parameters.len().saturating_sub(1))
        .any(|p| p.foreign_provenance == Some(l::ForeignTypeProvenance::CompletionEndpoint))
        || foreign.return_type != Type::Void
    {
        bad("host completion foreign function requires a trailing by-value subscript_rt_completion parameter and a void C return", errors);
    }
    let declared: Vec<_> = foreign
        .parameters
        .iter()
        .filter(|p| p.foreign_provenance != Some(l::ForeignTypeProvenance::CompletionEndpoint))
        .flat_map(|p| match &p.ty {
            Type::Array(element) => vec![
                l::ValueType::Address(l::AddressType {
                    pointee: (**element).clone(),
                    array_base: None,
                }),
                l::ValueType::Data(Type::I32),
            ],
            ty => vec![l::ValueType::Data(ty.clone())],
        })
        .collect();
    let actual: Vec<_> = instruction
        .operands
        .iter()
        .filter_map(|o| super::verify::operand_type(function, o))
        .collect();
    if actual.len() != instruction.operands.len()
        || !super::verify_instruction::declared_parameters_match(
            module,
            &l::CallTargetKind::Foreign(id),
            &actual,
            &declared,
        )
    {
        bad(
            "host completion operand types disagree with the foreign declaration",
            errors,
        );
    }
    let result = instruction
        .result
        .and_then(|v| super::verify::value_type(function, v));
    let Some((_, ty)) = &foreign.completion_result else {
        bad("host completion result declaration is missing", errors);
        return;
    };
    if result != Some(&l::ValueType::Data(Type::async_handle(ty.clone())))
        || is_void != (*ty == Type::Void)
    {
        bad("host completion result is invalid", errors);
    }
    let supported = match ty {
        Type::Void
        | Type::Bool
        | Type::I8
        | Type::U8
        | Type::I16
        | Type::U16
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::F16
        | Type::F32
        | Type::F64
        | Type::Enum(_) => true,
        Type::Class(id) => module
            .classes
            .get(id.0)
            .is_some_and(|c| c.is_value && c.is_boundary),
        _ => false,
    };
    if !supported {
        bad(
            "host completion result is outside the boundary result set",
            errors,
        );
    }
    let Ok(layouts) = crate::layout::Layouts::build_lir(module) else {
        bad("host completion target layout is invalid", errors);
        return;
    };
    if !layouts
        .size_align(ty)
        .is_ok_and(|(size, _)| u64::from(size) == result_size)
    {
        bad(
            "host completion result size disagrees with the target layout",
            errors,
        );
    }
    let error = module.classes.iter().find(|c| c.source_name == "Error");
    let expected = error.and_then(|c| {
        let layout = layouts.class(c.id.0).ok()?;
        let offset = |name: &str, ty: Type| {
            c.fields
                .iter()
                .position(|f| f.source_name == name && f.ty == ty)
                .and_then(|i| layout.field_offsets.get(i).copied())
                .map(u64::from)
        };
        Some([
            u64::from(layout.size),
            c.id.0 as u64,
            offset(hir::ERROR_KIND_FIELD, Type::U32)?,
            offset("name", Type::Str)?,
            offset("message", Type::Str)?,
            0,
        ])
    });
    if expected.as_ref() != Some(metadata) {
        bad(
            "host completion Error metadata disagrees with the Error class layout",
            errors,
        );
    }
}

// The directive selects C scalar sizes independently of the script layout table.
// Named typedefs use their resolved C boundary kinds; structs use their mirror fields.
fn c_result_layout(c: &str, ty: &Type, module: &hir::Module) -> Result<(u32, u32), String> {
    let scalar = if c == "void" {
        Some((0, 1))
    } else {
        subscript_boundary::c_kind(c).map(|kind| (kind.size, kind.align))
    };
    if let Some(layout) = scalar {
        return Ok(layout);
    }
    if let Some((id, _)) = module
        .classes
        .iter()
        .enumerate()
        .find(|(_, class)| class.is_boundary && class.name == c)
    {
        return c_boundary_layout(
            &Type::Class(subscript_compiler::ClassId(id)),
            module,
            &mut Vec::new(),
        );
    }
    // Scalar typedefs retain their resolved boundary kind in the checked declaration.
    // A struct needs its named C declaration; the script result cannot supply it.
    if matches!(ty, Type::Class(_)) {
        return Err("completion C struct declaration is missing".into());
    }
    c_boundary_layout(ty, module, &mut Vec::new())
}

fn c_boundary_layout(
    ty: &Type,
    module: &hir::Module,
    visiting: &mut Vec<usize>,
) -> Result<(u32, u32), String> {
    if let Some(kind) = subscript_compiler::types::boundary_kind(ty) {
        return Ok((kind.size, kind.align));
    }
    Ok(match ty {
        Type::Void => (0, 1),
        Type::Class(id) => {
            let class = module
                .classes
                .get(id.0)
                .filter(|c| c.is_boundary)
                .ok_or("completion C struct is not a boundary struct")?;
            if visiting.contains(&id.0) {
                return Err("completion C struct has a recursive layout".into());
            }
            visiting.push(id.0);
            let mut size = 0u32;
            let mut align = 1u32;
            for field in &class.fields {
                if field.foreign_provenance.is_some() {
                    return Err("completion C struct field has a converted layout".into());
                }
                let (field_size, field_align) = c_boundary_layout(&field.ty, module, visiting)?;
                size = crate::layout::round_up_layout(size, field_align, "completion C field")?
                    .checked_add(field_size)
                    .ok_or("completion C struct size overflow")?;
                align = align.max(field_align);
            }
            visiting.pop();
            (
                crate::layout::round_up_layout(size, align, "completion C struct")?,
                align,
            )
        }
        _ => return Err("completion result is outside the C boundary result set".into()),
    })
}
