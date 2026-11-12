# A default call takes its own count of each argument — `compiler.md` §256

Status: COMPLETE.

## Red at the pin

The debug CLI of the code of `98bf9af8` (the contract commit changed
specs only after `1c2a6a78`), run on a `$TMPDIR` copy:

`subscript run a440-a-default-call-takes-its-own-count-of-each-argument.ts`
stopped with:

```
subscript: internal lowering error: LIR construction failed: :0:0: produced invalid LIR:
function 9 (`main`): block 0 instruction 1 stores counted operand 0 (value 0) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 1 instruction 5 stores counted operand 1 (value 5) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 2 instruction 5 stores counted operand 0 (value 10) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 6 instruction 3 stores counted operand 0 (value 28) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 9 instruction 4 stores counted operand 0 (value 32) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 11 instruction 5 stores counted operand 0 (value 37) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 13 instruction 4 stores counted operand 0 (value 41) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 13 instruction 10 stores counted operand 0 (value 45) without a fresh single-use owner or a preceding retain
function 9 (`main`): block 13 instruction 20 stores counted operand 0 (value 51) without a fresh single-use owner or a preceding retain
```

Each finding is a default call that stores a fresh argument, and the
callee takes the same value after it. `tsc` 5.9.2 accepts `a440`.

## The form of rule 1 and the verifier

The first form retained a value with one owner once, before the default
call, and left its own count for the callee. The counts of that form
balance: the default call takes the retained count, and the callee takes
the own count. The LIR verifier rejects it: `verify_counted_stores`
takes a fresh owner for a store only when the value has one use, and
the value then has three uses (the retain, the default call, the
callee). The verifier stopped each callee store with "stores counted
operand 0 (value 0) without a fresh single-use owner or a preceding
retain". A retain with a result gave a fresh copy for the default call,
but the original value still had two uses, and the verifier stopped the
callee store the same way.

The form that the verifier accepts: a value with one owner is retained
once for each pass (two retains), and its own count is released at
once, before the default call. Each store then has a preceding retain.
No count lives past the release, so no owner that the release model
(§255 rule 4) does not know crosses an exception edge.

## After the change

`a440` checks with no errors. The development tier, the ship tier, the
interpreter, and `node` give the same bytes: `w 3`, `method 3`,
`push 3`, `all 0`, `all empty 0`, `all one 1`, `take 11`, `take3 17`,
`cnt 4`, one per line. The release model reports no finding on `a440`
and on the corpus (the `lir` test, 66 tests).

Rule 2, measured with the memory accounting of the development tier,
with the diagnostics of a freed handle on: `a440` leaves 341 live bytes,
and the same program with each default given as an explicit argument
(`w(v(), 2)`, `all([], 0)`, `take(gen(), 1)`, and the others) leaves
341 live bytes. No diagnostic of a freed handle fires.

A later default with a conditional, `w4(p, k = 2, m = k > 1 ? 3 : 4)`
and `w5(p, b = true, k = b ? 3 : 4)`, runs and prints `6`, `4`, `5`.

`main` of `a440` is async, so the LIR text snapshot grows by the `a440`
block only (806 lines added, none removed).

## Unit test

`codegen/tests/default_call_count.rs`: the lowering of `w(v())`,
`take(gen())`, and `cnt([v(), v()])` passes the LIR verifier. At the pin
the first form stopped with "function 6 (`main`): block 0 instruction 1
stores counted operand 0 (value 0) without a fresh single-use owner or a
preceding retain". The control `const h = v(); w(h)` has two retains and
two releases in `main`, at the pin and after the change. Cost: 0.04 s,
no native build.

## Phase Review

- MINOR 1, recorded as §256.4 item 3: a `@Descriptor` default whose
  omitted field has a raising initializer leaks one retained count on
  each raise, as the explicit form already does under §255 rule 1 site
  2. `runs_user_code` does not look into the initializer of an omitted
  field of a `DescriptorLit`.
- MINOR 2, fixed: the unit test had no check of rule 2 that can fail.
  `each_default_call_takes_two_retains_and_one_release` asserts the
  count instructions of `main`: `w(v())` has 2 retains and 1 release,
  and `take3(gen())` with two omitted defaults has 4 retains and 2
  releases. With the release removed from `defaults.rs`, the test
  failed: `w(v())` gave 2 retains and 0 releases. The release is back.
- MINOR 3, fixed: the comment in `defaults.rs` states rule 1 as the
  contract states it.

## Files

- `codegen/src/lir/defaults.rs`
- `codegen/tests/default_call_count.rs` (new)
- `corpus/accept/a440-a-default-call-takes-its-own-count-of-each-argument.ts`
  and `.expected`
- `codegen/tests/lir-goldens/corpus.txt` (the `a440` block),
  `generated-docs/corpus-index.md`
