<!-- §113 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 113. The sandbox profile is removed

*(Owner decision 2026-09-21.)* Origin: plan Rev 4. Evidence:
`specs/tracking/s113-sandbox-removal.md`. It supersedes §109, whose
text is in `compiler-history.md`.

Problem, in two parts.

1. **The profile does not give the guarantee its name states.**
   Same-process execution trusts the compiler, the generated code, and
   the runtime to be memory-safe (§109.0). The profile contains no
   defect of those components. The parser is external, and its work
   and memory on a hostile source have no bound by construction
   (§109.2, M10 and M13), so the compile guarantee was a process
   boundary that the CLI added. `Context.collect()` and every host
   function were excluded from the return guarantee by name. A host
   that must contain content it did not write needs a process
   boundary of its own, and with that boundary the profile adds
   nothing the host lacks.
2. **The profile costs more than any other section.** Measured at
   `8f0a0c2`: 97 tracked files name it. Outside `specs/`, `docs/`,
   `generated-docs/`, `README.md`, `llms.txt`, and `CLAUDE.md`, 641
   lines in 74 files name it. §109 is 786 lines and
   records five Phase Reviews. Three files exist for it alone:
   `cli/src/compile_child.rs` (1,287 lines, three host families),
   `compiler/src/check/profile.rs` (1,010), and
   `runtime/tests/quota_peak.rs` (1,493). Its process-group test
   costs 733 s on the x86-64 Linux gate host after the
   core-principle-15 fix. No downstream host asked for the profile.

Invariant 6 returns to its first form: scripts are trusted. No compile
profile exists, and the language has one accepted form.

### 113.1 What goes

1. **Selection.** `Profile`, `CheckOptions.profile`,
   `CheckOptions.budgets`, `Module.profile`, `RunConfig.profile`, the
   CLI flag `--profile`, and the corpus header key `profile`. A
   harness that reads the key, and an assertion that a profile entry
   exists, go with it.
2. **Compile-time rules.** S023 to S027 are retired. A retired code
   is never assigned again (§99.3). The nesting guard, the checker
   work budget, the LIR instruction budget, the byte limits, and
   `compiler/src/check/profile.rs` go. `LowerError` carries no rule
   code if the instruction budget was the only writer of it.
3. **The intrinsics.** `IntrinsicFamily::Sandbox` and its two
   operations go from the LIR, the dev JIT, the C emitter, and the
   reference interpreter.
4. **The runtime limits.** The interrupt cell and its handle type, the
   allocation quota, the stack budget and the stack floor, the
   charged-bytes counter, `QuotaBuf`, `check_quota`,
   `quota_headroom`, the print-sink charge, the binding charge, and
   the registration charge go. These C symbols go from the generated
   header: `subscript_rt_ctx_interrupt_handle`,
   `subscript_rt_interrupt_set`, `subscript_rt_ctx_set_alloc_quota`,
   `subscript_rt_ctx_set_stack_budget`,
   `subscript_rt_ctx_charged_bytes`, `subscript_rt_sandbox_enter`,
   and `subscript_rt_sandbox_poll`.
5. **Trap kinds 25, 26, and 27 are retired.** The numbers stay
   unassigned, as a retired rule code does. `from_u32` answers them as
   it answers every unknown number. Kind 28 keeps its number.
6. **The budgeted compile child.** `cli/src/compile_child.rs`, its
   five environment variables, the private child flag, and the `libc`
   and `windows-sys` dependencies of the CLI crate go. The watch loop
   compiles in process only.
7. **The runner support.** `HostLimits`, the two default constants,
   `apply_run_limits`, the interrupt fields of `RunConfig` and
   `EntryOptions`, `run_jit_interrupted`, `run_c_aot_interrupted`,
   the C text for the limits and the interrupt thread, the entry
   selection by profile, `ReloadSession::new_configured`, and
   `ReloadSession::new_capturing_initializer_trap_configured` go.
   `EmitError` goes with the rule diagnostic it carried, and
   `emit_c` answers its message as a `String` again. Every in-repo runner installs its
   print observer, as before §109.7a.
8. **The position parameter of three runtime calls.**
   `subscript_rt_cb_bind`, `subscript_rt_cb_register`, and the array
   sort took a position id for a quota refusal alone (§112 rule 4).
   No path reads it now, so the parameter goes (core principle 9).
9. **Corpus, tests, programs.** The 15 entries with the header, the
   eight default-profile twins, and every `.expected` file of those
   entries go: 23 entries, which are `a235` to `a243`, `r233` to
   `r241`, `t57`, `t58`, and `t61` to `t63`. The ids are not assigned
   again. `codegen/tests/sandbox_adversarial.rs`,
   `codegen/tests/sandbox_interrupt.rs`,
   `runtime/tests/quota_peak.rs`, `runtime/tests/quota_reserved.rs`,
   the profile and budget tests of
   `cli/tests/commands.rs`, `examples/sandbox/` and its gate test,
   `benchmarks/src/bin/sandbox-cost.rs`, and the quota block of
   `benchmarks/aot-entry.c` go. `compiler/tests/parser_entry.rs`
   goes, and `lexer_for` returns into its one caller: the byte check
   was the reason for one lexer constructor, and no other problem is
   measured (core principle 13).
10. **The heavy-test variable.** `SUBSCRIPT_HEAVY_TESTS` has no other
    user. It goes from `tools/gate.sh` and from the `environment:`
    line that `cli/tests/gate.rs` pins. The `gate-skip:` and
    `gate-debug-only:` counts of §85 stay: the script counts lines and
    holds no expected number.
11. **Documents.** `README.md`, `llms.txt`, `docs/tutorial-c-cpp.md`,
    `docs/tutorial-rust.md`, and `examples/README.md` state the first
    form of invariant 6 and name no profile. The generated documents
    move through their generators.

### 113.2 What stays

Each item below landed in a §109 round and is not a profile rule. It
holds under the one accepted form, and its reason is stated here
because §109 no longer states it.

1. **The compile thread.** `check_program_with`, every public entry
   that parses, and every tier's lowering and emission run on a
   thread the compiler spawns, with `COMPILE_THREAD_STACK_BYTES` of
   stack. Reason: the parser and the checker recurse once per nesting
   level, and a 2 MiB caller thread overflows at depth 66 (measured,
   §109 round 1). A stack overflow aborts the process, which §90
   forbids. The constants keep their values: 2,147,483,648 bytes
   optimized and 8,589,934,592 unoptimized. They are a capacity, not
   a bound: no byte limit exists, so a source deep enough passes them.
   If the spawn fails, the work runs on the caller's thread. The
   refused-spawn diagnostic and its test hook go. §110 stays as it is.
2. **The checker visits each syntax node once**, and an arm of
   `warn_w002_expr_uses` that walks a child returns. The linear-time
   tests of `compiler/tests/nesting.rs` and its compile-thread tests
   stay. A test there that selects the profile goes.
3. **The diagnostic renderer's limits** (§109.2, M10): one line index
   per file, a window of at most 240 bytes, at most 200 items, and
   the total in the summary line.
4. **`live_bytes` is a maintained counter** (§18.2d). Its debug
   assertion against the walk and its tests stay. The reason is now
   the host: a per-frame read must not walk the live set.
5. **Result buffers that are sized first.** `String.repeat` writes
   into the allocation it sized, and `split` allocates each piece.
   These are default-path improvements. `QuotaBuf` becomes an ordinary
   growing buffer at each of its sites.
6. **Position id 0 is "no script site"** (§112 rules 1 to 3, 5, 6).
7. **The parser fork's duplicate-label fix** (fork commit `affcb6ee`).
8. **`MAX_FRAME_BYTES` and S100.** Only the 65,536-byte profile limit
   goes.
9. **§85 rule 4a.** The `gate-debug-only:` line stays a form of the
   gate with no current producer.

### 113.3 Sections this one amends

- §18.2d: `subscript_rt_ctx_charged_bytes` goes; the counter's reason
  is 113.2 rule 4.
- §111 rule 10 is deleted. The fixture observes reclamation through
  `subscript_rt_ctx_live_bytes`. Criterion 1 loses "quota refusal".
  Criterion 2 compares the live registration count and `live_bytes`.
- §112 rule 4 keeps its first sentence and its two 0 paths; the three
  quota paths and the position parameters go. 112.2's two entries
  (`t61` to `t63`) go, with criteria 3 and 5.
- `specs/blocks/benchmarks.md`: `sandbox-cost` goes.
- §85 rule 4a: its second paragraph named the three heavy tests of
  §109.6a, and it goes.
- The §111 fixture's functions carry the quantity they read in their
  names: `…MarkCharge` becomes `…MarkLiveBytes`, and `…ChargeFellBy`
  becomes `…LiveBytesFellBy`, in the interop fixture and in the
  engine example. The mirrors, the programs that call them, and the
  LIR text snapshot move by those names, and by the column of each
  parameter that follows a longer name on its mirror line.

### 113.4 Goldens that move

1. `codegen/tests/lir-goldens/corpus.txt` loses the two `Sandbox`
   intrinsic lines in each of its 43 entries, 86 lines. With those
   lines and the intrinsic ids normalised, no other line moves.
2. `runtime/include/subscript_runtime.h`,
   `generated-docs/language-reference.md`, and
   `generated-docs/corpus-index.md` move through their generators.
3. The deleted `.expected` files and `examples/sandbox/expected.txt`
   appear in the gate's moved-golden list (§85). The list holds no
   other path, except a golden that prints a renamed fixture function
   (113.3).
4. No `.expected` file of an entry that stays moves. The §111 accept
   entries print a comparison, so the change of counter does not move
   them. If one moves, stop and report.

### 113.5 Exit criteria

1. Outside `specs/`, `git grep -i sandbox` answers only the sentence
   "not a sandbox" of invariant 6, where a document states it, and
   the tooling section of `CLAUDE.md`. `git grep` for each removed symbol
   of 113.1 rules 4 to 7 answers nothing outside `specs/`.
2. `RuleCode::ALL` holds 18 codes, and the generated language
   reference renders. A test pins that trap numbers 25, 26, and 27
   name no kind.
3. `cargo tree -p subscript-cli` names neither `libc` nor
   `windows-sys` as a direct dependency.
4. `tools/gate.sh full` is green on the owner's host. The record
   carries no `SUBSCRIPT_HEAVY_TESTS`, and its moved-golden list is
   the list of 113.4. The Linux and Windows hosts run the gate after
   the landing (§100); the `cfg` arms that go are the reason.
5. The full gate's wall time before and after is recorded in the
   tracking note (core principle 15). No threshold; the number is the
   new baseline.
6. The Phase Review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
