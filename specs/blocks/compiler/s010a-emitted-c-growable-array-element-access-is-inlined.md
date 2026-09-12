<!-- §10a of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 10a. Emitted-C growable-array element access is inlined

The ship-tier C emitter (§11) lowered a **growable-array** element access to
an opaque call into the runtime staticlib (`subscript_rt_array_ptr`), which the
host C compiler cannot inline, cannot prove the bounds branch of, and around
which it will not vectorize. The **FixedArray** path was already inlined
(`base + idx*elem`), so only growable arrays paid this. Measured on
x86-64/Windows the opaque call alone cost ≈18% of the a22 emitted-C time
(17.2→14.0 ms at `-O2`); on arm64 it is latent because the surrounding loops
vectorize regardless — which is why the gap showed up only on x86
(`specs/tracking/windows-portability.md`).

The emitter now inlines the **in-bounds fast path** — a header-layout
pointer computation `data + (int64)idx*elem_size` guarded by an inlined
`0 <= idx < len` branch — and delegates only the **out-of-bounds case** to
`subscript_rt_array_ptr`, so the trap and its exact dynamic message stay
byte-identical to the runtime path (the runtime is still the sole producer
of the trap). This mirrors the FixedArray inline form and the standard AOT
technique of keeping the bounds check but making array element access a
header-inlinable operation rather than an opaque out-of-line call. This is
a **C-emitter** optimization only; the dev-JIT tier is
unchanged (it targets iteration speed, invariant 3), and dev-JIT ≡ ship-C
equivalence is preserved by construction.

The emitted C mirrors the runtime's committed array ABI — `ArrayHeader`
(`runtime/src/context.rs`, `#[repr(C)]`):
`{ u64 len; u64 cap; u64 elem_size; u8* data; }`, invariant 1 — as a C
`SsArrayHeader` typedef. The coupling is machine-verified two ways: a runtime
test pins the `ArrayHeader` field offsets (0/8/16/24), and the standing
differential gate (dev-JIT ≡ ship-C-AOT ≡ golden, byte-exact) fails
immediately on any layout drift, since a wrong offset reads garbage.
Closing the x86 gap the rest of the way (to the arm64 1.05×) is a value-copy
/ SIMD concern beyond this change, tracked separately.
