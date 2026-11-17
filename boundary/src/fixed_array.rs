//! The parameter kind for a C array declaration (compiler.md §324 rule 4).

use crate::{class_pass, PointerPass, StructView};

/// The crossing of a fixed array parameter: its address and its element pass.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FixedArrayParameter<S> {
    /// The declared number of elements.
    pub length: u32,
    /// The struct element and its pass; scalar and handle elements share script memory.
    pub element: Option<(S, PointerPass)>,
}

/// Decides the crossing kind of a C array parameter (compiler.md §324 rules 2 and 3).
#[must_use]
pub fn fixed_array_parameter<V: StructView>(
    view: &V,
    element: Option<V::Struct>,
    length: u32,
    writable: bool,
) -> FixedArrayParameter<V::Struct> {
    FixedArrayParameter {
        length,
        element: element.map(|element| (element, class_pass(view, element, writable))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FieldShape;

    #[test]
    fn fixed_arrays_share_bytes_and_write_back_only_mutable_scratch_elements() {
        struct View;
        impl StructView for View {
            type Struct = usize;
            fn structs(&self) -> Vec<usize> {
                vec![0, 1]
            }
            fn fields(&self, s: usize) -> Option<Vec<FieldShape<usize>>> {
                Some(vec![if s == 0 {
                    FieldShape::Bytes
                } else {
                    FieldShape::StringView
                }])
            }
        }
        for writable in [false, true] {
            assert_eq!(
                fixed_array_parameter(&View, None, 4, writable).element,
                None
            );
            assert_eq!(
                fixed_array_parameter(&View, Some(0), 2, writable).element,
                Some((0, PointerPass::ScriptMemory))
            );
            assert_eq!(
                fixed_array_parameter(&View, Some(1), 2, writable).element,
                Some((
                    1,
                    if writable {
                        PointerPass::ScratchWrittenBack
                    } else {
                        PointerPass::ScratchReadOnly
                    }
                ))
            );
        }
    }
}
