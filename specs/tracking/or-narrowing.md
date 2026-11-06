# A fact on both arms of `||` narrows — `compiler.md` §227

Status: COMPLETE.

## Red at the pin `62de4861`

The debug CLI built from the pin, on a copy of each entry:

- `subscript check a416-or-narrows-on-both-arms.ts` gave 8 diagnostics,
  each `error[S011]: ... may be null here; narrow with a null check
  first`, then `error: 8 error(s)`. The sites are `this.f.length` (a
  shared field), the `if`, the `&&` false form, `!( || )`, three arms,
  the conditional expression, `a !== null || a === "y"`, and the class
  form `a === b || a !== null`.
- `subscript check r484-or-fact-on-one-arm.ts` gave S011 at 10:12.

## After the change

`a416` checks with no errors. The development tier prints:

```
orIf:1:1:-1:-1
andFalse:2:1:-1:-1
notOr:2:1:-1:-1
threeArms:1:2:3:-1:-1
conditional:1:2:0:0
presentOrEqual:4:-1
classOr:5:6:-1
fieldOr:1:1:-1:-1
elementOr:1:1:-1:-1
```

Each function runs the left arm, the right arm, and neither arm. The ship
tier and `node` give the same bytes. `r484` stays S011 at 10:12. `tsc`
5.9.2 gives `TS18047: 'a' is possibly 'null'` at 10:12.

## Form

- Both fact kinds state presence. `Narrowing` comes from a null test,
  from equality with a value that has no absence (§219.1 rule 4), and
  from the `Error` test of `instanceof`. `ElementCheck` selects a
  diagnostic only.
- `narrowing_paths` reads each operand once and keeps both results. The
  side that existed before keeps its `end_facts_at` call, so the effect
  on the function context does not change.
- An `ElementCheck` fact changes no type. A unit test pins it through
  the diagnostic it selects.

## Gate

`gate quick 62de4861 dirty:6 debug 3174/0/5 skips 2 goldens-moved 0 exit 0`

## Phase Review

No CRITICAL or MAJOR finding. Two MINOR findings on the test strength of
`a416` are recorded in §227.3. Hygiene: clean.
