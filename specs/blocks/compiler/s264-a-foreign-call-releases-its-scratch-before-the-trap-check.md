<!-- §264 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 264. A foreign call releases its scratch before the trap check

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R13.
Selected by the owner on 2026-11-13.

Problem, read at `d149d5b9`. A foreign call that marshals a boundary
argument takes a scratch mark, allocates scratch storage for the
argument (`Context::boundary_scratch_alloc`), calls the foreign
function, writes the results back, and releases the scratch to the
mark. The foreign function can call a script callback, and the callback
can raise, so the call returns with a pending trap.

- The ship tier (`codegen/src/cemit/call.rs`) writes back only when no
  trap is pending, then releases the scratch, then checks the trap.
- The development tier (`codegen/src/lower/func/boundary.rs`) checks the
  `Call` trap first. A pending trap branches to the handler or to the
  unwind block, so the write-back and the release do not run.

Scratch storage is not managed memory, and a collection does not free
it. The Context frees the rest only when it drops. An exception that
leaves a callback becomes an `UncaughtException` trap of the call, so
the script cannot catch it, and the trap ends the host call. A host of
the development tier that keeps its session, as `ReloadSession` does,
clears the trap at the next call (§8.2) and calls again. Each such call
keeps its scratch for the life of the Context, so a host loop that
calls such an export each frame grows without a bound.

### 264.1 Rules

1. After a foreign call returns, the two tiers do these steps in this
   order: the write-back of the boundary results, only if no trap is
   pending; the release of the scratch to the mark; the check of the
   `Call` trap.
2. Each exit after the scratch mark releases the scratch once: the
   normal path and the path of a pending trap.
3. If no trap is pending, the output and the write-back do not change.
4. `ReloadSession` reports the count of live scratch allocations of its
   Context, beside `live_allocations`. A test reads the count of rule 2
   there.

### 264.2 Acceptance

1. Red first, at the pin: a test makes a `ReloadSession` with an export
   that calls a foreign function. The function takes a boundary
   argument with scratch storage and calls a script callback that
   throws. The test calls the export three times; each call reports the
   trap. After the calls, the count of rule 4 is not 0. The tracking
   note records the count.
2. After the change, the count is 0 after each call. The ship tier
   gives the same trap for one call, as a control.
3. A control: the same call with a callback that does not throw writes
   back the result, in the two tiers.
4. No corpus entry or golden changes. If a test of `boundary_scratch`
   changes, report it and stop.

### 264.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each form of a foreign call in the
development tier: a `void` result, a scalar result, a struct result,
nested targets, and a call with no scratch mark. Each now writes back
only with no pending trap, releases, and then checks the trap, as the
ship tier does. The pending state has one reader.

Before the implementation, the coding agent stopped four times: no
fixture had a foreign function with a scratch argument and a callback,
no public value gave the scratch count, a new test needed an entry in
`codegen/Cargo.toml`, and a callback exception is an
`UncaughtException` trap that the script cannot catch. The last stop
corrected the problem text and acceptance items 1 and 2.

1. MINOR, contrived, fixed: the write-back of a string view checked
   the trap of its allocation, and so left before the release.
2. MINOR, contrived, recorded: §264.4 item 1.
3. MINOR, realistic, fixed: the development tier read the pending
   state at each foreign call. It now reads it only for a call with a
   write-back or a target, as the ship tier does.
4. MINOR, realistic, fixed: the test ran on Unix only.
5. MINOR, fixed: the form of two comments.

### 264.4 Open

1. If the allocation of a scratch argument fails, the two tiers leave
   the call after the mark with no release. The scratch that the call
   allocated before stays until the Context drops.
