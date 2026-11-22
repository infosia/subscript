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
2. **A result type that holds a handle is rejected**, as for
   `Promise.all` (§166 rule 12), with a diagnostic that names
   `Promise.race`.
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
5. **An input that lost is observed.** An exception of an input of a
   race does not trap at the release of that input, if the input lost
   and if it fails after the race completed. The program can await an
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
   (rule 2); a spread or a second argument (rule 1); each with the
   header that states what `tsc` does.
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
