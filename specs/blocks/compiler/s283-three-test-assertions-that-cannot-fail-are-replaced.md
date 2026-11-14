<!-- §283 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 283. Three test assertions that cannot fail are replaced

*(Added 2026-11-14.)* Origin: a test cost review of 2026-11-14 at
`8a36c8c1` (section 7, "low-value assertions and tests"). A check
compares two facts that were derived separately (core principle 9).

Problem, read in the code:

1. `examples/tests/gate.rs`
   `derived_example_set_excludes_phase_gate_programs` asserts that
   each discovered example satisfies `is_example_stem`, which is the
   filter that discovery applied, and that no `gate/` program is in
   the example set. Discovery reads only the top level of `examples/`,
   and gate programs live in `examples/gate/` with the id prefix
   `gate/`. Neither assertion can fail.
2. `cli/tests/gate.rs` `assert_record` asserts that a slice taken with
   `[..16]` has length 16. The slice operation guarantees it.
3. `compiler/tests/corpus_warn.rs` pins the count of numbered examples
   to 12. §88 rule 1 says that a suite does not pin a count of
   entries. Each new example must edit this number.

### 283.1 Rules

1. The example discovery reads a root directory that the caller gives.
   The derived-set test runs it over a fixture directory that the test
   writes: one numbered example, one file with a name that is not an
   example name, one non-`.ts` file, and one `gate/` subdirectory with
   a program. The test compares the discovered ids with a hand-written
   list. A second fixture with no numbered example gives the
   discovery error.
2. The length assertion of item 2 is removed. The other assertions of
   `assert_record` stay.
3. The count assertion of item 3 is replaced with an assertion that
   the numbered example set is not empty.
4. The skip-line tests of `perf_gate.rs` and `lir.rs` stay: §85.3
   item 2 requires them.

### 283.2 Acceptance

1. Red first, at the contract commit: a fixture test of rule 1 with
   one wrong expected id fails. The tracking note records the output.
   The landed test has the correct list.
2. `cargo test -p subscript-examples --test gate`,
   `cargo test -p subscript-cli --test gate`, and
   `cargo test -p subscript-compiler --test corpus_warn` pass.
3. The full gate passes. No golden moves.

### 283.3 Phase Review

Pending.

### 283.4 Open

None.
