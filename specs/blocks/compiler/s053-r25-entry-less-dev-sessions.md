<!-- §53 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 53. R25 — entry-less dev sessions

Owner-scheduled 2026-08-10 (downstream request R25). The
downstream's windowed example is host-driven: the host calls the
script once per redraw, through named exports. The ship tier
supports that shape (`subscript emit --no-entry`, cli.md §2.2:
exports are the doorway, the host supplies `main`). `subscript
check` accepts the program (measured 2026-08-10: exit 0). The dev
session refused it: `ReloadSession` creation lowers the module,
and the lowering resolved an exported `main(): void`
unconditionally. The session driver does not use that entry — it
calls exports through the slot table — so the requirement was an
asymmetry between the tiers, not a protection.

### 53.1 Rule

Session creation does not require an entry point. Every
`ReloadSession` constructor accepts a module with no exported
`main(): void`. Creation still lowers the full module, runs the
module-global initializer, and applies every existing check.
`call_export` works as before. Hot reload works as before: §8.2
applies to an entry-less session unchanged.

`call_main` on an entry-less session fails loudly with the
existing `call_export` diagnostic: `` `main` is not an exported
zero-argument void function``. The failure ends the call, not the
session.

### 53.2 What does not change

- The dev and Cranelift-AOT run paths spawn a program, so they
  keep the requirement and the diagnostic
  `no exported `main(): void` entry point`.
- Ship: `emit_c` requires `main`; `emit_c_without_main` does not.
  Both are unchanged.
- A program with `main` behaves identically everywhere. Every
  existing golden is byte-identical — an exit criterion.
- The language surface does not move: the accept and reject sets
  are unchanged, and `check` already accepted the shape. No corpus
  entry — the evidence is direct unit tests (core principle 1).

### 53.3 Mechanics constraint

One model, already in the tree: the C emitter splits on
`require_main` and the run paths select the strict form. The
lowering adopts the same split — entry resolution becomes
conditional, the reload path selects the permissive form, and the
run paths keep the strict form with the unchanged diagnostic. The
exact shape (an option, an optional `Lowered` field) is the
implementer's.

### 53.4 Exit criteria (pre-registered)

1. Unit test: a module whose only exports are `frame(): void` and
   `shutdown(): void` creates a session; `call_export("frame")`
   and `call_export("shutdown")` produce the expected output.
2. Unit test: `call_main` on that session returns the §53.1
   diagnostic; a later `call_export` still works.
3. Unit test: an accepted body swap of `frame` on the entry-less
   session is observed in output.
4. Unit test: the dev run path on the same module still fails with
   `no exported `main(): void` entry point`.
5. Full gate green; every existing golden byte-identical; `tsc`
   gate green; zero-warning sweep green; `cargo fmt --check` green.
