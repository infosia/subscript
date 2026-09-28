<!-- §121 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 121. A reload build gives every call a raise edge

*(Added 2026-09-28.)* Origin: the open item of §119.5 (`can_raise` is
not in the reload declaration hash). Forced: the measurement leaves one
candidate that keeps invariant 3.

Problem: a lambda that Context state holds keeps the code of the
generation that made it. A swap does not rerun the global initializer,
so the lambda is not replaced. If a body edit makes a callee start
raising, the old lambda has no raise edge at that call. Its `catch` is
skipped. Measured on the dev JIT at `558b231`
(`specs/tracking/can-raise-reload.md`, probe P4): a global lambda
`(): void => { try { f(); } catch (e) { print("caught"); } }`; version 2
changes only the body of `f` to `throw new Error("x")`. The swap
succeeds; the lambda prints nothing and the host gets trap 29
`UncaughtException` with `Error: x`.

The same measurement shows that the other shapes are correct today. A
swap compiles the whole module in a new generation, so every named
function gets the edges of the new facts: a direct call (P1), a `using`
scope (P2), and a transitive call (P3) print `caught`. A suspended async
frame (P5) traps `StaleCoroutine` (12), as §8.2 requires.

Candidates, measured on the same probes:

- A. Put `can_raise` in the declaration hash. Every probe refuses the
  swap, so every edit that makes a function raise costs a restart, and
  3 of 30 reload tests fail. This gives up the iteration tier for an
  everyday edit (invariant 3). Rejected.
- B. Recompile the callers whose edges change. HEAD already does this
  for named functions; it cannot reach a retained lambda. Not a fix.
- C. In a reload build, every call that has a `Call` site also has a
  raise edge. P4 prints `caught`; P1 to P3 do not change; P5 keeps its
  stale trap; 30 of 30 reload tests pass. Cost on the 260-entry corpus:
  dev-JIT compile median 772.560 → 781.327 ms (+1.13%), code size
  1,847,972 → 1,869,360 bytes (+1.16%). Release `perf-gate --gate`, 3
  runs of 11 samples: dev iteration 3.936 → 3.934 ms, hot reload
  0.500 → 0.501 ms. The success path stays one load and one compare.

### 121.1 Rules

1. In a module compiled for reload, every call that has a `Call` site
   also has a raise site with its handler edge: the handler of the
   enclosing `try`, or the propagate exit. This holds whatever the
   callee's `can_raise` is, in named functions, methods, constructors,
   and lambdas. A call that creates an async handle is not a raise site:
   its exception completes the handle, and the `await` of the handle is
   the raise site (§116.1 rules 1 and 2).
2. The ship C tier and a dev-JIT module compiled without reload keep
   §115.6 rule 3: a raise site only at a call to a callee that can
   raise.
3. The compile mode is an input of the one site derivation (§20.2,
   §115.6 rule 3). The LIR of a reload build carries the extra edges;
   each engine reads them. No engine adds an edge of its own. The LIR
   function fact `can_raise` of a reload build is `true` for every
   function, so the raise verifier accepts the extra edges; it is the
   HIR fact only in a build without reload.
4. The success path after a call stays one load and one compare
   (§115.6 rule 4). The extra edges change only the path taken when the
   word is nonzero.
5. The raise edge of a reload build does everything §115 and §116
   require of an exception exit: `using` disposal and the release of
   scope-owned handles (§116 rule 4b).
6. `can_raise` stays out of the declaration hash. §119.5's first item
   is closed by this section.
7. The LIR module does not carry its compile mode, so the verifier
   cannot check rule 1 by itself. Today one guard in the site
   derivation covers every call form. A call path added outside that
   derivation must bring the mode into the verifier and a total check
   with it.

### 121.2 Acceptance

1. Red first (core principle 10): a reload test with probe P4 fails at
   `558b231` and passes after the change. A lambda cannot hold a
   `using` resource (`r172-using-in-lambda`), and it borrows its
   captured handles (§116 rule 4c), so a retained lambda has nothing
   for rule 5 to release; rule 5 is tested through the named-function
   probe P2.
2. P1, P2, P3, and P5 keep their measured results as reload tests.
3. A test builds a reload-mode LIR module and a non-reload one from the
   same source: the reload module has a raise site at a call to a
   callee that cannot raise; the non-reload module has none.
4. The LIR text snapshot and the `.expected` goldens do not move: the
   snapshot is a non-reload build.
5. Record the cost again from the implementation (compile time, code
   size, the `perf-gate --gate` subjects), paired against `558b231`, in
   `specs/tracking/can-raise-reload.md`.
