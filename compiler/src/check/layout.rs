//! Checker-owned aggregate layout limits.
//!
//! One aggregate may occupy at most [`MAX_AGGREGATE_BYTES`] bytes.
//! This is the signed 32-bit direct-displacement range used by the
//! Cranelift lowering for class, frame, and global offsets. Array byte
//! lengths, nested-array products, field offsets, and final struct
//! padding are all included in the bound.

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::hir;
use crate::types::{Type, MAX_AGGREGATE_BYTES};

use super::Checker;

#[derive(Clone, Copy)]
struct Layout {
    size: u64,
    align: u64,
}

#[derive(Clone, Copy)]
enum Outcome {
    Layout(Layout),
    TooLarge,
    Invalid,
}

/// Result of laying out a type without consulting class definitions.
pub(super) enum IndependentLayout {
    Fits,
    TooLarge,
    DependsOnClass,
}

/// Checks a `FixedArray` while its annotation is being resolved when
/// its layout does not depend on a class declared elsewhere.
pub(super) fn class_independent_layout(ty: &Type) -> IndependentLayout {
    match independent_type_layout(ty) {
        Outcome::Layout(_) => IndependentLayout::Fits,
        Outcome::TooLarge => IndependentLayout::TooLarge,
        Outcome::Invalid => IndependentLayout::DependsOnClass,
    }
}

fn limit() -> u64 {
    u64::from(MAX_AGGREGATE_BYTES)
}

fn round_up(value: u64, align: u64) -> Outcome {
    let Some(mask) = align.checked_sub(1) else {
        return Outcome::Invalid;
    };
    let Some(sum) = value.checked_add(mask) else {
        return Outcome::TooLarge;
    };
    let rounded = sum & !mask;
    if rounded > limit() {
        Outcome::TooLarge
    } else {
        Outcome::Layout(Layout {
            size: rounded,
            align,
        })
    }
}

fn array_layout(elem: Outcome, length: u32) -> Outcome {
    let Outcome::Layout(elem) = elem else {
        return elem;
    };
    let Outcome::Layout(stride) = round_up(elem.size, elem.align) else {
        return Outcome::TooLarge;
    };
    let Some(size) = stride.size.checked_mul(u64::from(length)) else {
        return Outcome::TooLarge;
    };
    if size > limit() {
        Outcome::TooLarge
    } else {
        Outcome::Layout(Layout {
            size,
            align: elem.align,
        })
    }
}

fn iter_result_layout(value: Outcome) -> Outcome {
    let Outcome::Layout(value) = value else {
        return value;
    };
    let align = value.align.max(1);
    let Outcome::Layout(value_offset) = round_up(1, align) else {
        return Outcome::TooLarge;
    };
    let Some(end) = value_offset.size.checked_add(value.size) else {
        return Outcome::TooLarge;
    };
    round_up(end, align)
}

fn independent_type_layout(ty: &Type) -> Outcome {
    match ty {
        Type::Bool | Type::I8 | Type::U8 => scalar(1, 1),
        Type::I16 | Type::U16 | Type::F16 => scalar(2, 2),
        Type::I32 | Type::U32 | Type::F32 | Type::Enum(_) => scalar(4, 4),
        Type::I64 | Type::U64 | Type::F64 | Type::Date => scalar(8, 8),
        Type::Str
        | Type::Object
        | Type::Array(_)
        | Type::Map(..)
        | Type::Set(_)
        | Type::Generator(_)
        | Type::Nullable(_)
        | Type::Null => scalar(8, 8),
        Type::Func(_) => scalar(16, 8),
        Type::Void | Type::Error => scalar(0, 1),
        Type::FixedArray(elem, length) => array_layout(independent_type_layout(elem), *length),
        Type::IterResult(value) => iter_result_layout(independent_type_layout(value)),
        Type::Class(_) => Outcome::Invalid,
    }
}

fn scalar(size: u64, align: u64) -> Outcome {
    Outcome::Layout(Layout { size, align })
}

struct Validator<'a> {
    classes: &'a [hir::ClassDef],
    states: Vec<Option<Outcome>>,
    visiting: Vec<bool>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Validator<'a> {
    fn new(classes: &'a [hir::ClassDef]) -> Self {
        Self {
            classes,
            states: vec![None; classes.len()],
            visiting: vec![false; classes.len()],
            diagnostics: Vec::new(),
        }
    }

    fn type_layout(&mut self, ty: &Type) -> Outcome {
        match ty {
            Type::Class(id) => {
                let Some(class) = self.classes.get(id.0) else {
                    return Outcome::Invalid;
                };
                if class.is_value {
                    self.class_layout(id.0)
                } else {
                    scalar(8, 8)
                }
            }
            Type::FixedArray(elem, length) => array_layout(self.type_layout(elem), *length),
            Type::IterResult(value) => iter_result_layout(self.type_layout(value)),
            _ => independent_type_layout(ty),
        }
    }

    fn class_layout(&mut self, id: usize) -> Outcome {
        if let Some(outcome) = self.states.get(id).copied().flatten() {
            return outcome;
        }
        let Some(class) = self.classes.get(id) else {
            return Outcome::Invalid;
        };
        if self.visiting[id] {
            // Containment-cycle reporting remains the existing codegen
            // internal error; this validator is responsible only for
            // representable-size limits.
            return Outcome::Invalid;
        }
        self.visiting[id] = true;

        let mut size = 0u64;
        let mut align = 1u64;
        let mut outcome = Outcome::Invalid;
        let mut complete = true;
        for field in &class.fields {
            let Outcome::Layout(field_layout) = self.type_layout(&field.ty) else {
                complete = false;
                break;
            };
            let Outcome::Layout(aligned) = round_up(size, field_layout.align) else {
                self.class_too_large(class, field);
                complete = false;
                break;
            };
            let Some(end) = aligned.size.checked_add(field_layout.size) else {
                self.class_too_large(class, field);
                complete = false;
                break;
            };
            if end > limit() {
                self.class_too_large(class, field);
                complete = false;
                break;
            }
            size = end;
            align = align.max(field_layout.align);
        }
        if complete {
            match round_up(size.max(1), align) {
                Outcome::Layout(layout) => outcome = Outcome::Layout(layout),
                _ => {
                    let pos = class
                        .fields
                        .last()
                        .map_or_else(|| class.pos.clone(), |field| field.pos.clone());
                    self.diagnostics.push(Diagnostic::new(
                        RuleCode::S100,
                        format!(
                            "`{}` layout exceeds the supported aggregate limit of {} bytes \
                             after final alignment",
                            class.name, MAX_AGGREGATE_BYTES
                        ),
                        pos,
                    ));
                    outcome = Outcome::TooLarge;
                }
            }
        }

        self.visiting[id] = false;
        self.states[id] = Some(outcome);
        outcome
    }

    fn class_too_large(&mut self, class: &hir::ClassDef, field: &hir::Field) {
        self.diagnostics.push(Diagnostic::new(
            RuleCode::S100,
            format!(
                "`{}` layout exceeds the supported aggregate limit of {} bytes \
                 while placing field `{}`",
                class.name, MAX_AGGREGATE_BYTES, field.name
            ),
            field.pos.clone(),
        ));
    }

    fn validate(mut self, pending: &[(Type, Pos, &'static str)]) -> Vec<Diagnostic> {
        for (ty, pos, description) in pending {
            if matches!(self.type_layout(ty), Outcome::TooLarge) {
                self.diagnostics.push(Diagnostic::new(
                    RuleCode::S100,
                    format!(
                        "{description} exceeds the supported aggregate limit of \
                         {MAX_AGGREGATE_BYTES} bytes"
                    ),
                    pos.clone(),
                ));
            }
        }
        for id in 0..self.classes.len() {
            self.class_layout(id);
        }
        self.diagnostics
    }
}

impl Checker<'_> {
    pub(super) fn validate_layouts(&mut self) {
        let diagnostics = Validator::new(&self.classes).validate(&self.pending_layouts);
        self.diags.extend(diagnostics);
    }
}
