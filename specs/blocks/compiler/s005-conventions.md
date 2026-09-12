<!-- §5 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 5. Conventions

CLAUDE.md code conventions apply to all crates (`compiler/`, `runtime/`):
no panics in library code, `///` docs + `#![warn(missing_docs)]`,
`#[must_use]`, `#[non_exhaustive]`, SAFETY comments on every unsafe impl,
unit tests with every public API, one module per area.

**C-visible symbol prefix.** Every symbol this project defines in the C
namespace carries the project's name: `subscript_rt_*` for the runtime
API the host calls (constants `SUBSCRIPT_RT_*`, and the opaque Context
type, `subscript_rt_context`), and `subscript_*` for
everything the generated program defines — `subscript_init`,
`subscript_export_<name>`, the `subscript_main_entry` typedef, and every
emitted helper and type. *(Renamed 2026-07-29, owner decision, from
`sub_rt_*`/`ss_*`: `sub` alone was ambiguous. `ts_`/`tsc_` was
considered and rejected — it reads as an embedded TypeScript runtime,
which this project is not, and `tsc` is the name of the TypeScript
compiler the gate runs.)*

### 5.x Comments in Rust code *(Owner decision 2026-09-07)*

1. A comment states what the code does, or the rule it follows. A
   rule is cited as the current contract section (`compiler.md §N`,
   `stdlib.md §N`, `collisions.md Cn`). Nothing else is cited.
2. A comment names no plan phase (`P6`, `P25`), request (`R39`),
   observation (`OBS-3`), review round, date, commit, or former
   behaviour ("previously", "no longer", "used to", "superseded",
   "retired"). The history is in `specs/tracking/` and
   `compiler-history.md`; a comment is not a changelog.
3. A comment that restates the code beside it is deleted. A `///`
   on a public item states the item's contract in one to three
   sentences and stays (`missing_docs`). A `// SAFETY:` comment
   stays and states the invariant, not its history.
4. A test's `//!` states what the test proves and cites the
   contract section it proves; it does not name the round or the
   request that produced it.

### 5.y File size *(Owner decision 2026-09-12)*

1. Each section of this contract is one file under
   `specs/blocks/compiler/`. `compiler.md` holds §0, the index, and
   nothing else.
2. A Rust source file has at most 2,000 lines. A file that a change
   would push past the limit is split first, by construct, into
   child modules of the module that holds the type. The split moves
   `impl` blocks and free functions; it changes no signature and
   widens no visibility past `pub(super)`.
3. The reason is measured, not stylistic: an agent that changes one
   function reads the whole file. At 10,000 lines that read costs
   about 100,000 tokens before the change starts
   (`specs/tracking/context-size-2026-09-12.md`).
