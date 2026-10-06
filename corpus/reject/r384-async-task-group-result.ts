// corpus: reject/r384-async-task-group-result
// purpose: An async completion must not hold a task group.
// exercises: task-group, async-result
// questions: compiler.md §170 rules 2 and 15
// tsc: accepts
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
// expected-error: S009 at the async result declaration

async function make(): Promise<TaskGroup> { return new TaskGroup(); }
export async function main(): Promise<void> {
  const group: TaskGroup = await make();
  await group.join();
}
