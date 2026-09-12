# Context size — the contract file and the four largest Rust files (2026-09-12)

Problem: a round that changes one function reads the whole file that
holds it. The two largest inputs of a round were one contract file
and four Rust files. Measured at `403f8fc`:

| Input | Size |
|---|---|
| `specs/blocks/compiler.md` | 837,002 bytes, 15,907 lines, 114 sections, 410 subsections |
| `codegen/src/lir.rs` | 9,955 lines |
| `codegen/src/cemit.rs` | 9,064 lines |
| `compiler/src/check/expr.rs` | 8,769 lines |
| `codegen/src/lower/func.rs` | 8,344 lines |
| `specs/tracking/` | 108 files, 1.1 MB |
| `CLAUDE.md` | 17,279 bytes, read on every turn |

A 10,000-line Rust file is about 100,000 tokens. One round that
reads a contract section and two of these files spends about
300,000 tokens before the first edit.

## Step 1 — the contract is one file per section

`specs/blocks/compiler.md` keeps §0, the index. Each section is one
file under `specs/blocks/compiler/`, `s<NNN><suffix>-<slug>.md`. The
split is a byte-exact move: the concatenation of the section files
in § order equals the former text from `## 1.` to the end. Each
file starts with one HTML comment that names its section and the
index. The index table gains a `File` column. Rule: §5.y.

Citations do not change. `compiler.md §68` resolves through the §0
table. Grep for `compiler.md` found no code that reads the file and
one anchor link (`examples/README.md`, §14.6), which now points at
the section file.

After the split:

| File | Size |
|---|---|
| `specs/blocks/compiler.md` | 22,310 bytes → about 23 KB with the layout note |
| largest section, `s068-…` | 70,518 bytes |
| second, `s067-…` | 33,646 bytes |

## Step 2 — the four Rust files, by construct

Rule §5.y.2: at most 2,000 lines per Rust source file. The round
splits the four files named above. It moves `impl` blocks and free
functions into child modules; it changes no signature and no
behaviour. The gates that prove that: `cargo fmt --check`, the
clippy baseline, and `tools/gate.sh quick`, with no golden and no
LIR snapshot moved.

Files over 2,000 lines at `403f8fc`, for the record: 17. The four
above, then `runtime/src/ffi.rs` (7,154), `codegen/src/interpreter.rs`
(6,930), `compiler/src/check/mod.rs` (6,476), `runtime/src/context.rs`
(6,121), `compiler/src/hir.rs` (5,066), `compiler/src/lib.rs` (3,481),
`codegen/tests/cemit.rs` (2,767), `runtime/src/arrops.rs` (2,729),
`codegen/tests/lir.rs` (2,575), `codegen/tests/support/lir_facts.rs`
(2,232), `bindgen/src/emit.rs` (2,225), `codegen/src/ship.rs` (2,184),
`compiler/src/warn.rs` (2,155). The 13 that this round does not
split stay listed here until a round splits them. A total check for
§5.y.2 does not exist yet; the baseline-list shape of the clippy
check in `tools/gate.sh` fits it.
