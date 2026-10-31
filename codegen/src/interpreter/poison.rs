//! The address poison of one activation (compiler.md §68.2 item 9, §221.1
//! rule 11). A value identifier is local to one function, so each frame holds
//! its own registry and a move across frames is the check of §221.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use subscript_compiler::lir as l;
use subscript_compiler::Pos;

use super::Invalidation;

type Poison = Rc<RefCell<Option<Invalidation>>>;

/// The addresses that one frame derived from each of its array values.
#[derive(Default)]
pub(super) struct PoisonRegistry {
    addresses: HashMap<l::ValueId, Vec<Weak<RefCell<Option<Invalidation>>>>>,
}

impl PoisonRegistry {
    /// Records an address with the array value that it is based on.
    pub(super) fn register(&mut self, base: l::ValueId, poison: &Poison) {
        self.addresses
            .entry(base)
            .or_default()
            .push(Rc::downgrade(poison));
    }

    /// Poisons each live address of `bases`, and drops each dead one.
    pub(super) fn invalidate(
        &mut self,
        bases: &[l::ValueId],
        pos: &Pos,
        instruction: impl FnOnce() -> String,
    ) {
        let mut instruction = Some(instruction);
        let mut description = None;
        for base in bases {
            if let Some(addresses) = self.addresses.get_mut(base) {
                addresses.retain(|address| {
                    let Some(address) = address.upgrade() else {
                        return false;
                    };
                    let mut poison = address.borrow_mut();
                    if poison.is_none() {
                        let invalidated_by = description
                            .get_or_insert_with(|| {
                                instruction.take().map_or_else(String::new, |f| f())
                            })
                            .clone();
                        *poison = Some(Invalidation {
                            instruction: invalidated_by,
                            pos: pos.clone(),
                        });
                    }
                    true
                });
            }
        }
    }
}
