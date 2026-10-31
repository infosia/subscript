//! A `FixedArray` traversal reads its subject (compiler.md §220.1).
//!
//! A subject that is a place keeps the operands of that place. Each step
//! computes the element address from them, so no address lives across a step.
//! Another subject is a hidden local.

use super::*;

/// What a `FixedArray` traversal reads at each step.
pub(super) enum FixedSubject {
    /// The subject is a place: its prepared form and the bindings that hold
    /// its operands, in the order of [`place_operands`].
    Place {
        place: PreparedPlace,
        kept: Vec<BindingId>,
        /// The kept operands that a block parameter carries.
        threaded: Vec<BindingId>,
    },
    /// The subject is not a place: the hidden local that holds its value.
    Hidden { local: l::LocalId, ty: Type },
}

impl FixedSubject {
    /// The bindings that each block of the loop must carry.
    pub(super) fn kept_bindings(&self) -> Vec<BindingId> {
        match self {
            Self::Place { threaded, .. } => threaded.clone(),
            Self::Hidden { .. } => Vec::new(),
        }
    }
}

/// The run-time operands of a prepared place, base first.
fn place_operands(place: &mut PreparedPlace) -> Vec<&mut l::Operand> {
    fn base_operands<'p>(base: &'p mut PreparedBase, operands: &mut Vec<&'p mut l::Operand>) {
        match base {
            PreparedBase::Value(value) => operands.push(value),
            PreparedBase::Place(place) => operands.append(&mut place_operands(place)),
        }
    }
    let mut operands = Vec::new();
    match &mut place.kind {
        PreparedPlaceKind::ExistingAddress(address, _) => operands.push(address),
        PreparedPlaceKind::Local(..) | PreparedPlaceKind::Global(..) => {}
        PreparedPlaceKind::Field { base, .. } => base_operands(base, &mut operands),
        PreparedPlaceKind::Index { base, index, .. } => {
            base_operands(base, &mut operands);
            operands.push(index);
        }
    }
    operands
}

impl FunctionBuilder<'_, '_> {
    /// True if a traversal of `expr` reads storage that the program can reach
    /// again. The base of a place that is a value temporary is not one.
    fn is_traversal_place(&self, expr: &hir::Expr) -> bool {
        match &expr.kind {
            hir::ExprKind::Local(name, _, _) => {
                self.lookup_substitution(name).is_none()
                    && self
                        .lookup_binding(name, &expr.pos)
                        .is_ok_and(|binding| self.bindings[binding.0].storage.is_some())
            }
            hir::ExprKind::Global(_) => true,
            hir::ExprKind::Field { obj, .. } => {
                !is_stored_aggregate(self.lowering.hir, &obj.ty)
                    || matches!(obj.kind, hir::ExprKind::This)
                    || self.is_traversal_place(obj)
            }
            hir::ExprKind::Index { obj, .. } => {
                !matches!(obj.ty, Type::FixedArray(..)) || self.is_traversal_place(obj)
            }
            _ => false,
        }
    }

    /// Evaluates the operands of the subject one time, and returns what each
    /// step reads. The first step checks the place.
    pub(super) fn fixed_subject(
        &mut self,
        subject: &hir::Expr,
        pos: &Pos,
    ) -> Result<FixedSubject, LowerError> {
        if !self.is_traversal_place(subject) {
            let value = self.require_expr(subject)?;
            return self.hidden_subject(subject, value, pos);
        }
        let mut place = self.prepare_place(subject)?;
        if place.read.is_some() {
            let value = self.load_place(&place, pos)?;
            return self.hidden_subject(subject, value, pos);
        }
        let mut kept = Vec::new();
        let mut threaded = Vec::new();
        let this_value = self.this_value.clone();
        for operand in place_operands(&mut place) {
            // A receiver is a parameter, so each block reads it. `this` in a
            // `@ValueType` method is the one address that lives across a step
            // (compiler.md §220.1 rule 3).
            if !matches!(operand, l::Operand::Value(_)) || Some(&*operand) == this_value.as_ref() {
                continue;
            }
            let ty = self.operand_type(operand, pos)?;
            let name = format!("<for-of place {}>", self.bindings.len());
            let binding = if is_async_owner_type(&ty) {
                let site = Some(hir::AsyncCopySite::Binding);
                let declared =
                    self.declare_binding(name, ty, false, operand.clone(), pos.clone(), site)?;
                declared.0
            } else {
                let binding = self.declare_hidden_binding(&name, ty, operand.clone());
                threaded.push(binding);
                binding
            };
            kept.push(binding);
        }
        Ok(FixedSubject::Place {
            place,
            kept,
            threaded,
        })
    }

    /// Stores a subject that is not a place in a hidden local (compiler.md
    /// §218.1 rule 12). A counted local is the one owner of its elements, so
    /// its scope releases it on each exit from the traversal (§220.1 rule 7).
    fn hidden_subject(
        &mut self,
        subject: &hir::Expr,
        value: l::Operand,
        pos: &Pos,
    ) -> Result<FixedSubject, LowerError> {
        let ty = l::ValueType::Data(subject.ty.clone());
        let local = self.add_local(
            "<for-of subject>".to_string(),
            ty.clone(),
            false,
            pos.clone(),
        )?;
        self.store_hidden_local(local, value, &ty, pos)?;
        if is_async_owner_type(&ty) {
            let source_name = format!("<for-of subject {}>", local.0);
            let binding = BindingId(self.bindings.len());
            self.bindings.push(Binding {
                source_name: source_name.clone(),
                ty,
                mutable: false,
                storage: Some(local),
                value: None,
            });
            let scope = self.scopes.last_mut();
            scope
                .expect("one binding scope")
                .insert(source_name, binding);
        }
        let ty = subject.ty.clone();
        Ok(FixedSubject::Hidden { local, ty })
    }

    fn store_hidden_local(
        &mut self,
        local: l::LocalId,
        value: l::Operand,
        ty: &l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        self.emit_store_instruction(
            l::InstructionKind::StoreLocal(local),
            vec![value],
            vec![StoredOperand {
                index: 0,
                ty: ty.clone(),
                action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Binding),
                pos: pos.clone(),
            }],
            (None, false),
            Vec::new(),
            pos.clone(),
        )?;
        Ok(())
    }

    /// Keeps a counted subject for the whole traversal.
    pub(super) fn own_for_of_subject(
        &mut self,
        subject: &hir::Expr,
        value: &l::Operand,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let ty = l::ValueType::Data(subject.ty.clone());
        if is_async_owner_type(&ty) {
            self.declare_binding(
                "<for-of owner>".to_string(),
                ty,
                false,
                value.clone(),
                pos.clone(),
                Some(hir::AsyncCopySite::Binding),
            )?;
        }
        Ok(())
    }

    fn subject_address(
        &mut self,
        local: l::LocalId,
        subject_ty: &Type,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        self.emit(
            l::InstructionKind::AddressOfLocal(local),
            Vec::new(),
            Some(l::ValueType::Address(l::AddressType {
                pointee: subject_ty.clone(),
                array_base: None,
            })),
            false,
            Vec::new(),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "local address produced no value"))
    }

    /// Reads the element at `index`. The address of the subject comes from the
    /// kept operands, so a step checks the place as a read of the place does.
    pub(super) fn fixed_element(
        &mut self,
        fixed: &FixedSubject,
        index: l::Operand,
        element: &Type,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        let array = match fixed {
            FixedSubject::Hidden { local, ty } => self.subject_address(*local, ty, pos)?,
            FixedSubject::Place { place, kept, .. } => {
                let mut place = place.clone();
                let this_value = self.this_value.clone();
                let mut kept = kept.iter();
                for operand in place_operands(&mut place) {
                    if !matches!(operand, l::Operand::Value(_))
                        || Some(&*operand) == this_value.as_ref()
                    {
                        continue;
                    }
                    let binding = kept
                        .next()
                        .ok_or_else(|| self.error(pos, "a place operand has no kept binding"))?;
                    *operand = self.read_binding(*binding, pos)?;
                }
                self.materialize_address(&place, pos)?
            }
        };
        let array_base = self.address_base(&array, pos)?;
        let address = self
            .emit(
                l::InstructionKind::AddressOfIndex { checked: false },
                vec![array, index],
                Some(l::ValueType::Address(l::AddressType {
                    pointee: element.clone(),
                    array_base,
                })),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .ok_or_else(|| self.error(pos, "element address produced no value"))?;
        self.emit(
            l::InstructionKind::LoadAddress,
            vec![address],
            Some(l::ValueType::Data(element.clone())),
            false,
            Vec::new(),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "element load produced no value"))
    }

    /// The bound of a traversal: the length that the type states (compiler.md
    /// §220.1 rule 8).
    pub(super) fn fixed_bound(&self, ty: &Type, pos: &Pos) -> Result<l::Operand, LowerError> {
        let Type::FixedArray(_, count) = ty else {
            return Err(self.error(pos, format!("traversal subject `{ty}` is not a FixedArray")));
        };
        let count = i32::try_from(*count)
            .map_err(|_| self.error(pos, "a traversal bound does not fit an i32"))?;
        Ok(i32_constant(count))
    }

    /// The element index of step `step` of a traversal from the last element.
    pub(super) fn reverse_fixed_index(
        &mut self,
        step: &l::Operand,
        bound: &l::Operand,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let ty = Some(l::ValueType::Data(Type::I32));
        let last = self
            .emit(
                l::InstructionKind::Binary(l::BinaryOp::Sub),
                vec![bound.clone(), i32_constant(1)],
                ty.clone(),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .ok_or_else(|| self.error(&expr.pos, "last index produced no value"))?;
        self.emit(
            l::InstructionKind::Binary(l::BinaryOp::Sub),
            vec![last, step.clone()],
            ty,
            false,
            Vec::new(),
            expr.pos.clone(),
        )?
        .ok_or_else(|| self.error(&expr.pos, "reverse index produced no value"))
    }

    /// Clears a hidden local at the loop exit, so a later collection does not
    /// keep its handles (compiler.md §220.1 rule 7).
    pub(super) fn finish_fixed_subject(
        &mut self,
        fixed: &FixedSubject,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let FixedSubject::Hidden { local, ty } = fixed else {
            return Ok(());
        };
        let ty = l::ValueType::Data(ty.clone());
        let zero = self
            .emit(
                l::InstructionKind::Zero,
                Vec::new(),
                Some(ty.clone()),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .ok_or_else(|| self.error(pos, "typed zero produced no value"))?;
        self.store_hidden_local(*local, zero, &ty, pos)
    }
}
