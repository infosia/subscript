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
   indirect call, a runtime operation with a function-value operand,
   and a foreign call: a host calls a callback that the script
   registered, also from a foreign call with no function operand);
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
   `for…of` over a generator, and the close of a generator at a
   `break`, a `return`, or an exception of that loop, which runs its
   `finally` code. The language has no `yield*`, no spread of a
   generator, and no `return()` method (each is rejected at the
   check).
4. **The poll after a resume.** §351 rule 1 holds for the resume
   instruction as for a call: in a function of a cycle, if a possible
   body is in that cycle and the continuation can allocate or suspend
   before a poll, one poll is directly after the resume.
5. **A resume does not count as a poll** in the scan of §351 rule 3.
   A generator can resume at a suspension point, past its entry poll.
6. **A value that the continuation reads is rooted at the poll**
   (§351 rule 10): the result of `next()` also.
7. **Rule 10a of §342 does not change** (the interpreter check of the
   allocation fact). Rule 10 here adds a check of the new fact.
8. **The bound of §351 rule 6 includes the returns of generator
   resumes.** §351 rule 7 no longer names nested generators.
9. **A resume point of a generator of a cycle is an entry (rule 11h
   of §342).** *(Added 2026-11-22, after the first round of the
   implementation.)* A later resume enters a generator at the
   successor of a `yield`, past its entry poll. A generator that
   allocates there and then resumes its child gives a chain of
   resumes with no poll on the way down, one frame for each level.
   So in a generator of a cycle, a resume point whose path can
   allocate or suspend before a poll has a poll at its start. The
   scan is the scan of §351 rule 1. If the path of a normal resume
   can allocate before a poll, the poll is before the test of the
   close request, and the path of a close passes it too. If only the
   path of a close can, the poll is at the start of that path, so a
   normal resume does not run it. A generator
   outside each cycle gets no such poll. An async function gets none:
   each resume of it is a job of a drain (§351 rule 12). The verifier
   checks the rule by its own derivation.
10. **The interpreter checks the fact.** *(Added 2026-11-22, after
    the Phase Review.)* The total match stops a kind with no fact. It
    does not stop a wrong fact, and the placement and the verifier
    read the same fact. So the interpreter tests each entry of a
    script activation that has a script frame under it: the
    instruction or the terminator that the calling frame runs must
    have a fact that covers the function that is entered (the named
    function; a function whose address is taken; a generator). An
    entry under the fact "none" is an internal error that names the
    kind and the two functions. An entry from the driver (the module
    initializer, an export, a job of the drain) has no script frame
    under it and is not tested. The interpreter runs no foreign call
    and no host completion, so the corpus does not test the fact of
    those two kinds.

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
   that iterates its child with `for…of` and then allocates; a value
   that passes up through nested generators (§351.4 item 2: each
   level iterates its child and yields each value). Control: a
   recursion that resumes a generator that is not in its cycle has no
   poll after the resume. *(Amended 2026-11-22: the first text named
   `yield*`, which the checker rejects, and a control whose
   continuation returns a scalar. Each resume is followed by the
   release of its receiver, which has the fact "can allocate", so
   each resume into the cycle gets a poll.)*
2a. **Rule 9.** A chain of generators that each allocate after a
   `yield` and then resume the child, with no loop on that path, at
   depths 128 and 256, in the dev JIT and the ship tier. The sample is
   at the bottom of the last descent, after a collection at the bottom
   of the first: the new objects are under a bound that the test
   derives, equal at the two depths, and the test asserts that the
   objects of one descent with no poll are over that bound. Red with
   no poll at the resume point: 8,192 and 16,384 objects. A loop on
   the path (a `for…of` walk of a tree) has its backedge poll on the
   path also, and each of the two polls alone bounds the descent; the
   test keeps those shapes as bounded at the pin and after.
   Roots: values that a generator holds across a `yield` and reads
   after the poll of the resume point, mode 2, three tiers.
3. **Verifier.** For each shape of items 1 and 2: the lowered module
   with only the poll after the resume removed is rejected, with the
   poll of the generator creation and each entry poll present. For
   item 2a: the module with only one poll of a resume point removed
   is rejected with the message of that rule only.
4. **Roots.** A generator yields an object that has no other holder.
   The caller reads a field through the result of `next()` after the
   new poll. Mode 2, in the three tiers: the output equals the output
   of mode 0.
5. **Rule 1 is total, and rule 10 checks it.** A unit test gives the
   fact of each instruction kind and each terminator kind that runs
   script code. The interpreter runs the accept corpus with the check
   of rule 10 and reports no entry. Firing controls: with the fact of
   one kind read as "none" (a direct call, a callback operation, a
   generator resume, an `await` of a direct async call), the check
   reports the entry. A test over the generator entries of the corpus
   shows that the resume fact adds polls only to functions of a cycle
   with a resume edge. *(Amended 2026-11-22: the first text stated
   that no function without a resume changes. A function that joins a
   cycle through a resume edge gets polls.)*
5a. **A foreign call (rule 1).** A function calls a host function that
   calls a registered callback, and the callback calls that function;
   each level allocates after the foreign call. Depths 128 and 256,
   dev JIT and ship tier, in the form of item 1. Red with the fact
   "none": 775 and 1,543 objects. A unit test of the placement and a
   removal test of the verifier.
6. **Cost.** The count of poll sites over the accept corpus and over
   the benchmark workloads, before and after: the record states both.
   The seven workloads of §351 acceptance 5 keep their counts.
   `perf-gate --gate` meets each criterion.
7. The stress sweep of §342.2 passes in the three tiers.
   `tools/gate.sh full` passes. No corpus golden moves; the LIR
   snapshot gets `CollectionPoll` lines only.

### 352.3 Sections this one amends

- §351 rules 1, 3, 6, and 7, and §351.4 items 1 (the callback form
  closes) and 2 (closes).
- §342 rule 11e: a resume of a generator is a call of that rule.
  §342 rule 11c: rule 9 here adds the poll of a resume point (rule
  11h).

### 352.4 Open

1. The interpreter runs no foreign call and no host completion, so
   the check of rule 10 does not test the fact of those two kinds. A
   unit test states each fact, and the acceptance test of item 5a
   measures the foreign call in the two native tiers.
2. A runtime operation with a function-type operand has the fact
   "each function whose address is taken", also where the operation
   does not call the operand (`push` of a function value into an
   array, `Map.set` with a function value). A foreign call has an
   edge to each function whose address is taken, also to a function
   that the host did not get. Each is sound and can add polls. The
   count over the accept corpus is in §352.5.
3. An export that the host calls from inside a foreign call is not in
   the graph (§351.4 item 1). The ship tier traps it (§223).
4. No test of the two native tiers measures the poll at the start of
   a close path (rule 9, the second case). A unit test of the
   placement and a removal test of the verifier hold it.
5. In a `for…of` walk of a tree the backedge poll and the poll of the
   resume point are both on the path of a descent, and each alone
   bounds it. The test of that shape fails only with both removed.
6. The acceptance test of item 5a does not run on the MSVC host (the
   native fixture, §11c). The unit test and the verifier test of the
   foreign call run on each host.
7. The dev JIT gives a test no read of the live bytes at the bottom of
   a descent, so the premise of the bound of item 2a is asserted in
   the ship tier only (§351.4 item 4). The interpreter tests of this
   section use depths 6 and 8: a debug test thread of 2 MiB ends at
   depth 12 in one of them.

### 352.5 Phase Review

Form.

- `codegen/src/lir/script_entry.rs`: `ScriptEntry` with five values,
  and total matches over `InstructionKind`, `CallTargetKind`,
  `BuiltinMethod`, `Terminator`, and `SuspendKind`, with no default
  arm.
- The placement (`collection_poll.rs`, with the scan of a
  continuation and of a resume point in the child module
  `collection_poll/return_path.rs`) and the verifier
  (`verify_collection_poll.rs`) each read the fact with graph code of
  their own: a reachability search in the placement, Tarjan
  components and a backward fixpoint in the verifier.
- A resume of a generator is `Call(BuiltinMethod::GeneratorNext)`; a
  `for…of` over a generator has it in its loop header, and
  `GeneratorClose` at a `break`, a `return`, or an exception. The
  poll is directly after each. The checker rejects `yield*`, a spread
  of a generator, `Array.from` of one, a destructuring from one,
  `return()`, and `using` of one.
- A resume point is the successor of a `yield` suspension. Its block
  is the test of the close request and a branch. The poll is at the
  start of that block if the path of a normal resume is open, and at
  the start of the close path if only that path is open.
- The interpreter check (`codegen/src/interpreter/script_entry_check.rs`)
  is the first statement of the function that runs each frame body.
  It reads the instruction of the frame under the new one, or the
  `await` terminator of the parent for a direct async call. The
  frame index does not move while a callee is entered, and an index
  that does not name an instruction names a terminator, whose fact is
  "none" except for that `await`; so a wrong index fails the check.
- A release of the last holder of a suspended generator frees the
  frame and runs no `finally` code (`a351` shows it), so a release
  has the fact "none".

Measured. Objects over the base, automatic mode, depths 128 and 256.
"With no poll" is the tree with the fact or the poll of that row
switched off by a temporary change.

| Shape | With no poll, both tiers | Ship tier | Dev JIT |
|---|---|---|---|
| A function resumes a generator and then allocates five objects (the fixture of the review) | 645 and 1,285 | 5 and 5 | 5 and 5 |
| The handle is a parameter of the function that resumes it | 640 and 1,280 | 5 and 5 | 5 and 5 |
| A recursive generator iterates its child with `for…of` and then allocates | 645 and 1,285 | 5 and 5 | 5 and 5 |
| Each level iterates its child and yields each value | 1,280 and 2,560 | 20 and 10 | 10 and 10 |
| A chain of generators that allocate after a `yield` and resume the child, new objects at the bottom of the last descent (rule 9) | 8,192 and 16,384 | 2,624 and 2,112 | 2,624 and 2,112 |
| A function calls a host function that calls a registered callback (item 5a) | 775 and 1,543 | 7 and 7 | 7 and 7 |

| Item | Result |
|---|---|
| Collections between the bottom of the recursion and the exit, ship tier, the fixture | 0 with no poll; 9 and 10 |
| The interpreter check over the accept corpus | 409 entries run, none reported |
| The check with the fact of one kind read as "none" | entries of 409 that fail: 328 (a function), 110 (a function whose address is taken), 42 (a generator), 69 (an async function) |
| Poll sites over the accept corpus | 1,567 at the pin; 1,572 with the resume as a call; 1,577 with the polls of the resume points; 1,579 with the fact of a foreign call (`a246` and `a249`, one more each); the generator polls are in one entry, `a409` |
| A host completion through a foreign function with the fact of a foreign call | no poll site changes in the accept corpus or in the examples. The target kind is a `#[non_exhaustive]` enum of another crate, so that match has one arm for a target with no name here, with the fact of a foreign call |
| Poll sites of `tree`, `callbacks`, `sort`, `particles`, `queen`, `fib-recursive`, `collect` | 2, 2, 1, 1, 0, 0, 2 at the pin and after |
| Polls on the path of a normal resume of a `for…of` tree walk, for each value and each level | 2: the backedge poll and the poll after the resume |
| Roots, mode 2, three tiers | an object, a string, and an array that a generator yields and the caller reads after the poll; values that a generator holds over a `yield`: output equal to mode 0 |
| Stress sweep, 614 entries | dev JIT 611 of 611, ship tier 609 of 609, interpreter 499 of 499; exit counts equal |
| `perf-gate --gate`, two passes | each criterion met: `a22` ship tier 1.32x and 1.33x, `collect` ship tier 1.14x and 1.16x |

Phase Review, first (a subagent with no context): no CRITICAL, 1
MAJOR, 8 MINOR. The review read each kind with the fact "none"
against its lowerings and its runtime entry, and followed eight more
shapes of a chain of resumes and closes; it found no shape inside a
module that passes the bound.

| Finding | Close |
|---|---|
| MAJOR: nothing tested that a fact is true; the placement and the verifier read the same fact | rule 10, the interpreter check |
| A foreign call had the fact "none", and a host calls a registered callback from it | the fact of rule 1; acceptance 5a |
| A poll of a resume point that only the close path needs ran on each normal resume | rule 9, the second case |
| §351 rules 1, 3, 6, and 7 and §342 rules 11c and 11e had no note | the notes |
| Acceptance 5 stated more than its test shows | the text of item 5 |
| A doubt that one shape fails with only its named poll removed | measured: each shape fails, except the tree walk (open item 5) |
| The collection clause of the dev JIT test passed with collections on the descent | removed; the dev JIT test uses the object bound |
| Test support in a public module | `#[doc(hidden)]` |
| Depths and premises that are not measured on other hosts | open item 7 |

Phase Review, second, of the check, the foreign call, and the finer
placement: no CRITICAL, no MAJOR, 4 MINOR. The review found no entry
of a frame that the check does not see, no index that names a
neighbour instruction, and no correct program that the check
reports.

| Finding | Close |
|---|---|
| A host completion through a foreign function calls the C function in the instruction, and its fact was "none" | the fact of a foreign call for that target |
| Test comments held measured figures | the comments state what each test proves; the figures are here |
| A shape of a balanced tree that cannot fail | deleted |
| A function-type operand that the operation does not call | open item 2 |

Three results of this section that corrected the contract text:
`yield*` is not in the language; each resume into the cycle gets a
poll, because the release of its receiver has the fact "can
allocate"; a descent through later resumes had no poll (rule 9).

`tools/gate.sh full` on the tree before this record:

```text
gate full f0fbdb398167293e54669434b4b4fb4cf1a7ccef dirty:14 debug 4107/0/5 release 4088/0/5 skips 2/0 clippy 4/10/13 goldens-moved 1 exit 0
```

`tools/hygiene.sh`: exit 0.

