<!-- §130 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 130. The value class decorator is `@ValueType`

*(Added 2026-09-29.)* Origin: the owner chose the name on 2026-09-29.

Problem: `@CStruct` marks a value class (C2): copy on assign, on pass,
and on index, no `extends`. That is a meaning inside the script. The
C layout is not what the decorator adds: every language-visible struct
already has the C layout (CLAUDE.md invariant 1). The name reads as a
C interop marker, so it looks like the counterpart of an export to the
host, which it is not.

`@Value` was measured and rejected: a module-level declaration named
`Value` hides the ambient decorator in that module, and `tsc` 5.9.2
rejects `@Value export class Value {}` with TS1238 and TS2449. Four
reject entries declare `class Value` (`r141`, `r142`, `r145`, `r146`).
`@ValueType` has the same shadowing rule, and a class of that name is
rare.

### 130.1 Rules

1. The value class decorator is `@ValueType`, with the same options
   (`@ValueType({ align: N })`, §62) and the same meaning (C2). The
   ambient prelude declares `ValueType`.
2. `@CStruct` is S100 at the decorator, with a message that names
   `@ValueType`. The prelude no longer declares `CStruct`, so `tsc`
   rejects it too (TS2304).
3. Every corpus entry, example, test source, generated doc, and
   diagnostic message spells the decorator `@ValueType`. The contract
   text of the active sections and of `collisions.md`, `corpus.md`,
   `examples.md`, `stdlib.md`, and `warnings.md` spells it
   `@ValueType`; `compiler-history.md` and `specs/tracking/` keep the
   old name as the record of what was.
4. The internal names that follow the decorator (Rust identifiers,
   comments, and corpus entry file names such as `a141-cstruct-align`)
   use the new name; an entry keeps its id and its golden bytes. The
   repository `README.md` spells the new name too.

### 130.2 Acceptance

1. No `.expected` golden moves, except line 8 of `a255`: its program
   prints the label `CStruct:`, which the rename changes to
   `ValueType:`; the round records the old and new line. The LIR text
   snapshot does not move. *(Corrected 2026-09-29: the coding agent
   found the label; the first text allowed no golden move.)*
2. A reject entry pins rule 2: `@CStruct class V { x: i32 = 0; }` is
   S100 with the message naming `@ValueType`; the header states the
   measured `tsc` result.
3. A test pins that `grep` finds `CStruct` in no source, corpus,
   example, prelude, or generated doc file, other than the rule 2
   diagnostic and its reject entry.
