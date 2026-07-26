// corpus: trap/t26-allocation-failure-policy
// purpose: Records why allocation-failure trap tuples are not compared across tiers.
// exercises: allocation-failure, policy-only
// questions: none
// tier-policy: policy-only; allocation failure is not safely or deterministically source-reachable without allocator fault injection
// expected-trap: none; this entry is a non-runnable coverage record and its paired .expected is intentionally blank
