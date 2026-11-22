<!-- §350 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 350. `Promise.race`

*(Added 2026-11-22.)* Origin: the owner decisions of 2026-11-22 and
the measurement in `specs/tracking/promise-race.md`.

Problem, measured at `7c1130fa`:

1. **A program cannot wait for the first of some tasks.**
   `Promise.all` waits for each input. `Worker.receive` (§349) gives
   one handle for each Worker, and §349.6 item 7 records that no form
   takes the first message of some Workers. The form that remains is a
   loop of `poll()` and `Context.suspend()`, which uses one core
   (§349, the measurement).
2. **The standard form is rejected with two different texts.**
   `await Promise.race(...)` is S016 "unknown name `Promise`"
   (§166.3 item 1), and the held form is S013.
3. **`tsc` and node define the form.** The language accepts
   `Promise.all` with the turn sequence of node, and the same
   machinery gives a race.

### 350.1 Rules

Surface: `Promise.race(inputs)`, with the lib declaration of `tsc`.
The prelude does not change.

1. **Accepted forms.** `Promise.race` takes one argument: an array
   literal of handles, or a value of type `Promise<T>[]`. Each rule of
   `Promise.all` on its argument holds (§166): no type argument, no
   spread, one element type `Promise<T>`. The result type is
   `Promise<T>`. The result is a handle as any other: `await`, a held
   form, S013 for a handle that the program drops.
2. **A result type that holds a count is rejected**, as for
   `Promise.all` (§166 rule 12), with a diagnostic that names
   `Promise.race`. The test is the counted type of the result: a
   handle, a generator, an array or a `FixedArray` of them at any
   depth. The same test now holds for `Promise.all`, which tested a
   handle and an array of handles only.
3. **The first completion wins (decision A).** The race completes
   with the completion of the first input whose completion it reads:
   the value, or the exception. An `await` of the race gives that
   value or raises that exception.
4. **The race reads completions in the sequence of `Promise.all`.**
   At the call, each input that has a completion gets a reaction job,
   in array sequence, and each other input gets one when it completes.
   The first job that runs wins (decision C). The race has no
   completion at the return of the call: an `await` of it resumes one
   turn after the completion of the winner, as node does and as
   `Promise.all` does.
5. **An input of a race is observed from the call.** An exception of
   an input does not trap at the release of that input: not if the
   input lost, and not if it fails after the race completed. The program can await an
   input again, and that await gives its value or raises its exception
   (§70.3 rule 4).
6. **After the race has its completion, it holds no input.** The race
   releases its count of each input and removes its reaction from each
   input that has no completion. So a race whose loser does not
   complete does not stay registered for that loser, and a loop that
   makes one race for each turn over a task that runs on keeps a
   bounded count of frames. The input carries the fact of rule 5 from
   the call of the race.
7. **An empty array traps (decision B).** A race over an array with no
   element traps at the position of the call, with a trap kind of its
   own (the next free number) and a message that names `Promise.race`.
   An empty literal with no element type is rejected at the check
   (S100), as for `Promise.all`.
8. **No cancel.** An input that lost runs to its end. A
   `Worker.receive` that lost stays in the sequence of its Worker and
   takes the next message; a program that drops that handle does not
   read that message (§349.6 item 1). The program keeps the handles of
   the inputs that lost and awaits them or races them again.
9. **The race gives the value only.** A program that needs the index
   awaits each input in an async function that returns the index with
   the value, and races the handles of those calls.
10. **A runtime frame of the aggregate kind.** The task visitor
    reports a race with the kind number of `Promise.all`. It is a
    root of a collection while it is registered (§348 rule 5). Its
    completion holds the bytes of the winner; a string or a byte
    buffer of a host completion that won stays valid after the release
    of that input.
11. **The three tiers agree**, with the output and the turn sequence
    of rule 4. `Promise.race` can allocate (§342 rule 10).
12. **A duplicate handle is one input for each element**, as for
    `Promise.all`.

### 350.2 Acceptance

1. **Corpus accept entry, `js-comparable: yes`**, equal in the three
   tiers and under node: the first value wins; an exception that wins
   is caught; an input that lost and then fails gives no trap; inputs
   that lost are awaited after the race; two inputs with a completion
   at the call, in both array sequences (rule 4); a pending input that
   completes in the same turn does not pass one that had its
   completion at the call; the resume turn of an `await` of a race
   against a direct `await` (rule 4); a `void` race; a class result; a
   nullable result; the held form; a duplicate handle; a race in a
   `TaskGroup`.
2. **Corpus accept entry with Workers** (dev JIT and ship tier): the
   index form of rule 9 over the receives of two Workers, in a loop
   that takes each message of both in arrival sequence and prints a
   result that does not depend on that sequence (a sum for each
   Worker).
3. **Corpus trap entry**: a race over an empty `Promise<i32>[]` value
   traps with the new kind at the call, equal in the three tiers. The
   header cites the collision: node does not settle.
4. **Corpus reject entries**: a result type that holds a handle
   (rule 2), in a form that `tsc` accepts (`Promise<Promise<i32>[]>[]`;
   `tsc` rejects the annotation of a race over `Promise<Promise<i32>>[]`);
   a spread argument (rule 1); each with the header that states what
   `tsc` does.
5. **Rule 6.** A test in the three tiers: a loop of 1,000 and of
   1,400 turns, each turn makes a race of one task that completes and
   one handle that does not complete in the loop; the live bytes after
   the run and the count of unfinished tasks are equal at the two
   counts. Red before rule 6 is built (the form of `Promise.all`
   grows). Control: the same loop with `Promise.all` in a form that
   can run.
6. **Runtime tests**: rules 3, 4, 5, 6, 7, 10 (a host string that
   wins), and 12 at the runtime API, each with a firing control where
   it states an absence.
7. **The diagnostics**: `await Promise.race(...)` and the held form
   check; each rejection of rule 1 and rule 2 names `Promise.race`;
   `Promise.resolve` keeps its rejection.
8. The stress sweep of §342.2 passes in the three tiers, with the exit
   counts (§345 rule 4).
9. `perf-gate --gate` meets each criterion. `tools/gate.sh full`
   passes.

### 350.3 Sections this one amends

- `collisions.md` C8: `Promise.race` is admitted; an empty race traps,
  and node does not settle.
- §166.3 item 1: closes for `Promise.race`.
- §349.6 item 7: closes.
- §116.1 rule 4: an input of a race that lost is observed (rule 5).

### 350.4 Not in this section

- A cancel of an input that lost.
- `Promise.any`, `Promise.allSettled`, `Promise.resolve`,
  `Promise.reject`, `new Promise`.
- An array literal with handles of different result types (a union
  for `tsc`; the array rule of this language rejects it, as for
  `Promise.all`).

### 350.5 Phase Review

Form.

- A race is the aggregate of `Promise.all` with a mode
  (`runtime/src/context/async_scheduler.rs`). The entry is
  `subscript_rt_async_race(ctx, jobs, result_size, pos_id)`. The frame
  holds the completion size of `T`, and the win is a copy of the bytes
  of the winner.
- `remaining` of a race counts its reaction jobs that are queued or
  registered. At the win the race takes its job from each input that
  has no completion and releases each input. A job that was in the
  ready queue at the win runs and reads nothing; the frame stays
  registered until those jobs ran.
- Rule 5: at the call, an input with an exception is marked as
  observed, and an input with no completion goes to a list in the
  collection state. Each place that stores a failure calls
  `note_task_failure` (§342 rule 14b), which applies the list before a
  release or a detection reads the completion. One handle is in the
  list one time.
- The trap of rule 7 is `TrapKind::PromiseRaceEmpty`, 38,
  `promise-race-empty`.
- The checker has five rejection sites of the race, so that each
  rejection shows a race example. The LIR instruction is `AsyncRace`,
  with a `Call` trap site that `AsyncAll` does not have.
- The interpreter has the race in `codegen/src/interpreter/async_race.rs`
  and the fact of rule 5 as a flag of its coroutine.
- The index form of rule 9 with a loop needs one more statement: a
  handle that the loop stores into an array element needs an `await`
  on the path after the loop (S013). `a509` and the tutorial await
  each handle that remains.

Measured.

| Item | Result |
|---|---|
| `a507` | equal in the three tiers and under node v24.22.1, byte for byte |
| Resume turn: direct `await`, `await Promise.race([task])`, `await Promise.all([task])` | 3, 4, 4 in the three tiers and under node |
| Rule 6, live bytes after 1,000 and 1,400 turns | race: dev JIT 159 and 159, ship tier 160 and 160, interpreter 57 and 57; `Promise.all`: 64,156 and 89,756, 80,160 and 112,160, 48,055 and 67,255 |
| Rule 6 at the runtime API, 100 and 1,000 turns | registered frames, unfinished tasks, and waiters of the input that does not complete: 1, 1, 0 at both counts; `Promise.all`: 101, 1, 100 and 1,001, 1, 1,000 |
| Rule 6 in the interpreter | registered tasks and waiters equal at 1,000 and 1,400 turns; 400 more of each with the detach switched off |
| Rule 5 | an input that lost and then throws: the run ends with no trap in the three tiers; the same task with no race traps with an uncaught exception at 5:3 in the three tiers |
| Rule 7 | kind 38 at the call (13:24 in `t120`) in the three tiers; at the pin the checker rejects the program with S016 |
| `a509`, 20 runs in each tier | 40 of 40 equal to the golden; no run over its time limit |
| Mutations at the runtime | no observation of the inputs: 4 tests fail; no detach: 14 tests fail |
| Stress sweep, 614 entries | dev JIT 611 of 611, ship tier 609 of 609, interpreter 499 of 499; 487 exit counts compared; 6 not compared by §349 rule 16 |
| Cost of the tests | `codegen/tests/promise_race.rs`: 0.9 s in a release build, 1.7 s in a debug build, 8 ship builds; the runtime tests 0.07 s |
| `perf-gate --gate`, two passes | each criterion met, on the tree before the corrections of the review: `a22` ship tier 1.33x and 1.32x, `collect` ship tier 1.14x and 1.14x, `a22` dev JIT 19.88x and 19.77x, `collect` dev JIT 3.69x and 3.67x |

Phase Review (a subagent with no context): no CRITICAL, 1 MAJOR, 4
MINOR. The review followed each count of an input, each path that
reads the observed fact, and the job sequence of rule 4, and it
evaluated each line of the golden of `a507` by the microtask rules of
node.

| Finding | Close |
|---|---|
| MAJOR: in the interpreter, a held race that lost its last holder before its completion gave an internal error at a queued job of an input that lost | the interpreter keeps the settled state after the release; a test in the three tiers |
| Rule 2 tested a handle and an array of handles, and a counted type of another form passed (`Promise<Promise<i32>[][]>`); the same for `Promise.all` | the test is the counted type, for both forms; rule 2 states it |
| The tutorial and the language reference did not state rule 5 as the contract does, the mixed literal, or the empty race under node; §349.6 item 7 stated the former state as a present fact | each is corrected |
| New `unsafe` calls of the runtime had no `// SAFETY:` comment | each has one |
| The test of acceptance 5 in the three tiers compares the live bytes and not the count of unfinished tasks | open item 1 |

`tools/gate.sh full` on the tree before this record:

```text
gate full 80b3f15f9940ea0c093131e496113b7daa953c1e dirty:69 debug 4044/0/5 release 4025/0/5 skips 2/0 clippy 4/10/13 goldens-moved 1 exit 0
```

`tools/hygiene.sh`: exit 0.

### 350.6 Open

1. The test of acceptance 5 in the three tiers compares the live
   bytes. The count of unfinished tasks is compared at the runtime
   API and in the interpreter test only.
2. An array literal with handles of different result types is
   rejected by the array rule (S100); `tsc` gives a union (§350.4).
3. A scalar argument (`Promise.race(x)` with `x: i32`) is rejected at
   a site whose class states that `tsc` accepts the form, and `tsc`
   rejects it (TS2769). `Promise.all` has the same class at its site.
4. A `Worker.receive` that lost a race takes the next message of its
   Worker (rule 8). A program that drops that handle does not read
   that message.
5. `Promise.any`, `Promise.allSettled`, `Promise.resolve`, and
   `Promise.reject` stay rejected; `await Promise.resolve(1)` gives
   S016 "unknown name `Promise`" (§166.3 item 1).

