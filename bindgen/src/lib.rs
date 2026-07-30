#![warn(missing_docs)]
//! Mirror generator (this project's own `bindgen`, plan P5.2 /
//! `specs/blocks/compiler.md` §12.2).
//!
//! Reads a C interop header and emits the ambient `.d.ts` boundary mirror
//! per the Q13 boundary typing rules (`specs/blocks/collisions.md` §2),
//! already decided and binding:
//!
//! - opaque handle typedef (`typedef struct X_T *X`) → branded empty
//!   `interface X`;
//! - struct-pointer / nullable-struct-pointer field → `X | null`;
//! - `(pointer, count)` array-pair descriptor struct → `T[]` at use
//!   sites (no named type emitted);
//! - length-carrying string-view struct (`{ const char*; size_t; }`) →
//!   `string` at use sites (no named type emitted);
//! - enum → an ambient `declare enum` carrying its constant values;
//! - flag-set typedef (`typedef <uintN> X;` + `static const X …`) → a
//!   `u64` type alias plus `declare const` members whose C values are
//!   folded into the mirror (bare literal initializers, tsc-clean on an
//!   ambient const only without a type annotation), §13.2;
//! - descriptor-embedded `(count, pointer)` array pair inside a struct
//!   (`size_t <n>Count; const T* <n>;`) → the pointer field becomes `T[]`
//!   with the count elided (§13.2);
//! - callback userdata slot (`void*`) → `object | null`;
//! - the supported function-pointer typedef
//!   `(string view, void*, void*) -> void` → a `type` alias; a reachable
//!   callback of every other shape is rejected, while an unreferenced
//!   host-only typedef is omitted (§23.3a);
//! - absorbed string views and descriptors are parameter-only: returning
//!   either aggregate by value is rejected because return provenance has
//!   no mirror vocabulary;
//! - callbacks cross only as mirrored struct fields; direct callback
//!   parameters/returns, callback descriptor elements, and callback fields
//!   in function-free headers are rejected;
//! - every other struct → a boundary `declare class` (C-layout value
//!   struct whose fields may carry the boundary types above);
//! - each C function → an ambient `declare function` with the mapped
//!   signature.
//!
//! The output is global ambient (no `import`/`export`), like the
//! language prelude, so its declarations are visible to every program
//! file. It carries a "do not edit" header; regenerating from the same
//! header reproduces it byte-for-byte (the regeneration test).
//!
//! # C provenance directives
//!
//! When a header declares at least one foreign function, the mirror begins
//! with fixed-shape TypeScript comments carrying every C spelling that C
//! emission needs after boundary shapes have been absorbed:
//!
//! ```text
//! // @subscript-c-header include="engine.h"
//! // @subscript-c-descriptor function="engineWorldReplaceEntities" parameter="engineStates" aggregate="EngineEntityStateView" element="EngineEntityState" const=true
//! // @subscript-c-string-view function="engineWorldSetName" parameter="engineName" aggregate="EngineStringView"
//! // @subscript-c-callback typedef="EngineEventCallback"
//! ```
//!
//! Descriptor and string-view records name a foreign `function` and its
//! absorbed `parameter`. Callback parameters have no aggregate record:
//! the runtime trampoline uses its own layout-identical string-view struct,
//! so no C emission site consumes that name. Quoted values use
//! JSON-compatible escaping for quotes, backslashes, and control
//! characters. Descriptor `const` is `true` when the descriptor's element
//! pointer is const and `false` for a mutable out-array. A consumer may
//! assume one header record, exactly one record per absorbed standalone
//! function parameter, and one callback record per reachable C
//! function-pointer typedef. Descriptor-embedded count/pointer fields have
//! no provenance record because C emission fills their enclosing aggregate
//! positionally.
//! Malformed, duplicate, or missing directives are ingestion errors; a
//! consumer must not reconstruct a C name from a language type.
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
/// narrow fixture parser at P6.1.
///
/// # Errors
///
/// Returns a [`ParseError`] when libclang cannot be loaded, when the
/// header fails to parse, when it uses a construct the frontend does not
/// model, when an absorbed descriptor/string-view appears in return
/// position, when a callback appears in an unsupported boundary position,
/// or when a callback typedef reachable from the boundary differs from the
/// supported `(string view, void*, void*) -> void` shape.
pub fn generate(header: &str) -> Result<String, ParseError> {
    generate_for_header(header, "header.h")
}

/// Generates a mirror whose banner and provenance use `include_spelling`.
///
/// `include_spelling` is the basename a host writes inside `#include`,
/// never a filesystem path. The CLI derives it from its input path; this
/// entry point lets in-memory callers supply the same fact explicitly.
///
/// # Errors
///
/// Returns a [`ParseError`] under the same conditions as [`generate`], or
/// when `include_spelling` is empty or contains a path separator or control
/// character.
pub fn generate_for_header(header: &str, include_spelling: &str) -> Result<String, ParseError> {
    if include_spelling.is_empty()
        || include_spelling.contains(['/', '\\'])
        || include_spelling.chars().any(char::is_control)
    {
        return Err(ParseError(format!(
            "header include spelling `{include_spelling}` must be a nonempty basename"
        )));
    }
    let parsed = clangfe::parse(header)?;
    emit::emit_for_header(&parsed, include_spelling)
}
