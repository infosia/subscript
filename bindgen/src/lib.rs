#![warn(missing_docs)]
//! Mirror generator (this project's own `bindgen`, plan P5.2 /
//! `specs/blocks/compiler.md` §12.2).
//!
//! Reads the neutral synthetic C interop header (`corpus/interop/*.h`)
//! and emits the ambient `.d.ts` boundary mirror per the Q13 boundary
//! typing rules (`specs/blocks/collisions.md` §2), already decided and
//! binding:
//!
//! - opaque handle typedef (`typedef struct X_T *X`) → branded empty
//!   `interface X`;
//! - struct-pointer / nullable-struct-pointer field → `X | null`;
//! - `(pointer, count)` array-pair descriptor struct → `T[]` at use
//!   sites (no named type emitted);
//! - length-carrying string-view struct (`{ const char*; size_t; }`) →
//!   `string` at use sites (no named type emitted);
//! - enum → an ambient `declare enum` carrying its constant values;
//! - flag-set typedef → a `u64` type alias with `declare const` members
//!   (the pinned header has no flag-set instance — see the report);
//! - callback userdata slot (`void*`) → `object | null`;
//! - function-pointer typedef → a `type` alias;
//! - every other struct → a boundary `declare class` (C-layout value
//!   struct whose fields may carry the boundary types above);
//! - each C function → an ambient `declare function` with the mapped
//!   signature.
//!
//! The output is global ambient (no `import`/`export`), like the
//! language prelude, so its declarations are visible to every program
//! file. It carries a "do not edit" header; regenerating from the pinned
//! header reproduces it byte-for-byte (the regeneration test).
//!
//! The generator names no external project; every type it recognizes is
//! synthetic (`Sub`-prefixed) or a standard C scalar. It depends only on
//! `std`. Errors are returned as `Result`, never panics.

mod clangfe;
mod cparse;
mod emit;

pub use clangfe::{parse, Alias, Constant, Macro, Parsed};
pub use cparse::{CField, Decl, ParseError};

/// Generates the ambient `.d.ts` mirror text for a C interop header.
///
/// The header is parsed by the libclang-based frontend
/// ([`clangfe`], `specs/blocks/compiler.md` §13.1), which replaced the
/// narrow fixture parser at P6.1; the emitter is unchanged and reproduces
/// the committed mirror byte-for-byte.
///
/// # Errors
///
/// Returns a [`ParseError`] when libclang cannot be loaded, when the
/// header fails to parse, or when it uses a construct the frontend does
/// not model.
pub fn generate(header: &str) -> Result<String, ParseError> {
    let parsed = clangfe::parse(header)?;
    Ok(emit::emit(&parsed.decls))
}
