<!-- §351 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 351. A return into a recursion reaches a poll before it allocates

*(Added 2026-11-22.)* Origin: a review of the poll placement by
another agent, and the measurement G8 of
`specs/tracking/automatic-collection.md`.

Problem, measured at `9a8fa86a`:

1. **The bound of §342 rule 11d does not hold on a chain of
   returns.** A function calls itself and then allocates. With a
   minimum headroom of 64 bytes, a call of depth 1,024 keeps 1,025
   objects and runs no automatic collection before the outermost
   exit, in the ship tier and the dev JIT. The count follows the
   depth, not the module.
2. **The argument counted the descent only.** The entry poll of a
   function in a call-graph cycle serves each call. After the deepest
   call returns, no poll is on the path of the continuations.
3. **The verifier does not see it.** It checks the cycles of one
   function and the entry polls. A lowered module with no poll after
   a recursive call passes.
4. **Three more shapes have the same gap**: a recursion through a
   function value, a cycle that a direct call closes after an
   indirect call, and a handler that allocates when an exception
   returns through the frames.

### 351.1 Rules

Terms. The **call graph with indirect edges** has an edge for each
direct call, and an edge from each function with an indirect call, or
with a runtime operation that takes a function value, to each function
whose address is taken. A **cycle** is a strongly connected component
of that graph that has an edge inside it. The **continuation** of a
call is the path from the instruction after the call.

1. **A poll after a call into the cycle of the caller (rule 11e of
   §342).** For each call in a function of a cycle whose possible
   callee is in that cycle: if a path of the continuation reaches an
   instruction that can allocate, or a suspension, before it reaches a
   poll, the placement puts one poll directly after the call. A path
   follows the handler edges of later instructions and ends at a
   return, a trap, or an unreachable point.
2. **A handler of a function of a cycle has a poll (rule 11f).** In
   each function of a cycle that can allocate, each handler block
   whose path can allocate before a poll gets one poll after the
   leading instructions that take the exception.
3. **A direct call to a function with an entry poll counts as a poll**
   in the scan of rules 1 and 2. So a function that calls itself two
   times and then allocates gets one poll, after the second call.
4. **A function outside each cycle gets no new poll.** A frame of it
   is on the stack one time at most.
5. **The entry poll rule uses the same graph.** A function that can
   allocate and is in a cycle of the graph with indirect edges has an
   entry poll (§342 rule 11b named the direct cycle and the functions
   whose address is taken; the set does not shrink).
6. **The bound.** Each path through LIR control flow reaches a poll,
   or the outermost exit, after a count of allocating instructions
   that only the module bounds, for direct calls, indirect calls,
   callback operations, normal returns, and exceptional returns. The
   module documentation of the placement pass states the argument
   with the chain of returns.
7. **Not in the bound**, each recorded in §351.4: a recursion that
   passes through the host; a value that passes up through nested
   generators; the allocations of one runtime operation (§345 rule
   3).
8. **The verifier checks rules 1, 2, and 5** by a derivation of its
   own: a backward fixpoint over block suffixes for "an allocation is
   reachable with no poll", against the forward scan of the placement.
   A module with one of those polls removed is rejected while its
   entry polls are present.
9. **`Coerce` of a reference class to its own nullable type cannot
   allocate.** The fact table of §342 rule 10 stated "can allocate"
   for it. With the correct fact, rule 1 adds no poll to a tree-shaped
   recursion.
10. **A value that the continuation reads is rooted at the poll**, as
    at each poll: the root storage follows the liveness of the
    finished LIR. No rule of the lowerings changes.
11. **An automatic collection stays not observable** (§342 rule 2):
    no golden of the corpus moves.

### 351.2 Acceptance

1. **Collection on the return path.** The fixture of the problem at
   depths 256 and 1,024, automatic mode, minimum headroom 64 bytes,
   sampled before the outermost exit, in the dev JIT and the ship
   tier: an automatic collection runs in each call, and the objects
   over the base are under a bound that the test derives from the
   object size and the headroom, equal for the two depths. Red at the
   pin: 257 and 1,025 objects, no collection. The test does not use
   mode 2: a collection at each poll collects on the descent and
   passes for a wrong reason.
2. **The other shapes**, in the same test form: two functions that
   call each other; a recursion through a function value; a cycle
   that a direct call closes after an indirect call, where only the
   continuation of the direct call allocates; a handler that
   allocates with a throw from the bottom. Red at the pin for each.
   Control: a recursion whose continuation does not allocate has no
   new poll and keeps one object.
3. **Verifier.** For each shape of items 1 and 2: the lowered module
   with only the new poll removed is rejected, with its entry poll
   present. A recursion with no allocation has no poll and passes;
   the same recursion with one allocation after the call and no poll
   is rejected (the firing control).
4. **Roots.** A recursion that reads a managed result of the call
   after the new poll (an object, a string, an array, and an object
   that a handler reads), with mode 2, in the three tiers: the output
   equals the output of mode 0.
5. **Rule 9.** The fact of the coercion has a test. The emitted C of
   `tree`, `callbacks`, `sort`, `particles`, `queen`, `fib-recursive`,
   and `collect` has the same count of polls as at the pin.
6. **Cost.** `perf-gate --gate` meets each criterion. The LIR snapshot
   gets the lines of the new polls and no other change.
7. The stress sweep of §342.2 passes in the three tiers.
   `tools/gate.sh full` passes. No corpus golden moves.

### 351.3 Sections this one amends

- §342 rule 11: rules 11e and 11f here; rule 11d has the bound of
  rule 6 here. §342 rule 10: rule 9 here.
- §345 rule 3: the bound has the scope of rule 6 here.

### 351.4 Open at the contract

1. A recursion that passes through the host is not in the call graph:
   a foreign call, then an export call from the host. Contrived.
2. A value that a chain of nested generators passes up crosses one
   suspension for each generator, with no poll between them. The
   allocation of that path is not measured.
