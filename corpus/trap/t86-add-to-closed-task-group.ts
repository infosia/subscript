// corpus: trap/t86-add-to-closed-task-group
// purpose: A joined group rejects a later add.
// exercises: task-group, join, await, trap
// questions: compiler.md §170 rule 6, §94
// tsc: accepts
// tier-policy: both tiers trap
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
// expected-trap: task-group at add on the closed group

async function work(): Promise<void> { return; }
export async function main(): Promise<void> {
  const group: TaskGroup = new TaskGroup();
  await group.join();
  print("add to closed group");
  group.add(work());
  print("unreached");
}
