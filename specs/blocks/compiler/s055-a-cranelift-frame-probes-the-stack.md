<!-- §55 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 55. A Cranelift frame probes the stack

Measured on `x86_64-pc-windows-msvc` 2026-08-10, toolchain 1.95.0.
`codegen/tests/boundary_scratch_breadth.rs` ends with
`STATUS_ACCESS_VIOLATION` (`0xc0000005`) in the release profile. The
dev profile passes. The defect is older than the run: a worktree at
`085ce32` fails the same way.

Cause: `dev_flags()` and `aot_flags()` set `opt_level` and `is_pic`
only. Cranelift emits no stack probe by default. Windows reserves a
thread stack and commits it one page at a time. A page becomes
committed only when the program touches the guard page that precedes
it. A frame larger than one page moves the stack pointer past the
guard page, and the first write to that frame faults.

The host profile changes no generated code. It changes how much
stack the host commits before it calls into the generated code. The
dev profile commits more, so the large frame lands on committed
pages. One profile therefore hides the defect.

The test program is the §44.8 breadth fixture: 32 positions, each
with its own target, nested state, leaf, and arrays. Its `main`
frame is large. A host program with a large frame has the same
exposure, in any profile.

Evidence, one variable, worktree at `085ce32`, release profile:

| `dev_flags()` | result |
|---|---|
| the present two settings | `STATUS_ACCESS_VIOLATION` |
| plus `enable_probestack`, `probestack_strategy = "inline"` | 1 passed |
| the two settings again, patch reverted | `STATUS_ACCESS_VIOLATION` |

`RUST_MIN_STACK=67108864` does not change the result. The reserved
size is not the cause; the missing probe is.

### 55.1 Rule

`dev_flags()` and `aot_flags()` both set:

```
enable_probestack = true
probestack_strategy = "inline"
```

The strategy is `inline` because `outline` emits a call to
`__cranelift_probestack`. The dev tier resolves a symbol by absolute
address in the caller process, and the Cranelift object goes to a
host linker. Neither one supplies that symbol.

Cranelift emits a probe only for a frame larger than one page, so a
small function keeps its present code.

### 55.2 What does not change

- The emitted-C ship tier gives the frame to the platform C
  compiler. That compiler owns the probe. This rule does not reach
  it.
- Every golden stays byte-identical. A probe writes to stack that
  the frame owns. It computes nothing.
- The rule is target-independent. Cranelift applies the setting on
  every target, and the output is the same on all of them.

### 55.3 Exit criteria (pre-registered)

1. Record the red output first:
   `cargo test --offline --release -p subscript-codegen --test
   boundary_scratch_breadth` on windows-msvc.
2. The same command passes after the change.
3. A unit test reads both flag sets back. It asserts
   `enable_probestack` is true and the strategy is `inline` in each.
4. The workspace gate passes on windows-msvc in **both** profiles.
   The dev-profile-only gate is what hid this defect.
5. The workspace gate stays green on the reference machine, and
   every golden stays byte-identical.
6. `cargo fmt --check` exit 0.
