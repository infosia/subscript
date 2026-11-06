# A host entry during a host entry traps — `compiler.md` §223

Status: COMPLETE.

## Red at the pin `acd866ef`

`codegen/tests/host_entry_reentry.rs` runs one program and one C host.
Each case has its own Context. Output with no production change:

```
[export-in-foreign-call]
before
inner
after
trap 0
[export-after-outer-entry]
before
inner
after
inner
trap 0
[callback-in-foreign-call]
cb 3
callback-after
trap 0
[step-in-foreign-call]
step-before
step-after
trap 0
```

The inner export runs, the inner step returns, and both outer entries
continue.

## After the change

```
[export-in-foreign-call]
before
trap 34 host entry `subscript_export_inner` called during a host entry of the same Context
[export-after-outer-entry]
before
inner
trap 0
[init-in-foreign-call]
init-before
trap 34 host entry `subscript_init` called during a host entry of the same Context
[parameter-export-in-foreign-call]
take-before
trap 34 host entry `subscript_export_take` called during a host entry of the same Context
[parameter-export-after-outer-entry]
take 5
trap 0
[callback-in-foreign-call]
cb 3
callback-after
trap 0
[step-in-foreign-call]
step-before
trap 34 host entry `subscript_rt_ctx_async_step` called during a host entry of the same Context
```

The Red run at the pin had no `init` case and no `take` case. The Phase
Review added them.

In the second case the outer `main` traps first. The host clears the
trap, and then the foreign function runs `inner` outside a host entry.

The emitted C checks the trap after each foreign call, so the outer entry
stops (acceptance 5). Neither `after` nor `step-after` prints.

## Form

- The flag is a `u32` at offset 24 of the Context prefix, after the
  globals pointer. Generated C reads it at a fixed offset, as it reads
  the trap kind at 0 and the globals pointer at 16.
- Each C entry is a public guard around a static body: `sub_host_<name>`
  for an export, `sub_module_init` for the initializer.
- The two step functions guard in Rust.
- `subscript_kick_async_exports` has no guard of its own. It calls the
  public export of each async root, and each call raises and lowers the
  flag.
- A refused entry skips the entry validation of wire-alias parameters.
- If the Context already holds a trap, the inner call runs no script code
  and the first trap record stays.

## Gate

The first quick gate failed with 3 failures: `a23-game-loop` and the
capstone example export `init`, and the first name of the initializer body
was also `sub_host_init`. The body became `sub_module_init`, and the test
program exports `init`. The one re-run:

`gate quick acd866ef dirty:12 debug 3166/0/5 skips 2 goldens-moved 0 exit 0`

After the Phase Review fixes, one run:

`gate quick acd866ef dirty:13 debug 3166/0/5 skips 2 goldens-moved 0 exit 0`

The new test compiles and runs one C program, about 2 s.
