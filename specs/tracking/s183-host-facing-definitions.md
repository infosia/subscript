# §183: host-facing definitions

## Red at HEAD

Revision: `a87ffa48`.
The debug CLI was built from the unchanged revision.

Input:

```ts
export function step(dt: f32, paused: boolean): void { if (!paused) { print(`${dt}`); } }
export function main(): void {}
```

`subscript build` exits 2. The C diagnostic is:

```text
program.c:162:6: error: conflicting types for 'subscript_export_step'
program.h:567:6: note: previous declaration is here
1 error generated.
subscript: compiling/linking the emitted C failed with exit status: 1
```

Measured declaration:

```c
void subscript_export_step(subscript_rt_context* ctx, float a0, bool a1);
```

Measured definition:

```c
void subscript_export_step(subscript_rt_context* ctx, float a0, int32_t a1) {
```

The boundary scalar test against the emitter at `a87ffa48` fails with
exit status 101. The C diagnostic and test result are:

```text
program.c:495:6: error: conflicting types for 'subscript_export_take_boolean'
  495 | void subscript_export_take_boolean(subscript_rt_context* ctx, int32_t a0) {
      |      ^
program.h:565:6: note: previous declaration is here
  565 | void subscript_export_take_boolean(subscript_rt_context* ctx, bool a0);
      |      ^
1 error generated.
test every_boundary_scalar_export_matches_dev_and_c_checks_a_wrong_definition ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

## Definition sites

- `subscript_export_<name>`: `codegen/src/cemit/emitter.rs` emits host declarations and definitions. At Red, their boolean parameter types differ.
- `subscript_init`: the emitter defines the runtime header's module initializer with its declared Context parameter and void result.
- `subscript_kick_async_exports`: the emitter declares and defines the zero-parameter script-root driver with a Context parameter and void result.
- `subscript_worker_entry<index>`: the emitter defines the runtime worker entry with Context, inbox, and outbox pointers and a void result.
- `sub_f<id>_resume`: the emitter defines coroutine callbacks with three pointer parameters and a `uint8_t` result, as `SubAsyncResume` declares.
- Bound-header callbacks use `subscript_rt_cb_trampoline` or `subscript_rt_cb_registration_trampoline`. The runtime defines them; the emitter takes their addresses.

## Callback form gap

The bound callback cast is outside this change; see §183.4 item 1.

## Test cost and acceptance

`codegen/tests/host_export_boundary.rs` derives its sweep from `subscript_boundary::KINDS`.
One module per tier covers 12 scalar kinds and 24 host arguments.
The C host and the dev host print byte-identical values for each kind.
Each witness also has a handwritten literal expected print value, checked
against both tiers.
The boolean arguments are `true` and `false`.
The f16 host arguments contain binary16 bits, as the emitted header declares:
`0x3e00` prints `1.5`, and `0xc100` prints `-2.5`.
The u16, u32, and u64 witnesses include `65535`, `4294967295`, and
`18446744073709551615`, respectively, setting each unsigned high bit.

The firing control changes only the boolean wrapper definition from `bool` to `int32_t`.
The C compiler rejects the definition and names `subscript_export_take_boolean`.

The isolated sweep and control cost 0.607834 seconds on Apple arm64, excluding the Rust build.
The test performs one C link, one host run, and one rejected C syntax check.

The shared ship/test C flags and the separate C-emitting test helper's
MSVC flags include `/we4028` and `/we4029`. MSVC documents C4028 for
parameter type mismatches and C4029 for parameter count mismatches;
these flags promote those warnings to errors. The x86_64-pc-windows-msvc
gate at `0fb07e33` measured C4028: the firing control's wrapper type change
fails the syntax check with C4028 under `/we4028`. The MSVC diagnostic
does not name the function, so the control on MSVC reads `C4028`.
No test changes the parameter count, so C4029 is from documentation.

Measured tutorial wrapper:

```c
void subscript_export_step(subscript_rt_context* ctx, int32_t a0, float a1, bool a2) {
    int32_t c2 = a2;
    sub_f1(ctx, a0, a1, c2);
}
```

The wrapper reads its parameter types from the host signature through `ctype`.
The boolean conversion uses the local `int32_t` carrier inside the body.
No `.expected` output changed.
