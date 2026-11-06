# Two nullable references compare by identity — `compiler.md` §228

Status: COMPLETE.

## Red at the pin `28ccd17b`

The debug CLI built from the pin, on a copy of the entry:
`subscript check a417-nullable-references-compare.ts` gave 11 errors,
each S100 "operator `<op>` not defined for `<T> | null` and `<T> | null`",
then `error: 11 error(s)`. The sites are `===`, `!==`, `==`, and `!=` on
`C | null`, `N | null` fields, `C | undefined` results of `Map.get`,
`Map<i32, i32> | null`, and `==` and `!=` of `C | null` against
`C | undefined`.

## After the change

`a417` checks with no errors. The development tier prints:

```
strictEq:true:false:false:true:false
strictNe:false:true:true:false:true
looseEq:true:false:false:true:false
looseNe:false:true:true:false:true
linked:true:false:true:false
mapGet:true:false:true:false
mapGetNe:false:true:false:true
mapsEq:true:false:true:false
mapsLooseNe:false:true:false:true
acrossEq:true:false:false:true:false
acrossNe:false:true:true:false:true
```

For the parameter forms the columns are both absent, the left absent,
the right absent, the same object, and two objects. The ship tier, the
interpreter, and `node` give the same bytes.

## Form

- `bin_result` in `compiler/src/check/expr/operator.rs` admits a pair
  with the predicate that admits `T | null` against `T`. Rule 1 is
  therefore a derivation. That predicate holds for a reference class,
  `Map`, `Set`, and `Error`, and does not hold for an array.
- A `===` or `!==` pair of two absence spellings stops earlier with
  S012 (§195), so rule 4 needs no new check.
- No lowering changed. Each tier lowered the pair correctly.
- Two existing tests asserted the old S100 on a pair. Each now asserts
  rule 1, and each keeps an S100 control of two different types.

## Gate

`gate quick 28ccd17b dirty:6 debug 3178/0/5 skips 2 goldens-moved 0 exit 0`

## Phase Review

No CRITICAL or MAJOR finding. Two MINOR findings are recorded in §228.3.
Hygiene: clean.
