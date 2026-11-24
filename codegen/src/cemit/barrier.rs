//! The barrier of a store of a reference into a block (`compiler.md` §356
//! rules 10 to 13).
//!
//! The store fact comes from [`crate::lir::block_store`]. The static type
//! of the stored word decides if the word can hold a reference. After a
//! store with both facts, the emitted code tests the flag "a mark is in
//! progress" of the Context and, on the cold path, gives the stored words
//! to the work list of the mark.

use super::*;
use crate::lir::block_store::{self, BlockStore};

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    /// The condition that a mark is in progress: one load of the flag byte
    /// of the Context (`compiler.md` §356 rule 12).
    fn mark_flag_condition() -> String {
        format!(
            "SUBSCRIPT_UNLIKELY(*((const uint8_t*)ctx + {}u) != 0u)",
            subscript_runtime::Context::mark_in_progress_offset()
        )
    }

    /// True if a word of this type can hold a reference.
    pub(super) fn can_hold_reference(&self, ty: &Type) -> bool {
        *ty != Type::Void && !crate::layout::holds_no_handle(&self.emitter.layouts, ty)
    }

    /// The statement that gives the stored words at `lvalue` to the work
    /// list, with no test of the flag. A pointer is one word; an aggregate
    /// gives its whole range.
    fn mark_statement(&mut self, lvalue: &str, ty: &Type) -> Result<String, String> {
        let ctype = self.emitter.ctype(ty)?;
        if ctype.ends_with('*') {
            Ok(self.emitter.runtime_call(
                "void",
                "subscript_rt_mark_store",
                &["void*".into(), "uintptr_t".into()],
                &["ctx".into(), format!("(uintptr_t)({lvalue})")],
            ))
        } else {
            Ok(self.emitter.runtime_call(
                "void",
                "subscript_rt_mark_range",
                &["void*".into(), "const void*".into(), "uint64_t".into()],
                &[
                    "ctx".into(),
                    format!("(const void*)&({lvalue})"),
                    format!("(uint64_t)sizeof({lvalue})"),
                ],
            ))
        }
    }

    /// Emits the barrier after a store of a word of type `ty` into the
    /// block memory `lvalue` (`compiler.md` §356 rule 10). A store of a
    /// type that holds no reference has none (rule 13).
    pub(super) fn emit_mark_barrier(
        &mut self,
        out: &mut String,
        lvalue: &str,
        ty: &Type,
    ) -> Result<(), String> {
        self.emit_mark_barriers(out, &[(lvalue.to_string(), ty.clone())])
    }

    /// Emits one test of the flag for several stores, then the barrier of
    /// each store whose type can hold a reference.
    pub(super) fn emit_mark_barriers(
        &mut self,
        out: &mut String,
        stores: &[(String, Type)],
    ) -> Result<(), String> {
        let mut statements = Vec::new();
        for (lvalue, ty) in stores {
            if self.can_hold_reference(ty) {
                statements.push(self.mark_statement(lvalue, ty)?);
            }
        }
        if statements.is_empty() {
            return Ok(());
        }
        let _ = writeln!(out, "    if ({}) {{", Self::mark_flag_condition());
        for statement in statements {
            let _ = writeln!(out, "        {statement};");
        }
        out.push_str("    }\n");
        Ok(())
    }

    /// Emits the barrier of the whole range of the lvalue, whatever its
    /// type (`compiler.md` §356 rule 10).
    pub(super) fn emit_mark_range_barrier(&mut self, out: &mut String, lvalue: &str) {
        let statement = self.emitter.runtime_call(
            "void",
            "subscript_rt_mark_range",
            &["void*".into(), "const void*".into(), "uint64_t".into()],
            &[
                "ctx".into(),
                format!("(const void*)&({lvalue})"),
                format!("(uint64_t)sizeof({lvalue})"),
            ],
        );
        let _ = writeln!(
            out,
            "    if ({}) {{\n        {statement};\n    }}",
            Self::mark_flag_condition()
        );
    }

    /// Emits the barrier of the stores of a suspension into the coroutine
    /// frame, if the terminator has the store fact (`compiler.md` §356
    /// rule 13).
    pub(super) fn emit_suspend_barrier(
        &mut self,
        out: &mut String,
        terminator: &l::Terminator,
        stores: &[(String, Type)],
    ) -> Result<(), String> {
        if block_store::terminator_block_store(terminator) {
            self.emit_mark_barriers(out, stores)
        } else {
            Ok(())
        }
    }

    /// Emits the barrier after the store of an instruction whose fact is
    /// [`BlockStore::Operand`]: the operand went to `lvalue`. An instruction
    /// with another fact has no barrier here.
    pub(super) fn emit_store_barrier(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        lvalue: &str,
        operand_types: &[l::ValueType],
    ) -> Result<(), String> {
        let BlockStore::Operand(index) = self.block_store(instruction) else {
            return Ok(());
        };
        match operand_types.get(index) {
            Some(l::ValueType::Data(ty)) => {
                let ty = ty.clone();
                self.emit_mark_barrier(out, lvalue, &ty)
            }
            // A cursor holds its source, so its whole range goes.
            Some(l::ValueType::Iterator(_)) => {
                self.emit_mark_range_barrier(out, lvalue);
                Ok(())
            }
            // An address names a word inside a block or a root, and the
            // block of that word has its own reference.
            Some(l::ValueType::Address(_)) => Ok(()),
            None => Err(internal("a store has no operand of its fact")),
        }
    }

    /// The store fact of `instruction` (`compiler.md` §356 rule 13).
    pub(super) fn block_store(&self, instruction: &l::Instruction) -> BlockStore {
        block_store::instruction_block_store(
            self.emitter.module,
            self.function,
            &self.address_definitions,
            instruction,
        )
    }

    /// Emits the barrier after a call whose fact is
    /// [`BlockStore::ThroughAddresses`]: the target of each address
    /// operand that can be inside a block.
    pub(super) fn emit_address_operand_barriers(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
    ) -> Result<(), String> {
        let mut stores = Vec::new();
        for ((operand, text), ty) in instruction.operands.iter().zip(operands).zip(operand_types) {
            let l::ValueType::Address(address) = ty else {
                continue;
            };
            if !block_store::address_can_be_in_block(
                self.emitter.module,
                self.function,
                &self.address_definitions,
                operand,
            ) {
                continue;
            }
            let ctype = self.emitter.ctype(&address.pointee)?;
            stores.push((format!("*(({ctype}*)({text}))"), address.pointee.clone()));
        }
        self.emit_mark_barriers(out, &stores)
    }
}

#[cfg(test)]
mod tests {
    use subscript_compiler::{check_program, SourceFile};

    /// The emitted C of the function `name`, from its definition to the
    /// next function.
    fn function_source(source: &str, name: &str) -> String {
        let hir = check_program(&[SourceFile::new("barrier.ts", source)]).expect("checks");
        let module = crate::lir::lower_module(&hir).expect("lowers");
        let id = module
            .functions
            .iter()
            .find(|function| function.source_name == name)
            .unwrap_or_else(|| panic!("function {name} is missing"))
            .id;
        let program = crate::emit_c(&hir).expect("emits C");
        let opening = format!(" sub_f{}(", id.0);
        let start = program
            .source
            .match_indices(&opening)
            .map(|(index, _)| index)
            .find(|index| {
                program.source[*index..]
                    .lines()
                    .next()
                    .is_some_and(|line| line.ends_with('{'))
            })
            .unwrap_or_else(|| panic!("definition of {name}"));
        let end = start + program.source[start..].find("\n}\n").expect("function end");
        program.source[start..end].to_string()
    }

    const SOURCE: &str = r#"
class Node { next: Node | null = null; value: i32 = 0; }
function link(a: Node, b: Node): void { a.next = b; }
function number(a: Node): void { a.value = 3; }
export function main(): void {
  const a: Node = new Node();
  link(a, new Node());
  number(a);
  print(`${a.value}`);
}
"#;

    /// `compiler.md` §356 rules 12 and 13: a field store of a reference
    /// has the test of the flag and the call; a field store of a number
    /// has neither.
    #[test]
    fn a_reference_field_store_has_the_barrier_and_a_number_store_has_none() {
        let flag = format!(
            "*((const uint8_t*)ctx + {}u) != 0u",
            subscript_runtime::Context::mark_in_progress_offset()
        );
        let link = function_source(SOURCE, "link");
        assert!(link.contains(&flag), "{link}");
        assert!(
            link.contains("subscript_rt_mark_store(ctx, (uintptr_t)("),
            "{link}"
        );
        let number = function_source(SOURCE, "number");
        assert!(!number.contains(&flag), "{number}");
        assert!(!number.contains("subscript_rt_mark_"), "{number}");
    }

    /// `compiler.md` §356 rules 10 and 13: a suspension inside a `for…of`
    /// loop saves the cursor in the frame. The cursor holds its source
    /// array, so its whole range goes to the work list after the save.
    #[test]
    fn a_cursor_saved_at_a_suspension_has_the_range_barrier() {
        const LOOP: &str = r#"
async function value(n: i32): Promise<i32> { return n; }
async function walk(): Promise<i32> {
  const items: i32[] = [1, 2, 3];
  let total: i32 = 0;
  for (const item of items) { total += await value(item); }
  return total;
}
export async function main(): Promise<void> { print(`${await walk()}`); }
"#;
        // The program saves no other aggregate in a frame: the cursor is
        // the only range of a frame that a barrier gives.
        let hir = check_program(&[SourceFile::new("cursor.ts", LOOP)]).expect("checks");
        let program = crate::emit_c(&hir).expect("emits C");
        assert!(
            program.source.contains("SubIter b"),
            "the frame holds a cursor"
        );
        assert!(
            program
                .source
                .contains("subscript_rt_mark_range(ctx, (const void*)&(frame->b"),
            "{}",
            program.source
        );
    }
}
