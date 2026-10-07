//! Counted element operations use a static element description.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn acquire_copied_array_elements(
        &mut self,
        array: Value,
        pos: &Pos,
    ) -> Result<(), String> {
        let action = self
            .count_action
            .as_ref()
            .and_then(l::CountAction::release_type);
        if action.is_none() {
            return Ok(());
        }
        let value = self.iconst(types::I64, 0);
        let zero = self.iconst(types::I32, 0);
        self.counted_array_operation(array, 0, value, zero, zero, zero, pos)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn counted_array_operation(
        &mut self,
        array: Value,
        operation: u32,
        value: Value,
        target: Value,
        start: Value,
        end: Value,
        pos: &Pos,
    ) -> Result<(), String> {
        let element = self
            .count_action
            .as_ref()
            .and_then(l::CountAction::release_type);
        let description = if let Some(element) = element {
            let description = crate::counted::description(&self.ml.layouts, &element)?;
            let data = self.ml.literal_data(&description)?;
            let global = self.ml.module.declare_data_in_func(data, self.builder.func);
            self.builder.ins().symbol_value(types::I64, global)
        } else if operation == 3 && self.count_action == Some(l::CountAction::Uncounted) {
            self.iconst(types::I64, 0)
        } else {
            return Err(internal("counted array operation has no count action"));
        };
        let operation = self.iconst(types::I32, i64::from(operation));
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        self.call_runtime(
            self.ml.rt.counted_array_operation,
            &[
                self.ctx,
                array,
                description,
                operation,
                value,
                target,
                start,
                end,
                position,
            ],
            false,
        )?;
        Ok(())
    }
}
