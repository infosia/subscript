// corpus: accept/a337-task-group
// purpose: A group waits for every task and reports the first failure in reaction order.
// exercises: task-group, held-handle, synchronous-borrow, collection, try-catch
// questions: compiler.md §170, §70, §94, §116
// tsc: accepts
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.

async function work(tag: string, turns: i32, fail: boolean): Promise<void> {
  for (let i: i32 = 0; i < turns; i++) { await Context.suspend(); }
  print(`done ${tag}`);
  if (fail) { throw new Error(tag); }
}

function addWork(group: TaskGroup, tag: string): void {
  group.add(work(tag, 1, false));
}

export async function main(): Promise<void> {
  const success: TaskGroup = new TaskGroup();
  success.add(work("success-a", 1, false));
  success.add(work("success-b", 2, false));
  const joined: Promise<void> = success.join();
  Context.collect();
  await Context.suspend();
  Context.collect();
  await joined;
  print("success joined");

  const failures: TaskGroup = new TaskGroup();
  failures.add(work("input-first", 3, true));
  failures.add(work("completion-first", 1, true));
  try { await failures.join(); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  print("all failures joined");

  const borrowed: TaskGroup = new TaskGroup();
  addWork(borrowed, "helper");
  Context.collect();
  await borrowed.join();
  print("helper joined");

  const immediate: TaskGroup = new TaskGroup();
  immediate.add(work("immediate", 0, false));
  await immediate.join();
  print("immediate joined");

  const completed: TaskGroup = new TaskGroup();
  completed.add(work("completed", 1, false));
  await Context.suspend();
  await Context.suspend();
  await completed.join();
  print("completed joined");

  const completedFailures: TaskGroup = new TaskGroup();
  completedFailures.add(work("late-input-first", 2, true));
  completedFailures.add(work("late-completion-first", 1, true));
  await Context.suspend();
  await Context.suspend();
  await Context.suspend();
  Context.collect();
  try { await completedFailures.join(); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  print("completed failures joined");

  const empty: TaskGroup = new TaskGroup();
  await empty.join();
  print("empty joined");

}
