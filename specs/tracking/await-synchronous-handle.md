# `await` reads the handle of a synchronous call — `compiler.md` §234

Status: COMPLETE.

## Red at the pin `4afd673b`

The debug CLI built from the code of the pin (`56798d52`):
`subscript check a421-await-a-synchronous-handle-call.ts` gave 6 errors,
each S100, then `error: 6 error(s)`:

| Line:col | Message |
|---|---|
| 44:27 | `s` is synchronous and cannot be awaited (in a template) |
| 45:32 | method `s` is synchronous and cannot be awaited |
| 46:24 | class `K` used as a value |
| 47:24 | `sn` is synchronous and cannot be awaited (with an argument) |
| 49:9 | `s` is synchronous and cannot be awaited (expression statement) |
| 50:22 | `sg<P>` is synchronous and cannot be awaited (explicit type argument) |

## After the change

`a421` checks with no errors. The development tier prints:

```
template:1
method:20:static:30:argument:14
generic:7
instantiated:5:8:alias:9
```

The ship tier, the interpreter, and `node` give the same bytes.

## Form

- The checker decides from the result of the instance: a synchronous
  function, method, or static method whose instance result is a handle.
  `first<Promise<i32>>`, `pick<Promise<i32>>`, and a result alias of
  `Promise<i32>` are accepted. The Phase Review found that a first
  version read the declaration, and rejected these.
- `check_await_call` in `compiler/src/check/expr/entry.rs` then checks
  the call as an ordinary call with no context, and awaits its handle,
  as for a handle in a local (rule 3). §215.1 rule 3a holds (rule 4).
- A synchronous call with no handle result does not reach this path. The
  witnesses `a-s095` and `a-s100` are such forms and still fire.
- `a421` has an async entry, so the LIR text snapshot grows by its block
  only.

## Gate

`gate quick 4afd673b dirty:8 debug 3210/0/5 skips 2 goldens-moved 1 exit 0`

After the Phase Review fixes, one run:

`gate quick 4afd673b dirty:10 debug 3212/0/5 skips 2 goldens-moved 1 exit 0`

The moved golden is the LIR text snapshot, which grows by the `a421`
block.

## Phase Review

Three MINOR findings, each fixed, recorded in §234.3. Hygiene: clean.
