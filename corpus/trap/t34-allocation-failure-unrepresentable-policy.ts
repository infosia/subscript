// corpus: trap/t34-allocation-failure-unrepresentable-policy
// purpose: Records why the runtime's `Layout::from_size_align` allocation-failure raise remains unreachable.
// exercises: allocation-failure, policy-only, unrepresentable-layout
// questions: none
// tier-policy: policy-only; supported 64-bit codegen carries allocation sizes as u32, so every size reaching Context::alloc is representable; oversized FixedArray annotations overflow codegen layout arithmetic before runtime
// expected-trap: none; this entry is a non-runnable coverage record and its paired .expected is intentionally blank
