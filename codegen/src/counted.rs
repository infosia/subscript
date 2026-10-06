//! The generated-code release description for each counted type (§171).

use subscript_compiler::{types::CountedType, Type};

use crate::layout::Layouts;
use crate::lower::internal;

// Each node has three native-endian u64 words: kind, stride/offset, count.
// A container's child immediately follows it. No node describes element bytes.
pub(crate) fn description(layouts: &Layouts, ty: &Type) -> Result<Vec<u8>, String> {
    fn append(
        layouts: &Layouts,
        ty: &Type,
        description: &CountedType,
        out: &mut Vec<u8>,
    ) -> Result<(), String> {
        let (kind, offset, count, child) = match (description, ty) {
            (CountedType::Handle, Type::AsyncHandle(_)) => (1, 0, 0, None),
            (CountedType::Array(child), Type::Array(element)) => {
                (2, 0, 0, Some((&**element, &**child)))
            }
            (CountedType::FixedArray(child, count), Type::FixedArray(element, _)) => {
                let (size, _) = layouts.size_align(element)?;
                (
                    3,
                    u64::from(size),
                    u64::from(*count),
                    Some((&**element, &**child)),
                )
            }
            (CountedType::IterResult(child), Type::IterResult(value)) => {
                let (_, align) = layouts.size_align(value)?;
                (4, u64::from(align), 1, Some((&**value, &**child)))
            }
            _ => {
                return Err(internal(
                    "release description does not match its static type",
                ))
            }
        };
        for word in [kind, offset, count] {
            out.extend_from_slice(&word.to_ne_bytes());
        }
        if let Some((ty, child)) = child {
            append(layouts, ty, child, out)?;
        }
        Ok(())
    }
    let shape = ty
        .counted_type()
        .ok_or_else(|| internal("release description requires a counted type"))?;
    let mut bytes = Vec::new();
    append(layouts, ty, &shape, &mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    #[test]
    fn release_description_uses_the_c_layout_at_every_depth() {
        let module = check_program(&[SourceFile::new(
            "description.ts",
            "export function main(): void {}",
        )])
        .expect("clean source");
        let layouts = Layouts::build(&module).expect("valid layouts");
        let handle = Type::AsyncHandle(Box::new(Type::Void));
        let array = Type::Array(Box::new(handle));
        let fixed = Type::FixedArray(Box::new(array), 2);
        let ty = Type::Array(Box::new(Type::IterResult(Box::new(fixed))));
        let bytes = description(&layouts, &ty).expect("counted description");
        let words = bytes
            .chunks_exact(8)
            .map(|word| {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(word);
                u64::from_ne_bytes(bytes)
            })
            .collect::<Vec<_>>();
        assert_eq!(words, [2, 0, 0, 4, 8, 1, 3, 8, 2, 2, 0, 0, 1, 0, 0]);
        assert!(description(&layouts, &Type::Array(Box::new(Type::I32))).is_err());
    }
}
