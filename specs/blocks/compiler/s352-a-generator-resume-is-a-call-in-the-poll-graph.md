<!-- §352 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 352. A generator resume is a call in the poll graph

*(Added 2026-11-22.)* Origin: a second review of the poll placement
by another agent, at `524edac8`, after §351.

Problem, measured by that review in the ship tier at `524edac8`
(automatic mode, minimum headroom 64 bytes, sampled before the
outermost exit; this section measures it again at its pin):

1. **A return from `Generator.next()` has no poll.** A function
   `step` creates a generator, calls `next()` on it, and then
   allocates five objects. The body of the generator calls `step`.
   At depths 32, 256, and 1,024 the Context keeps 165, 1,285, and
   5,125 objects before the exit. The automatic collections of the
   run are on the descent. No poll-site test runs after the first
   allocation of the return path.
2. **The call graph of §351 has no edge for the resume.** It has an
   edge for a direct call, an indirect call, a runtime operation with
   a function-value operand, and an `await` of a direct async call.
   `next()` takes a generator handle, so the instruction that runs
   the body of the generator adds no edge, in the placement and in
   the verifier.
3. **The graph is a list of cases.** Each instruction that runs
   script code in the same activation chain needs an edge. §351 found
   the cases one at a time (the indirect call, the callback
   operation, the `await`), and this is the fourth. A total fact
   closes the class.
4. **§351.4 item 2 is the same gap** in another shape: a value that
   passes up through nested generators.

### 352.1 Rules

1. **Each LIR instruction and each terminator states what script code
   it runs before it returns.** The fact has these values: none; one
   function (a direct call); each function whose address is taken (an
   indirect call, a runtime operation with a function-value operand);
   each generator body (a resume or a close of a generator); one
   async function (an `await` of a direct async call). The fact is a
   total match over the instruction kinds and the terminator kinds,
   with no default arm, in the form of the allocation fact (§342 rule
   10). A new instruction kind does not compile until it states its
   fact.
2. **The placement and the verifier build the call graph from that
   fact.** Each of the two keeps its own graph code and its own cycle
   search (§351 rule 8); the fact is the one input they share, as the
   allocation fact is.
3. **A resume of a generator has an edge to each generator function
   of the module.** The handle does not name its body. The edge is
   from the function with the resume instruction: `next()`, a
   `for…of` over a generator, `yield*`, and each operation that
   closes a generator and runs its `finally` code.
4. **The poll after a resume.** §351 rule 1 holds for the resume
   instruction as for a call: in a function of a cycle, if a possible
   body is in that cycle and the continuation can allocate or suspend
   before a poll, one poll is directly after the resume.
5. **A resume does not count as a poll** in the scan of §351 rule 3.
   A generator can resume at a suspension point, past its entry poll.
6. **A value that the continuation reads is rooted at the poll**
   (§351 rule 10): the result of `next()` also.
7. **The interpreter refuses nothing new.** The fact is static; rule
   10a of §342 (the interpreter check of the allocation fact) does not
   change.
8. **The bound of §351 rule 6 includes the returns of generator
   resumes.** §351 rule 7 no longer names nested generators.

### 352.2 Acceptance

A test with a deep recursion uses depths 128 and 256: a debug build
of the ship program runs on a main-thread stack of 1 MiB on one host
(§351.4 item 6).

1. **Collection on the return from a resume.** The fixture of the
   problem (a function, then `next()`, then five objects; the body of
   the generator calls the function), automatic mode, minimum headroom
   64 bytes, dev JIT and ship tier: the objects before the outermost
   exit are under the bound of §351 acceptance 1, equal at the two
   depths. The test also compares the collection count at the bottom
   of the recursion with the count before the exit, and requires a
   difference above zero: a count from the start of the call passes
   with collections on the descent only. Red at the pin.
2. **More shapes**, in the same form: the generator handle is a
   parameter of the function that resumes it; a recursive generator
   that iterates its child with `for…of` and then allocates; a
   recursive generator with `yield*`; a value that passes up through
   nested generators (§351.4 item 2: each level resumes its child and
   then yields). Control: a continuation that returns a scalar gets no
   poll.
3. **Verifier.** For each shape of items 1 and 2: the lowered module
   with only the poll after the resume removed is rejected, with the
   poll of the generator creation and each entry poll present.
4. **Roots.** A generator yields an object that has no other holder.
   The caller reads a field through the result of `next()` after the
   new poll. Mode 2, in the three tiers: the output equals the output
   of mode 0.
5. **Rule 1 is total.** A unit test gives the fact of each instruction
   kind and each terminator kind that runs script code, and a test of
   the graph shows that the edges of §351 (direct, indirect, callback
   operation, `await`) are the same set as before this section for
   the accept corpus: the count of poll sites of each function that
   has no generator resume does not change.
6. **Cost.** The count of poll sites over the accept corpus and over
   the benchmark workloads, before and after: the record states both.
   The seven workloads of §351 acceptance 5 keep their counts.
   `perf-gate --gate` meets each criterion.
7. The stress sweep of §342.2 passes in the three tiers.
   `tools/gate.sh full` passes. No corpus golden moves; the LIR
   snapshot gets `CollectionPoll` lines only.

### 352.3 Sections this one amends

- §351 rules 1, 3, 6, and 7, and §351.4 item 2 (closes).
- §342 rule 11e: a resume of a generator is a call of that rule.
