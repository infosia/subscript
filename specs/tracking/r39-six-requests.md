# R39 — six of nine downstream requests, landed

Status: **landed 2026-09-02** against `specs/blocks/compiler.md` §82
and `specs/blocks/collisions.md` C7, C10, C12. Origin: downstream
request R39 at pin `e1c2be1`. Contract `25c9437`, amended `dad652a`
after the review. Implementation `0549a46` (§82.2), `47c71cf`
(§82.1), `a24865f` (§82.3), `505c7f5` (§82.4), `a0edb7e` (§82.6),
`ec41d65` (§82.5), fixes `61c31e9`.

## Decisions

| Item | Decision |
|---|---|
| R39.1 `Ref<T>` parameter | deferred (owner, 2026-09-02); §82.7 |
| R39.2 operators | withdrawn by the request; `tsc` TS2365 |
| R39.3 compound assignment sugar | landed, §82.1 |
| R39.4 three codes | landed, §82.2: S016, S017, S018 |
| R39.5 `??`, `?.` | landed narrowed, §82.3 |
| R39.6 method type parameters | landed, §82.4 |
| R39.7 downstream tool | landed, §82.6 |
| R39.8 static read accessor | landed, §82.5 |
| R39.9 overloads | deferred (owner, 2026-09-02); `r39-overloads-deferred.md` |

## Findings at the pin, this host

- Every probe in the request reproduced (§82 measurements 1–6).
- `tsc` 5.9.2 rejects `const a: Box | null = maybe()?.next` with
  TS2322: an optional chain has type `T | undefined`. The request
  stated that `tsc` accepts `?.`; that holds only where `undefined`
  binds no name. §82.3 admits `?.` in those two positions only.
- Compound assignment on a plain field, an array element, and a local
  already ran at the pin. The rejection covered accessors and class
  index signatures only.
- S015 is retired (§33.4), so the new codes are S016–S018.
- The request's rule 4 for overloads fails for two numeric
  signatures: `tsc` sees one `number`. Recorded in
  `r39-overloads-deferred.md`.

## What landed

- §82.2: `RuleCode::S016/S017/S018`. Moved sites (compiler/src):
  S016 — `check/tyres.rs` unknown type name; `check/expr.rs` unknown
  name, unknown function, unknown class; `check/mod.rs` import of a
  missing export. S017 — `check/mod.rs` duplicate declaration in one
  scope / switch body, duplicate top-level name, duplicate function
  name, two accessors of one kind, every "cannot share the member
  name" clash, duplicate type parameter. S018 — `check/expr.rs` has
  no method (class, mirror, Worker, Inbox, Outbox, FixedArray,
  general receiver, async method), has no member (class, mirror,
  numeric, general receiver), enum has no member, class has no static
  member, generic class has no static member, static read accessor
  missing (two sites), Worker has no static method. Standard-library
  receivers keep their subset code (rule 3). Moved entries: r146,
  r149, r150, r151, r161, r162, r163, r164 → S017. New: r174, r175
  (S016), r176 (S018). r169 stays S100 (its pinned diagnostic under
  the mirror is the embedded-header copy).
- §82.1: `check_assign` and `check_update` rewrite to read-then-write
  in an expression statement and in a `for` update clause; a
  non-place receiver or index binds a `[[compound#N]]` local in the
  enclosing statement list; in an initializer with no statement list
  it is S100 with a block. a176 (`js-comparable: no C10`); r173;
  r130, r143, r144 retired (`retired:` markers in C10/C12).
- §82.3: `??` and the two `?.` positions; `Divergence` variants
  `NullishNonNullable`, `OptionalChainNonNullable`,
  `OptionalChainUnbound`, `OptionalChainIndex`, `NullishAssignment`,
  `NonPlaceNullishInitializer` under C7. a177 (`js-comparable: yes`);
  r177, r178. No `codegen/src/` change.
- §82.4: `ClassSig` template maps per namespace; `instantiate_method`
  beside `instantiate_fn`; a bodiless template and a duplicate type
  parameter fail at collection (rule 1a, both for methods and free
  functions); three `Divergence` variants citing §64. a178
  (`js-comparable: no C2 C8`, the `@CStruct` decorator has no
  JavaScript shim, as a143); r179–r181. A template read as a value
  reports the method-as-value S100.
- §82.5: the static read-accessor rejection removed; a179
  (`js-comparable: yes`); r147 retired.
- §82.6: `tools/downstream.sh`.
- Counts: accept `.ts` 173 → 177; `.expected` 174 → 178; rejects
  166 → 171.

## Gates (this host, HEAD `61c31e9`)

- release: 64 suites, 1,229 passed, 0 failed, 1 ignored.
- debug (the coding agent's final run): 64 suites, 1,231 passed, 0
  failed, 1 ignored, 2,445 s.
- `cargo build --all-targets`: 0 warnings. `cargo fmt --check`,
  `tsc`, `tools/hygiene.sh`: exit 0. Clippy 7 / 18 / 13.
- No pre-existing golden or `.expected` moved.
- Debug wall time across the six agent runs today: 1,190–3,216 s,
  against 476 s recorded for r37 on this host. Release: 358 s against
  245 s. Cause not measured in this round.

## Downstream gate (§82.6 item 6)

`tools/downstream.sh` at `ec41d65`, working tree clean: the downstream
harness stops at
`rejections::every_fixture_is_red_with_rule_and_owner` — fixture
`k28-half-builtin.ts` expects S100 for "`Vec3h` has no method `abs`",
which §82.2 moved to S018. That is the R39.4 change, not a defect; the
fixture moves to S018 at the re-pin. The other checker-owned fixtures
expect messages that did not move. 35 passed, 1 failed, 1 ignored in
the harness suite before the stop. The patch file and `Cargo.lock`
were restored on exit.

## Review round 1 (fresh no-context subagent)

§82.9 holds the record: 1 CRITICAL (a bodiless generic template
checked clean and ran; the free-function form had the same hole at the
pin), 4 MAJOR, 8 MINOR. Every finding was fixed in `61c31e9` against
the amended contract `dad652a`; the C14 shape the first implementation
accepted is a unit test that fails with the pin's message.

## Review round 2

TODO

## Coding agent

Rounds 1–3 and 5 ran on codex. Round 4 failed twice on codex ("model
at capacity"; partial edits restored from HEAD each time) and ran on a
Claude Opus subagent, as did round 6 (owner, 2026-09-02). The fix
round ran on codex and hit the 7200 s MCP timeout during its final
test run; the run survived, its log was read, and the gates were
re-run here.
