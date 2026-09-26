# §115 Recoverable exceptions, and `JSON.parse` returns `T` — evidence

Contract: `specs/blocks/compiler/s115-recoverable-exceptions-and-a-direct-json-parse.md`.
Date: 2026-09-26. Host: arm64 macOS. Pin: `cb1d469` (contract commit
follows it).

## Owner decisions (2026-09-26)

- The reversal of C6 is accepted.
- A dispose hook that raises while an exception propagates traps.
- An exception that leaves an async body, a generator body, or a Worker
  entry traps; a `try` block that holds a suspension is rejected.

## `tsc` measurements at the pin

`tsc` 5.9.2, the repository `tsconfig.json`, the prelude copied to a
temporary directory:

| Source | Result |
|---|---|
| `catch (e) { const n: i32 = e; }` | TS2322 (`e` is `unknown`) |
| `catch (e: Error) { }` | TS1196 |
| `e instanceof SyntaxError`, `e.message`, `e.name`, `throw e` | clean |
| `catch { }`, `try { } finally { }` | clean |
| `throw 42`, `class MyErr extends Error {}` | clean |
| prelude `parse<T>(text: string): T`; `const c: Config = JSON.parse(text)` | clean, `T = Config` |
| same prelude; `const u = JSON.parse(text); const n: i32 = u;` | TS2322 (`unknown`) |

## Success-path baseline at the pin (§115.12 criterion 4)

`cross-language --only <id>`, release build at `dd2e744`, two runs.
Median of 11 timed runs each; milliseconds.

| Workload | ship run 1 | ship run 2 | jit run 1 | jit run 2 |
|---|---|---|---|---|
| fib-recursive | 3.685 | 3.715 | 7.922 | 7.952 |
| callbacks | 37.485 | 37.524 | 460.480 | 459.353 |
| tree | 132.283 | 131.778 | 420.931 | 412.537 |

The two runs agree within 1% on the ship tier, so the 5% kill threshold
is above the noise.

JSON loop: 200,000 `JSON.parse<Config>` calls on a four-field document
(a string, an `i32`, an `f64`, an `i32[]`), with `JsonResult` and
`Context.free`, ship tier, whole process under `/usr/bin/time -p`,
11 runs: median 0.32 s.

## Rounds

The coding agent ran as a Claude subagent on Opus (codex was near its
usage limit). Each round ran `tools/gate.sh quick` at its end.

| Round | Scope | Result |
|---|---|---|
| 1 | synchronous core: Error classes, `throw`, `try`/`catch`, `instanceof`, LIR handler edge, three engines, host-entry trap 29; a250–a252, r233–r238, r11, t61 | green (`debug 1578/0/2`) after the contract dropped the not-yet-existing ids from C6/C8 |
| 2 | boundaries (async, generator, Worker, host callback); Error is not a JSON type; a256, r239, t63–t65 | green (`debug 1587/0/2`) |
| 3 | `using` on an exception exit: park and resume, trap 30; a253, a254, t62 | red on one count assertion of the round; fixed, re-run inside round 4 |
| 4 | `JSON.parse<T>` returns `T`; `JsonResult` and trap 18 removed; a255; t01 retired; e05 renamed | two quick runs red on fmt and one unused import of the round; fixed |
| full gate | rounds 1–4 | `gate full 36932ad dirty:125 debug 1610/0/2 release 1607/0/2 skips 2/0 clippy 7/18/13 goldens-moved 3 exit 0` |
| 5 | Phase Review fix: a `using` scope is one HIR node; the lowering places hooks; a257 | green (`debug 1617/0/2`); the C1 probe prints `d`, `caught outside`, as `node` does |
| second Phase Review | rounds 1–5 | C1, M1, m1, m2 closed; one new MAJOR (below) |
| 6 | the interpreter raised an exception a second time across nested runtime callback loops | fixed; `codegen/tests/exceptions.rs` 19 passed |

## Contract corrections the rounds forced

- Round 1: the collision table cites an entry only when it exists
  (`compiler/tests/js_corpus.rs`). C6 and C8 gain the new ids in the
  implementation commit.
- Round 1: `catch (e: any)` is `tsc`-clean (TS1196 covers every other
  annotation); it is `r237`.
- Round 1: `JSON.stringify` of an Error printed the hidden tag
  (`{"[[kind]]":0,"name":"Error","message":"x"}`; `node` prints `{}`).
  §115.1 rule 6 makes the Error classes non-JSON types (S014).
- Round 3: a catch plus `throw e` moved the uncaught position to the
  `using` binding (6:9 where the `throw` was at 4:25). §115.5 parks and
  resumes the exception.
- Round 3: a handler region across `await` works on each engine. The
  S010 message for `r234`/`r235` names the form and gives no reason.
- Phase Review (CRITICAL C1, MAJOR M1): a hook that the checker copied
  onto a `return` inside a `try` block became a raise site of that
  `try`. Measured on the dev tier:
  `d`, `caught inside`, `after`, `d`, `caught outside`; `node` v24.18.0:
  `d`, `caught outside`. The same class had reached round 3, so §115.5
  rules 5–8 change the form: one `Stmt::Using` node, hooks placed by the
  shared lowering. §60.1 rule 8 and §97.1 rule 9 retire.

## Phase Reviews

First review: CRITICAL C1 and MAJOR M1 (the `using` exit, above); MINOR
m1 (a raise over a pending exception was silent; it now traps
`Internal`), m2 (lambda and initializer `can_raise` were derived in the
LIR lowering; they are HIR facts now), m3 (the C emitter lets a direct
call to a callee that cannot raise consume its edge with no check; kept,
because the word cannot change there and the verifier derives the same
fact separately), m4 (C6 and C8 ids, added in the implementation
commit), m5 (file sizes, open item), m6 (the after numbers, below), m7
(generated docs; the gate's freshness tests pass).

Second review: C1 and M1 closed. MAJOR: the interpreter's callback
bridge raised an exception a second time when it crossed two nested
runtime callback loops, and `raise_exception` then recorded an
`Internal` trap. Measured with a `Map.groupBy` key callback inside a
`Map.groupBy` key callback: the interpreter reported
`trap internal: an exception was raised while another was pending`;
the dev JIT and ship C caught the exception. Round 6 fixed it. MINOR:
`a253` is `js-comparable` through `ts.transpileModule` at ES2022,
because `node` v24.18.0 rejects `using` in a `case` clause as syntax.
This holds for the §97 `switch` form too.

## Success path after the landing (§115.12 criterion 4)

Release build of the working tree after round 4, same runner and host.
Milliseconds.

| Workload | ship pin (2 runs) | ship after | jit pin (2 runs) | jit after |
|---|---|---|---|---|
| fib-recursive | 3.685 / 3.715 | 3.721 | 7.922 / 7.952 | 8.406 |
| callbacks | 37.485 / 37.524 | 37.215 | 460.480 / 459.353 | 458.558 |
| tree | 132.283 / 131.778 | 131.790 | 420.931 / 412.537 | 419.479 |

Ship tier: each median is within 1% of the pin, under the 5% kill
threshold. Dev JIT `fib-recursive` is 5.7% slower than the pin mean; the
criterion does not cover the JIT, and the cause is not measured.

JSON loop (200,000 parses, ship tier, 11 runs): pin 0.32 s with
`JsonResult` and `Context.free`; after 0.30 s with the direct result.

## Open items

- A Worker trap position is not tier-identical: the parent's
  `WorkerTrapped` message holds the Worker's raw `pos_id`, and the tiers
  number position tables differently (13 on the JIT, 8 on ship, in the
  round 2 test). This held for every Worker trap before §115.
- Ten Rust files that were past 2,000 lines at the pin grew (§5.y
  rule 2 covers a file that a change pushes past the limit, not one
  already past it): `codegen/src/interpreter.rs`,
  `codegen/tests/support/lir_facts.rs`, `compiler/src/hir.rs`,
  `compiler/src/warn.rs`, `compiler/src/check/mod.rs`,
  `runtime/src/context.rs`, `runtime/src/ffi.rs`, `codegen/tests/lir.rs`,
  `codegen/tests/cemit.rs`, `compiler/src/lib.rs`.
- The interpreter's `map_callback_bridge` and `set_callback_bridge`
  are not reachable from lowered LIR: `Map.forEach` and `Set.forEach`
  lower to LIR iterator loops (`codegen/src/lir/call.rs`
  `lower_for_each`). Round 6 measured it with a probe that never fired.
- The checker rejects `function f(): i32 { while (true) { return 1; } }`
  with "non-void function has a reachable fallthrough". This held
  before §115.
- A `try` block that holds `await` or `yield` stays rejected (owner
  decision). Round 3 measured that the handler form works across a
  suspension, so the reason for the rule is the owner's scope choice,
  not a mechanism limit.

## Landing gate

`gate full f6c8d04 dirty:132 debug 1619/0/2 release 1616/0/2 skips 2/0 clippy 7/18/13 goldens-moved 1 exit 0`
(record `target/gate/` of 2026-09-26). The moved golden is the LIR text
snapshot: `can-raise` lines, `Raise(Propagate)` traps, 44
`ParseFailure` intrinsic lines, the new entries, and `a148` (two
per-`break` hook copies became one pair at the `switch` exit). `t01`
and `e05-no-exceptions` are staged deletions. `tools/hygiene.sh`: clean.
