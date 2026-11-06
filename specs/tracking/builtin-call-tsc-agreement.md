# A built-in call is accepted only where `tsc` accepts it — `compiler.md` §230

Status: COMPLETE.

## Red at the pin `6b31e0d2`

The debug CLI built from the checker of the pin:

- `subscript check r486-reduce-literal-result-widens.ts`: `no errors`
- `subscript check r487-unread-type-argument.ts`: `no errors`

## After the change

- `r486`: S100 at 11:43, "the `reduce` callback result `"b"` has the type
  `string`, and `string` is not assignable to the accumulator `Mode`;
  `tsc` rejects the call (TS2769)", with the note "write
  `reduce<Mode>(...)`, or annotate the callback result:
  `(...): Mode => ...`". `tsc` gives TS2322 at 11:3 and TS2769 at 11:20.
- `r487`: S100 at 9:19, "`filter` takes no type arguments, got 1".
  `tsc` gives TS2345 at 9:25.

## Form

- Rule 1: `reject_widened_reduce_result` in
  `compiler/src/check/expr/type_arguments.rs`. It applies when the body,
  through parentheses and conditional branches, is one string literal
  value. Two different literals in a conditional stay accepted, because
  `tsc` keeps their union.
- Rule 3: an entry of a type argument list is read when the checker
  resolves it (`resolve_type_position` in `compiler/src/check/tyres.rs`).
  An entry that is not read after the call check gives the site
  `BuiltinTypeArgumentCount`. Each place where the checker checks a call
  runs `Checker::checked_call` in `compiler/src/check/expr/type_arguments.rs`:
  `check_call`, the awaited call, the `unreachable` statement, the
  `for…of` call subject, and the two optional-chain calls. A call of a
  user function, a user method, or a function value is outside rule 3.
- Rule 1a, measured with the CLI and `tsc`:

  | Callback | subscript | `tsc` |
  |---|---|---|
  | `(acc: Mode, x)`, init `"a"` or `m` | accepts | accepts |
  | `(acc: Mode, x: i32, i)`, init `"a"` or `m` | accepts | accepts |
  | `(acc: Mode, x: i32)` | rule 1 S100 | TS2769 |
  | `(acc: Mode)` | rule 1 S100 | TS2769 |
  | `(acc, x: i32)` | S100, `init` decides `string` | TS2322, TS2769 |
  | `(acc, x)` | S100, the same | TS2322, TS2769 |
- `Map.groupBy<K, T>` read no entry at the pin, and `tsc` accepts it.
  The checker now reads both entries, so rule 3 accepts it. The
  `generic_tsc_matrix` omission count stays 9541.
- `reduce<U>` and `reduceRight<U>` now read `U` as the accumulator type,
  so the `reduce<Mode>` neighbour stays accepted.

## Built-ins that read a type argument (rule 4)

| Built-in | Count |
|---|---|
| `map`, `flatMap`, `reduce`, `reduceRight` | 1 |
| `Map.groupBy` | 2 |
| `Array.of`, `Array.from` | 1 |
| `JSON.parse` | 1 |
| `Context.bytesOf`, `Context.bytesInto`, `Context.fromBytes` | 1 |
| `Worker.spawn` | 2 |

Each other built-in takes none. The generated language reference has
the same list.

## Gate

`gate quick 6b31e0d2 dirty:22 debug 3188/0/5 skips 2 goldens-moved 0 exit 0`

The contract commit was amended to `72b86f43` after the Phase Review.
After the fixes, one run:

`gate quick 72b86f43 dirty:25 debug 3191/0/5 skips 2 goldens-moved 0 exit 0`

## Phase Review

Seven MINOR findings, recorded in §230.3. Hygiene: clean.
