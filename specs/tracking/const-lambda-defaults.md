# Tracking: a call of a `const` lambda by its name takes its defaults (§288)

Contract: `specs/blocks/compiler/s288-a-call-of-a-const-lambda-by-its-name-takes-its-defaults.md`.
The owner selected §284.4 item 3 on 2026-11-14 and accepted the scope of
rules 1 to 4.

## Red

At the contract commit `fbdfbc67`, `subscript check` of
`a453-a-call-of-a-const-lambda-by-its-name-takes-its-defaults` exited 1
with S100 "`<name>` expects N argument(s), got M: a call through a
function value passes every parameter" at each call that omits an
argument: `top(2)`, the self-call `loc(k - 1)`, `loc(2)`, `earlier(3)`,
`earlier(3, 8)`, `effect()` (twice), and the async self-call.

## Form

- The checker marks a named lambda call where it resolves the callee
  (`compiler/src/check/function_value.rs`). The HIR call carries the
  checked defaults of the lambda.
- Lowering takes the declaration-default path
  (`codegen/src/lir/defaults.rs`, `codegen/src/lir/call.rs`); a lambda
  default is identified by the lambda.
- The divergence example of `FunctionValueOptionalArgumentCount` is now
  a call through a `let`.
- `a453` output, checked by hand:
  `t2 d2 d1 d0 3:4:5 3:8:9 default 7 9 default 7 a2 a1 a0`.

## Phase Review

One fresh review: one MAJOR and six MINOR findings; see §288.3.

- Rule 3a: `compiler/src/check/init_effects.rs` registers each omitted
  lambda default as a `ModuleFunction::Default` unit. The MAJOR program
  now gives one S100 "`DEFAULT` is accessed before its declaration,
  through `show (default of c)`".
- Each other pass that walks a call was audited. The passes read the
  default at its owner, or treat the call as a script effect, as for a
  declaration default. Only the initialization-order check and the
  duplicate default check changed.

## Gate

- `gate full 0de7979b dirty:44 debug 3416/3/5 release 3397/3/5 skips 1/0 clippy 4/10/13 goldens-moved 1 exit 1`.
  The reject entry `r376-function-value-default` used
  `const h = (a: i32, b: i32 = 5): i32 => a + b; h(1);`, which rule 1
  accepts. Three tests failed on it: `corpus_reject.rs` (two) and
  `retired_reasons.rs`. The entry now uses `let h`, the form of rule 4,
  and cites §288 rule 4. `tsc` accepts it, and the checker gives the
  S100 of its header at line 10. `generated-docs/corpus-index.md` is
  regenerated for its questions column.
- `gate full 0de7979b dirty:45 debug 3419/0/5 release 3400/0/5 skips 1/0 clippy 4/10/13 goldens-moved 1 exit 0`.
  The moved golden is the LIR text snapshot, which grows by the `a453`
  section only.

## §291: a default reads a captured local, and an async default fails the handle

Contract: `specs/blocks/compiler/s291-a-default-reads-a-captured-local-and-an-async-default-fails-the-handle.md`.

### Red

At the contract commit, `subscript check` of `a455` gave three S100 "a
named lambda default cannot read captured local", for `k` (14:25),
`text` (18:28), and `box` (21:25).

### Form

- The checker records the captures that the defaults of a lambda read
  as captures of the lambda. A named lambda call that omits an argument
  evaluates the default with the environment of the called value. The
  §288 rule 3 S100, its divergence block, its rejection target, and its
  witness are removed.
- `codegen/src/lir/async_defaults.rs`: for an async callee, the caller
  evaluates the omitted defaults in a region that catches an exception.
  If no default throws, the call is the direct call of the pin. If a
  default throws, the call result is the handle of a synthesized async
  function that throws the caught exception at its first instruction.
- `a455` output, by hand, on the three tiers, and under `node`:
  `6 8 captured 9 7 6 1:5 0:5 first fail after function call caught failed first fail after lambda call caught failed`.
- The program of finding 2 printed the order of `node` and the pin on
  the three tiers after the fix.
- `codegen/tests/lambda_default_environment.rs` measured 0.70 s.

### Phase Review

One fresh review, then one review of the form that fixed findings 1 and
2. See §291.3.

### Gate

- The first full gate failed on the codegen clippy baseline: 16
  warnings, the baseline is 13. Two functions had eight arguments and
  one type was too complex. A struct and a type alias replace them.
- `gate full 9171d9fd dirty:45 debug 3438/0/5 release 3419/0/5 skips 1/0 clippy 4/10/13 goldens-moved 1 exit 0`.
  The moved golden is the LIR text snapshot, which grows by the `a455`
  section only.
