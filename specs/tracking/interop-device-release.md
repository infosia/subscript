# Interop device release

Contract: compiler.md §270 rules 1–4.

The device release path ends every one-shot slot and the subscription through their release helpers before free.
The helpers clear each slot.
The pump and immediate start retain the device until dispatch ends.

## Measurements

At contract pin `0dfe91af`, the fixture test started one deferred request and one subscription.
After the last device release, the Context held 2 open registrations. The expected count was 0.
The completion control passed: one callback ran, the device release count was 1, and the Context count was 0.
The Red run reported 1 failed test and 1 passed test. Cargo reported 1.40 seconds for the build.

After the change, the Context count after device release was 0. No pending callback ran.
The device stores its release count in its own allocation. No value survives free, so the test asserts the Context count after free.
The control reads a release count of 1 before free and a Context count of 0 before and after free.
The completion control cannot detect a second release attempt: an ended registration returns 0 and changes no count (§111 rule 5).
Rule 2 rests on the helpers that clear each slot.
The redundant test of both tiers and its main-thread list entry are removed.

The callback-release test covers a deferred request, an immediate request, and a subscription notification.
Each callback releases the last caller reference. Each run completes with 0 open registrations.
At contract pin `0dfe91af`, the deferred case failed: the Context count was 1 instead of 0; wall time 2.67 seconds.
With the round 1 cleanup and without dispatch retains, the immediate case failed: start returned 0 instead of 1.
That run read freed device memory; wall time 1.89 seconds.
The existing fixture tests do not enable AddressSanitizer, so this test uses completion and Context counts.

The other callback sites are SetLogger, Submit, DevicePump, DeviceWait, and ReleaseAndRefire.
None reads the device after its callback returns. FireOneShot runs only inside the retained start or request pump.

## Checks

Each time below is the command wall time, including Cargo work.

- `cargo test -p subscript-codegen --test interop_device_release`: PASS; 3 tests; 2.13 seconds.
- `cargo test -p subscript-codegen --test interop`: PASS; 21 tests; 3.95 seconds.
- `cargo fmt --check`: PASS; 2.03 seconds.
- `cargo clippy -p subscript-codegen --tests`: PASS; 4.85 seconds. No warning names a changed file.

## Test cost

`interop_device_release.rs` uses the shared native fixture build and direct calls in one test process.
It adds no C build or child process per test. Its three tests took less than 0.01 seconds after the Cargo build.
Its command cost was 2.13 seconds, including a fixture rebuild.

`interop.rs` checks dev-JIT and ship-C-AOT output with a C build and process per tier test.
Its 21 tests took 2.85 seconds; its command cost was 3.95 seconds.
The removed test added a C build and a process without a failure at the pin.

All existing comparisons of both tiers passed. No corpus entry output or golden changed.
