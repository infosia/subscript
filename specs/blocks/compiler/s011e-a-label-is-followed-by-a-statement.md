<!-- §11e of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 11e. A label is followed by a statement

*(2026-08-28, review of §68 consumers, M4.)* The emitted C placed a
declaration directly after a resume label: `resume_b6: SubFn t0 =
frame->b6_v14;`. C11 6.8.1 gives a label a statement, and a
declaration is not one; clang reports it under `-pedantic` as a C23
extension, and MSVC rejects it *(docs)*. The emitter writes `;` after
every label it emits, so a declaration that follows is a statement's
successor. `verify_no_empty_aggregate`'s neighbour checks the emitted
text for a label followed by a declaration, over every corpus entry.
