<!-- §114 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 114. The compile thread is removed

*(Owner decision 2026-09-21.)* Origin: the Linux full gate at
`c72da05`. Evidence: `specs/tracking/s114-compile-thread-removal.md`
and `specs/tracking/linux-portability.md`, the 2026-09-21 full gate.

Problem. The compiler spawns one thread for each compile, and gives
that thread 8,589,934,592 bytes of stack unoptimized, or 2,147,483,648
optimized (§113.2 rule 1). Three facts stand against the thread.

1. **The size has no input.** §109.2a derived it from the byte limit,
   the measured stack cost of one nesting level, and a margin. The
   worst file of that derivation holds 131,072 bytes and spells one
   level for each byte. §113.1 rule 2 retired the byte limits, so the
   first term is gone. `SOURCE_BYTE_LIMIT` is not in the tree, and no
   test pins the two constants.
2. **A trusted script does not reach the depth the stack buys.**
   Invariant 6 states that scripts are trusted. The deepest source of
   `corpus/`, `examples/`, and `prelude/` nests 8 brackets, over 545
   files. `tsc` 5.9.2 on `node` 24.21.0 answers `RangeError: Maximum
   call stack size exceeded` at 1,000 nested parentheses and at 1,000
   nested type arguments, so invariant 5 admits no program that deep.
   The unoptimized stack holds 275,000 levels of the same shape.
3. **The stack is the measured cause of two defects.** §110 records
   the first. The lowering runs on that thread. One module's code and
   its data fell on the two sides of the 8 GiB reservation, and the
   32-bit displacement did not reach. The Linux full gate at
   `c72da05` is the second. 12 debug tests report `fork JIT runner:
   Cannot allocate memory (os error 12)`. The host refuses `fork`
   over 37.93 GiB of mapped private address space, and five live
   compiles pass that sum.

§90 is not the reason for the thread. §90 is older (2026-09-06), and
its measurement holds no stack overflow: 15,898 runs gave 0 panics and
1 fault, and that fault is `parse_ts_enum_member` of the parser. The
pinned parser fork answers it (§90.1 rule 2).

### 114.1 What goes

1. **The thread.** `on_the_compile_thread`,
   `COMPILE_THREAD_STACK_BYTES`, `ON_THE_COMPILE_THREAD`,
   `CompileThreadMark`, and `CompileWork` go from
   `compiler/src/lib.rs`. Each caller runs the work directly:
   `check_program_with` and `parse_import_specifiers` in the compiler
   crate; `compile.rs`, `emit_files.rs`, `reload.rs`, and `ship.rs` in
   the codegen crate; `lib.rs` and `watch.rs` in the CLI crate. The
   `Send` bounds that the thread required go with it.
2. **The unwind path.** A panic in a compile reaches the caller by its
   own unwind. `std::panic::resume_unwind` on a joined handle goes.
3. **The capacity tests.** `compiler/tests/nesting.rs` loses
   `a_forty_thousand_level_conditional_chain_returns`,
   `a_bare_check_returns_from_a_two_mebibyte_caller_thread`,
   `a_bare_import_scan_returns_from_a_two_mebibyte_caller_thread`, and
   `a_nested_compile_thread_call_runs_inline`. Each one pins a depth
   or the thread itself. No test replaces them: no part of the
   compiler checks a nesting depth.
4. **§85 rule 4a.** The `gate-debug-only:` line has no producer. The
   rule, the count in `tools/gate.sh`, the `debug-only` field of the
   verdict line (§85 rule 6), and the cases of `cli/tests/gate.rs`
   that pin the line go. `gate-skip:` and §85 rule 4 stay as they
   read.

### 114.2 What stays

1. **A compile runs on the thread that calls it.** The depth a source
   can reach is a property of that thread's stack. It is a capacity,
   not a bound. No rule of this compiler bounds a nesting depth, and
   no part of the compiler counts one.
2. **§90.** A public entry returns on the input that invariant 6
   admits. The section's table, its rules, and the pinned parser fork
   do not change.
3. **The linear-time tests** of `compiler/tests/nesting.rs` (§113.2
   rule 2). They check that the checker visits each syntax node one
   time, at depth 200. That is a complexity property of honest code,
   not a nesting limit. The file keeps its name.

   The two tests now run on the thread that libtest gives them, which
   holds 2,097,152 bytes in every build of Rust. Core principle 15
   asks what a gate test costs, so the cost is measured on the
   x86-64 Linux host, for one `check_program` and one
   `check_warnings` at depth 200:

   | Build | Stack the pair needs | Margin over 2,097,152 |
   |---|---|---|
   | unoptimized | over 917,504 and under 950,272 | 2.2 |
   | optimized | over 262,144 and under 524,288 | 4.0 |

   A stack overflow ends the test binary, so the gate reads it as a
   count that fell, not as one named failure. The margin is the
   guard, and §114.5 criterion 5 measures it on each other gate host.
4. **§110.** One dev-JIT module holds its code and its data in one
   reservation. The 8 GiB stack was the measured trigger, and it goes.
   The rule stays, because the assumption it replaced is unstated and
   unsound: separate mappings put no bound on a 32-bit displacement.
5. **The diagnostic renderer's limits** (§113.2 rule 3). The line
   index is a complexity fix, and an honest file with many
   diagnostics reaches it. The 240-byte window and the 200-item cap
   are choices of display for a reader. No number changes.

### 114.3 Sections this one amends

- §113.2 rule 1 is deleted. The compile thread and its two constants
  go.
- §113.2 rule 2 keeps the linear-time tests of
  `compiler/tests/nesting.rs`. Its compile-thread tests and its depth
  tests go with the thread.
- §113.2 rule 3 keeps every number. Its reason is the reader and the
  complexity of the render, not a hostile source.
- §113.2 rule 9 is deleted with §85 rule 4a.
- §85 rule 4a is deleted. §85 rule 6 loses the `debug-only` field.
- §90.1 carries a premise before its rules: the section holds under
  invariant 6. Its rules do not change. The premise is the form,
  because an exception for each hostile shape does not converge
  (CLAUDE.md, two review rounds). §113.2 rule 1 cited §90 as the
  reason for an 8 GiB stack, and an unqualified "any" was the lever.
- §110's problem paragraph records that its trigger is removed. The
  rule and its constants do not change.

### 114.4 Goldens that move

No golden moves. A diagnostic, the LIR text, and the emitted C do not
carry the thread that produced them. If one moves, stop and report.

### 114.5 Exit criteria

1. Outside `specs/`, `git grep` for `on_the_compile_thread`,
   `COMPILE_THREAD_STACK_BYTES`, and `gate-debug-only` answers
   nothing.
2. `tools/gate.sh full` on the x86-64 Linux host is green. No test
   reports `fork JIT runner`, and the debug step reports 0 failed.
3. The peak `VmSize` of the `subscript-codegen` `interop` test binary
   is measured before and after, in both profiles, and recorded. The
   number at the pin is 136,381,092 kB unoptimized.
4. The full gate's wall time before and after is recorded (core
   principle 15). The measurement at the pin is fmt 1, build 0, debug
   154, release 283, clippy 15, tsc 0, hygiene 0 seconds.
5. The Windows and arm64 macOS hosts run the full gate after the
   landing (§100).
6. The Phase Review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
