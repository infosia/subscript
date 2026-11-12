# A parameter default discharges its handle — `compiler.md` §257

Status: COMPLETE.

## Red at the pin

The contract pin is `9526eaa8`. The Red run used the debug CLI of the
code of `6640bcf1`; both commits change specs only after `ce24838e`.
The run used a `$TMPDIR` copy of the first form of `a441`, which also
had the lambda row:

`subscript check a441-a-parameter-default-discharges-its-handle.ts`
gave 7 errors, each at a default:

```
error[S013]: an async handle is dropped without any await of its completion
 --> x.ts:17:38
error[S013]: an async handle is dropped without any await of its completion
 --> x.ts:21:38
error[S013]: an async handle is dropped without any await of its completion
 --> x.ts:25:41
error[S013]: an async handle is dropped without any await of its completion
 --> x.ts:30:29
error[S013]: an async handle is dropped without any await of its completion
 --> x.ts:35:36
error[S013]: an async handle is dropped without any await of its completion
 --> x.ts:37:42
error[S013]: an async handle is dropped without any await of its completion
 --> x.ts:37:47
error: 7 error(s)
```

`tsc` 5.9.2 accepts `a441`. `node` prints the `.expected` output.

## The lambda row of the table

The first table called the lambda as `await f()`. At the pin that call
is S100: "`f` expects 1 argument(s), got 0: a call through a function
value passes every parameter". An immediately called lambda gives the
same S100. `tsc` accepts both. The first `a441` called `await f(v())`.

The quick gate of that form failed:

`gate quick 6640bcf17148b01b71aed269a411c3278eef0346 dirty:9 debug 3331/1/5 skips 2 goldens-moved 1 exit 1`

`every_hir_execution_fact_is_carried_by_lir` found two dropped facts:

```
a441-a-parameter-default-discharges-its-handle.ts:35:36: trap "Call" carries 0 site(s); HIR requires 1
a441-a-parameter-default-discharges-its-handle.ts:35:36: call operand count 0 is absent from LIR
```

35:36 is the `v()` of the lambda default. A lambda default never runs
(§257.4 item 1), and LIR does not lower it. The amended contract
removes the lambda row. `a441` has no lambda now. The unit test keeps
the lambda form, because the checker still discharges its default.

## The change

`discharge_parameter_default` (`compiler/src/check/expr/entry.rs`)
discharges the origins of a checked default when the parameter type
transfers origins, as the argument path does. The function default
(`bind_params`), the constructor default, and the lambda default
(`check_lambda_body`) call it. The origin of the parameter stays (rule 2).

A default of an unannotated parameter (`p = v()`) gives no S013 at the
default, at the pin and after the change. If the body does not
discharge `p` (`return 1;`), S013 is at `p`.

## Measurements

- `compiler/tests/parameter_default_handle.rs`: 3 tests, 0.02 s. With
  the discharge turned off, all three fail: the table forms are
  rejected, and the two S013 tests find `(1, 38)` beside the expected
  site.
- `a441` gives the same output on the development tier, the ship tier,
  and `node`. `tsc` accepts it.
- The LIR text snapshot grows by the `a441` block only (504 lines).

## Phase Review

- MINOR 1 (open, §257.4 item 2): a default that reads an earlier
  parameter (`q: Promise<i32> = p`) discharges `p`. If the body passes
  `q` on and drops `p`, the rejected drop becomes a runtime trap.
- MINOR 2 (fixed): the unit test had no constructor form. It now has
  one, and it fails without the constructor discharge.
- MINOR 3 (fixed): the note said an unannotated default gives no S013.
  It gives no S013 at the default; S013 at `p` stays.
