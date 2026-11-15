<!-- §310 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 310. An embedder error carries the rule, the note, and the block

*(Added 2026-11-16.)* Origin: work unit 5 of the owner request of
2026-11-15 (`specs/tracking/divergence-alternatives.md`). The five
variants that no site emits were deleted by §307 rule 4.

Problem, measured at `23e6f607` by the survey. Each CLI path (`check`,
`run`, `build`, `emit`, `watch`) prints a diagnostic through
`render_diagnostics`, which prints the `= rule:` line, the `= note:`
line, and the divergence block of §79 rule 3. A Rust embedder gets the
diagnostics in `RunError` (`codegen/src/jit.rs`), `ReloadError`
(`codegen/src/reload.rs`), or `EmitCFilesError`
(`codegen/src/emit_files.rs`). The `Display` of each prints each
diagnostic through `impl Display for Diagnostic`
(`compiler/src/diag.rs`), which prints `pos: message [code]` only. So
an embedder that prints the error loses the rule, the note, and the
form to write instead. A `TscRejects` diagnostic with an `example`
also loses it in `render_items`, which prints `example` only beside a
divergence; no site has one now.

### 310.1 Rules

1. `Diagnostic` gets one function that gives its detail lines with no
   source excerpt: the `= rule:` line, the `= note:` line, and the
   block of §79 rule 3, in the layout of `render_diagnostics`. The
   renderer of the CLI and the function share one implementation.
2. The `Display` of `RunError`, `ReloadError`, and `EmitCFilesError`
   prints, for each diagnostic, the present line and then its detail
   lines, up to the item limit of the CLI renderer (§113.2 rule 3).
2a. No path turns a checker `Diagnostic` into a `String` before an
   error type holds it. The ship tier (`build_c_aot`) gives the
   `runner_main` diagnostic as `RunError::Rejected`, as the development
   tier does, and `emit_c` keeps the `Diagnostic`. *(Added 2026-11-16
   after the Phase Review: `run_c_aot` gave `RunError::Internal` with
   one line for a program with no `main`.)*
3. `impl Display for Diagnostic` keeps its one line.
4. *(Withdrawn 2026-11-16 after the Phase Review.)* An `example` with
   no divergence stays unprinted: the block starts with
   "TypeScript accepts", and §79 rules 3 and 4 give no block to a
   diagnostic with no divergence. No site has such an example.
4a. The fields that the CLI renderer and `detail_lines` read (the rule
   with its fallback, the example with its fallback) come from one
   function of `Diagnostic`. A test compares `detail_lines` with the
   lines that `render_diagnostics` prints for the same diagnostic.
5. The CLI output does not change.

### 310.2 Acceptance

1. Red first: a test prints a `RunError` of a program that `tsc`
   accepts and this checker rejects, and finds the `= subscript:`
   block; it fails at the pin. The same for `ReloadError` and
   `EmitCFilesError`.
2. A test of rule 2a: `run_c_aot` of a program with no `main` gives
   `RunError::Rejected` with the detail lines (Red at the pin), and
   the test of rule 4a.
3. The rendered output of each reject corpus entry through the CLI is
   the same at the pin and after the change.

### 310.3 Phase Review

At the contract pin, the tests of acceptance items 1 and 2 fail. Over
983 files of `corpus/reject`, `corpus/accept`, `corpus/warn`, and
`corpus/trap`, the CLI output of the pin and of the change is
byte-identical (measured by the review).

One fresh review found one MAJOR finding, realistic, and four MINOR
findings.

1. MAJOR: the ship tier turned the `runner_main` diagnostic into a
   `String` and gave `RunError::Internal` with one line, while the
   development tier gave `RunError::Rejected`. `emit_c` did the same.
   Rule 2a is added; both keep the `Diagnostic` now.
2. MINOR: rule 4 printed an example with no divergence under the label
   "TypeScript accepts", which §79 rules 3 and 4 forbid. Rule 4 is
   withdrawn.
3. MINOR, fixed: the fallbacks of the rule and the example had two
   copies (rule 4a); the three `Display`s had no item limit; the doc of
   `detail_lines`; the order of `use` lines in test modules.

### 310.4 Open

None.
