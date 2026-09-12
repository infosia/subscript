//! Script, closure, and indirect call emission.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn push_argument(
        &mut self,
        output: &mut Vec<Value>,
        value: RV,
        ty: &l::ValueType,
    ) -> Result<(), String> {
        match value_repr(&self.ml.layouts, ty)? {
            Repr::None => {}
            Repr::Scalar(_) => output.push(self.expect_scalar(value)?),
            Repr::Pair => {
                let (code, env) = self.expect_pair(value)?;
                output.extend([code, env]);
            }
            Repr::Agg { .. } => output.push(self.expect_aggregate(value)?),
        }
        Ok(())
    }

    fn call_result(
        &mut self,
        ty: Option<&l::ValueType>,
        results: &[Value],
        sret: Option<Value>,
    ) -> Result<RV, String> {
        let Some(ty) = ty else {
            return Ok(RV::None);
        };
        Ok(match value_repr(&self.ml.layouts, ty)? {
            Repr::None => RV::None,
            Repr::Scalar(_) => RV::Scalar(
                *results
                    .first()
                    .ok_or_else(|| internal("call has no scalar result"))?,
            ),
            Repr::Pair => RV::Pair(
                *results
                    .first()
                    .ok_or_else(|| internal("call has no code result"))?,
                *results
                    .get(1)
                    .ok_or_else(|| internal("call has no environment result"))?,
            ),
            Repr::Agg { .. } => {
                RV::Aggregate(sret.ok_or_else(|| internal("aggregate call has no result slot"))?)
            }
        })
    }

    pub(super) fn method_function(&self, method: l::MethodId) -> Result<l::FunctionId, String> {
        self.ml
            .lir
            .classes
            .iter()
            .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
            .find(|candidate| candidate.id == method)
            .map(|candidate| candidate.function)
            .ok_or_else(|| internal(format!("method {} is missing", method.0)))
    }

    pub(super) fn script_call(
        &mut self,
        function: l::FunctionId,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        receiver: bool,
    ) -> Result<RV, String> {
        let target = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|target| target.id == function)
            .ok_or_else(|| internal(format!("function {} is missing", function.0)))?;
        let mut arguments = vec![self.ctx];
        let sret = if let Some(l::ValueType::Data(ty)) = return_type {
            match self.ml.layouts.repr(ty)? {
                Repr::Agg { size, align } => {
                    let slot = self.stack_slot(size, align);
                    arguments.push(slot);
                    Some(slot)
                }
                _ => None,
            }
        } else {
            None
        };
        let mut operand_index = 0usize;
        if receiver {
            let value = *operands
                .first()
                .ok_or_else(|| internal("method call has no receiver"))?;
            let ty = parameter_types
                .first()
                .ok_or_else(|| internal("method call has no receiver type"))?;
            self.push_argument(&mut arguments, value, ty)?;
            operand_index = 1;
        }
        for (value, ty) in operands
            .iter()
            .copied()
            .skip(operand_index)
            .zip(parameter_types.iter().skip(operand_index))
        {
            self.push_argument(&mut arguments, value, ty)?;
        }
        let results = self.call_script(&function_key(target), &arguments, false)?;
        self.call_result(return_type, &results, sret)
    }

    pub(super) fn static_closure_call(
        &mut self,
        function: l::FunctionId,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
    ) -> Result<RV, String> {
        let target = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|target| target.id == function)
            .ok_or_else(|| internal(format!("function {} is missing", function.0)))?;
        let callable = *operands
            .first()
            .ok_or_else(|| internal("static closure call has no callable"))?;
        let (_, environment) = self.expect_pair(callable)?;
        let mut arguments = vec![self.ctx, environment];
        let sret = if let Some(l::ValueType::Data(ty)) = return_type {
            match self.ml.layouts.repr(ty)? {
                Repr::Agg { size, align } => {
                    let slot = self.stack_slot(size, align);
                    arguments.push(slot);
                    Some(slot)
                }
                _ => None,
            }
        } else {
            None
        };
        for (value, ty) in operands
            .iter()
            .copied()
            .skip(1)
            .zip(parameter_types.iter().skip(1))
        {
            self.push_argument(&mut arguments, value, ty)?;
        }
        // Lambda bodies belong to the current reload generation and have no
        // stable cross-generation slot. The callable operand supplies that
        // generation's environment, so call its declared body directly.
        let results = self.call_script_direct(&function_key(target), &arguments, false)?;
        self.call_result(return_type, &results, sret)
    }

    pub(super) fn indirect_call(
        &mut self,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
    ) -> Result<RV, String> {
        let callable = *operands
            .first()
            .ok_or_else(|| internal("indirect call has no callable"))?;
        let (code, env) = self.expect_pair(callable)?;
        let Type::Func(signature) = data_type(
            parameter_types
                .first()
                .ok_or_else(|| internal("indirect call has no callable type"))?,
        )?
        else {
            return Err(internal("indirect call operand is not a function"));
        };
        let mut arguments = vec![self.ctx, env];
        let sret = match self.ml.layouts.repr(&signature.ret)? {
            Repr::Agg { size, align } => {
                let slot = self.stack_slot(size, align);
                arguments.push(slot);
                Some(slot)
            }
            _ => None,
        };
        for (value, ty) in operands
            .iter()
            .copied()
            .skip(1)
            .zip(parameter_types.iter().skip(1))
        {
            self.push_argument(&mut arguments, value, ty)?;
        }
        let signature = self
            .ml
            .make_sig(&signature.params, &signature.ret, true, false)?;
        let signature = self.builder.import_signature(signature);
        let call = self
            .builder
            .ins()
            .call_indirect(signature, code, &arguments);
        let results = self.builder.inst_results(call).to_vec();
        self.call_result(return_type, &results, sret)
    }
}
