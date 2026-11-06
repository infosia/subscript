# A callback outside a host entry is a host entry — `compiler.md` §224

Status: COMPLETE.

## Red at the pin `14b171a1`

`codegen/tests/host_entry_reentry.rs` gained two cases. The program
passes an `EngineSink` to the foreign function `engineStore`. The C host
keeps it and calls it after `subscript_export_store` returns. For a
message of length 7 the callback calls `hostReenter()`, which calls
`subscript_export_inner`. The earlier cases did not change.

```
[export-in-stored-callback]
stored 7
inner
stored-after
trap 0
[stored-callback-then-export]
stored 3
inner
trap 0
```

## After the change

```
[export-in-stored-callback]
stored 7
trap 34 host entry `subscript_export_inner` called during a host entry of the same Context
[stored-callback-then-export]
stored 3
inner
trap 0
```

The second case is the control: a stored callback with no foreign call
runs, and an export after it runs, so the flag is lowered.

## Form

- Both trampolines run through `fire_callback`, which reads the Context
  from the binding. `Context::begin_callback_entry` raises the flag only
  if no host entry runs. `fire_callback` lowers it only if it raised it.
- An early return of `fire_callback` runs no script code and leaves the
  flag alone.
- A callback outside a host entry has no outer entry to stop. Its trap
  stays on the Context, and the host reads it with
  `subscript_rt_ctx_trap_kind`.
- The development tier binds the same trampolines. Its exports do not
  raise the flag, so each dev-tier callback raises it for its run. The
  only effect is that `subscript_rt_ctx_async_step` and `_budget` trap
  during a dev-tier callback.

## Gate

`gate quick 14b171a1 dirty:4 debug 3167/0/5 skips 2 goldens-moved 0 exit 0`
