<!-- §189 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 189. The `boundary` command shows each crossing

*(Added 2026-10-11.)* Origin: core principle 16 and §187. On
2026-10-10 the owner selected a command that shows how each foreign
call crosses the C boundary, named it `subscript boundary`, and put it
before §188. The measurement round at `eabe31c3` is
`specs/tracking/s189-boundary-command.md`.

Problem: a foreign call can pass script memory, build a scratch copy,
or copy and write back members after the call (§187 rules 3, 7, 10).
The cost of each choice comes from the declarations, but the call site
does not show it. Core principle 16 rejects a hidden cost. A declared
cost is accepted, so the user needs a way to see it.

### 189.1 Rules

1. **Surface.** `subscript boundary` takes the arguments of `subscript
   check` (the positional source file, `--mirror`, `--enable-module`,
   and `--deny-warnings`; `specs/blocks/cli.md` §2.6). It checks the program, lowers it, and prints one line for
   each foreign call site and each crossing position. It exits 0 when
   the program checks, and with the `check` status and diagnostics
   otherwise.
2. **One crossing plan.** One function gives, for a foreign callee,
   the plan of each parameter and of the result: for each node, the
   pass (§187 `pass.rs`), the member that causes a scratch copy, and the
   members that the call writes back. Code generation of both tiers
   builds its marshaling from this plan, and the command prints it. No
   stage walks the members a second time (core principle 9).
3. **Stage.** The command reads the lowered LIR, the view that both
   tiers use. A generic function gives one line set for each
   instantiation that the program calls; instantiations at one position
   print one after the other.
4. **The result.** The result line comes from the read facts of §187
   rule 12 (`read.rs`), not from the write-direction pass.
5. **Format.** One line per call site and position:
   `<file>:<line>:<col> <callee> <path>: <what> [<cause>, ...]`, then
   `writes-back <member> ...` when the call writes back. `<path>` is
   the parameter name, `.member` for a member, `[]` for pair elements,
   or `result`. `<what>` is a closed word set that the implementation
   lists in one place. Lines sort by position. The format is stable for
   golden tests.
6. **Run-time facts.** A fact that depends on run-time data is stated
   as a rule of the plan, not as a value: a pair copies "per element",
   a write-back writes "changed members", and a null argument builds no
   copy. A form that code generation does not lower (a fixed array of
   structs, §187.3 item 6) prints `not lowered`, with the reason.
7. **Not in this section.** No `--json` form, no per-callee summary,
   and no cost mark in `check`.

### 189.2 Acceptance

1. A golden test of the command on corpus programs that cover each word
   of rule 5: by-value bytes, script memory, scratch read-only, scratch
   written back, a pair, a string view, a callback, a result read by
   members, and `not lowered`.
2. A test that the plan agrees with what code generation emits: for
   each call site of the programs of acceptance 1, the emitted C holds
   a scratch mark exactly when the plan opens a scratch scope, and a
   write-back exactly when the plan writes back (core principle 9: the
   test reads the emitted C, not a record of the plan). The tiers
   record no plan for the test.
3. Cost: the command's time on the largest corpus program and on
   `examples/host/game.ts`, against `check`.
4. The CLI documentation and the C/C++ tutorial name the command where
   they describe foreign calls.
