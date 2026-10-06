<!-- §170 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 170. A task group joins its tasks

*(Added 2026-10-06.)* Origin: the owner's async usability proposals of
2026-10-06, item 4. The owner selected the task group and the
documentation of the cancellation form on 2026-10-06. A runtime
cancellation source and token are not in this section: the current
language expresses them (rule 13).

Problem: a handle that a reference object or a module array holds can
be dropped with no `await`, and its exception is lost. The checker
rejects a dropped handle in a local (S013, `r100`), but a handle passed
to a method or stored in a field or a global is "handled". A reference
object is released only by an explicit collection (invariant 2), so the
§116.1 rule 4 trap does not fire. Measured at `6515434c` (`subscript run`):

```ts
async function work(tag: string): Promise<void> {
  await Context.suspend();
  print(`work ${tag}`);
  if (tag === "a") { throw new Error("lost"); }
}
class Group { jobs: Promise<void>[] = []; add(j: Promise<void>): void { this.jobs.push(j); } }
function start(): void { const g = new Group(); g.add(work("a")); }
// main: start(); await Context.suspend(); await Context.suspend(); print("end");
```

Output `work a`, `end`, exit 0: the exception `lost` is not reported.
A program cannot write a group that detects its own drop, because a
reference object has no release point.

### 170.1 Rules

1. The prelude declares:

   ```ts
   declare class TaskGroup {
     constructor();
     add(job: Promise<void>): void;
     join(): Promise<void>;
   }
   ```

2. A group lives in one local: a `const` declaration whose initializer
   is `new TaskGroup()`. That local is its only holder, and the group
   has no count. `TaskGroup` is a valid type only for that local and for
   a parameter of a synchronous function or method, which borrows the
   group for the call. A synchronous parameter follows the same rules as
   the local. These forms are rejected with S009 (`collisions.md` C24
   row 37): `TaskGroup` in any other type position (a field, an array
   element, a global, a result, a `let` local, a parameter of an async
   function, async method, async arrow, or generator, a type argument,
   and every position in a generator body);
   an array or descriptor literal that holds a group; an assignment of a
   group; a return of a group; a capture of a group by a lambda; and a
   use of a group other than `g.add(...)`, `g.join()`, and an argument
   to a synchronous `TaskGroup` parameter. It is not a host boundary
   type.
3. `new TaskGroup()` creates an open group. A group whose declaring
   scope has no `join()` call of it is rejected with S013.
4. `g.add(job)` moves one count of `job` to the group. The caller's
   obligation for that handle ends, as when it passes the handle to a
   function. At the `add`, the group registers a group reaction on the
   task (rule 14). If the task is complete at the `add`, the reaction
   joins the ready queue at the `add` (as §94.1 rule 6 does for an
   `await`). `add` does not resume the task.
5. A group reaction reads the task's completion, so an exception is
   observed (§116.1 rule 4). It releases the group's count of the task.
   It records the first exception in reaction order, which is the queue
   order of §94, and it counts the task as finished. `g.join()` closes
   the group and returns a new join handle. If every added task is
   finished at the `join`, the join handle completes at the call; an
   `await` of it still suspends (§94.1 rule 6). Else the reaction that
   finishes the last task completes it. The join handle completes with
   the recorded first exception, or with success. Unlike
   `Promise.all`, it waits for every task after a failure.
6. `add` on a closed group traps with `TrapKind::TaskGroup`. A second
   `join` on one group traps with `TrapKind::TaskGroup`.
7. When the scope of the declaring local ends (a normal exit, a
   `return`, a `break`, or an exception exit) and the group was not
   joined, and the group holds a task that is not finished, or a
   finished task that failed, the exit traps with `TrapKind::TaskGroup`.
   An exception exit traps at once, as §116.1 rule 4b does. The message
   states the number of unfinished and failed tasks. A group whose tasks
   all succeeded, or an empty group, ends with no trap. A generator
   frame can end with no scope exit (a dropped iterator), so a group is
   not valid in a generator body (rule 2); the Phase Review measured
   that such a group loses a failed task's exception and stays rooted
   for the life of the Context. An async frame that never completes (a
   wait ring, a frame stopped at host clearance, a stale frame after a
   reload) keeps its group, as it keeps its handle locals: a failed
   task in that group is not reported, the same as an unobserved handle
   in that frame. After `join`,
   the end of the scope does not trap; the join handle has its own §116
   rules, and it keeps the group state that its reactions need.
8. `TrapKind::TaskGroup` is a new trap kind with the next free number.
9. While its scope is live, and while its join handle is live, the
   group roots its tasks, their completions, and its first exception for
   explicit collection (invariant 2).
10. §169 reports the join handle as a task of `kind` 3 (a group join).
    Its `function_pos_id` and `create_pos_id` are the position of the
    `join` call; it is `WAITING` until it completes. The group is not a
    task and has no task id. A task that a group holds reports its own
    state; a waiter on the join handle names the join task.
11. The three tiers (dev JIT, C AOT, interpreter) give the same order
    and output.
12. `collisions.md` C8 states that `TaskGroup` is a language class with
    no JavaScript counterpart, so its corpus entries are not
    `js-comparable` and cite C8.
13. The TypeScript tutorial states the cancellation form that the
    current language expresses: a token class with a `cancelled` flag,
    a `throwIfCancelled()` method that throws an `Error`, a source that
    sets the flag, and a `try` around the `await` that tests the
    message. It states that the check is cooperative, and that a task
    observes a cancellation only when it runs. A program fence and its
    output fence are in the §91 test.

14. The form: a group is a runtime object, separate from its join
    handle. It holds its state (open or closed), its tasks with the
    group's count of each unfinished task, the finished count, the
    failed count, the first exception in reaction order, and its join
    handle after `join`. A ready job and a waiter have a third tagged
    kind, the group reaction (the group and the task index), beside the
    §166 invocation continuation and aggregate reaction. The checkpoint
    dispatches it by its kind, and §168 counts it as one dispatch. The
    interpreter has the same tagged form. Round 1 measured that the §166
    aggregate cannot carry a group: it completes at the first exception,
    it registers at the call in input order, and it holds no open or
    closed state.

15. A group uses no §70 count, because a count does not reach every
    last holder. Round 2 measured that an async completion holds bytes
    and has no release of a counted result: with
    `async function make(): Promise<Promise<void>[]>`, one completed task
    stays registered after every holder ends; the synchronous control
    leaves none. Round 3 measured that an element removed through a
    copy of a handle array keeps one count after every holder ends; the
    control leaves none. A field holder ends only at an explicit
    collection (invariant 2). Rule 2 gives the group one lexical holder,
    so the end of its scope is the release point. The two count defects
    are outside this section.

### 170.2 Acceptance

1. Red first, at the contract pin: accept entry `a337` (success,
   failure of two tasks with the first in reaction order (rule 5), a task complete before its `add`, a group
   passed to a synchronous helper that adds tasks, a join after all
   tasks completed, an empty
   group, collection between checkpoints); reject entries `r383` (a group
   that is never joined or stored) `r384` (an async function whose
   fulfilled type is `TaskGroup`), and `r385` (a group stored in a
   field); trap entries `t84` (a scope that ends
   without `join` while a task is unfinished), `t85` (a scope that ends
   without `join` after a task failed), `t86` (`add` after `join`). Each with its measured `tsc`
   header.
2. Unit tests for rules 4 to 9, each with a same-shape control. The
   counts of each task and of the join handle are zero after success
   and after failure, and no group state stays live after its scope
   and its join handle end. A test of rule 7 on an exception exit.
3. The §154 total test passes for the new rejection site.
4. The async-cost benchmark on the existing workloads: the median is at
   most 1.05 times the pin median (release, best of three).
5. No existing `.expected` golden moves.
