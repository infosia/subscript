<!-- §302 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 302. Test and tooling fixes from open lists

*(Added 2026-11-15.)* Origin: the open lists of §175, §260, §268, §280,
§283, and §296. On 2026-11-15 the owner told Claude to close the open
items that a small change closes first.

Each item below is a defect of a test, a fixture, or a tool, not of the
language. Each one can hide a failure, leave a file behind, or read a
value that the test did not set. No program changes its output.

### 302.1 Rules

1. §296.4 item 1: the ship-tier scratch test deletes its count file
   before each run, so a run that writes no count fails the test.
2. §296.4 item 2, §280.4 item 2: a test that makes a scratch
   directory removes it on each exit, the exit of a failed assertion
   included (a guard value that removes it when it drops).
3. §296.4 item 3: the `scratch_mark` fields of the two
   `BoundaryTargets` structs have doc comments, and the emitted C check
   of a failed scratch allocation has the indent of its block.
4. §283.4 items 1 to 3: the fixture of §283 does not panic in `Drop`
   if the removal fails; it reports the failure and continues. Each
   fixture name has a counter, so two fixtures of one process do not
   share a name. The empty fixture holds one file that the discovery
   filters out (an unnumbered name), so the test shows that the filter
   runs before the empty check.
5. §280.4 item 1: a host test failure prints the stdout as text, with
   the path of the golden that it compares.
6. §260.4 item 1: if the gate stages `Cargo.lock`, its cleanup restores
   the staged state as well as the working-tree file, so the gate
   leaves the index as it found it.
7. §175.4 item 4: `uncounted_bindings_keep_the_same_outer_assignment`
   has a rejected control in the shape of its accepted programs.
8. §268.4 item 1: the tests that make a retained output file do not
   read `SUBSCRIPT_CODEGEN_JIT_OUTPUT_FILE` from the test environment;
   each test names its own file.

### 302.2 Acceptance

1. Each rule has a test that fails before its change, or, for a rule
   that changes text only (rule 3), a check that the coding agent
   reports. A guard of rule 2 has a test that makes an assertion fail
   inside the guard and finds no directory after it.
2. No corpus entry and no golden changes. The full gate passes.
3. Each test that this section adds states its measured cost.

### 302.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review measured rule 6 on scratch git repositories:
a gate that stages a changed `Cargo.lock` left it staged before the
change, and the script now restores the index and the working file for
exit statuses 0, 7, and 3, and for SIGTERM, with the same statuses.

1. MINOR, contrived, recorded: §302.4 item 1.
2. MINOR, contrived, fixed: a counter assertion could not fail without
   the counter, because the two names already differed by the clock.
3. MINOR, realistic, fixed: a test read the source text of another
   test (core principle 9). It is removed; the rejected control stays.
4. MINOR, realistic, fixed: rule 8 had a check on the source spelling
   `RetainedOutput::new()`. The output path now comes from the caller
   (`RetainedOutput::open`), so a test cannot read the environment.
5. MINOR, realistic, fixed: two doc comments cited this section, not
   the contract of the field (§296).
6. MINOR, realistic, fixed: a test file header stated the old counts.

### 302.4 Open

1. The test of rule 1 checks the helper that deletes the count file. A
   ship run with no post-run hook, which would show the rule in the
   test itself, costs a C build.
