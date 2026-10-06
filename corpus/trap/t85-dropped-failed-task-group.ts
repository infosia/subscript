// corpus: trap/t85-dropped-failed-task-group
// purpose: The declaring scope releases failed work.
// exercises: task-group, lexical-scope, conditional-join, exception
// questions: compiler.md §170 rule 7, §116
// tsc: accepts
// tier-policy: both tiers trap
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
// expected-trap: task-group at the declaring scope exit; 0 unfinished, 1 failed

async function work(): Promise<void> { throw new Error("group failed"); }
async function drop(join: boolean): Promise<void> {
  const group: TaskGroup = new TaskGroup();
  group.add(work());
  await Context.suspend();
  if (join) { await group.join(); }
  print("release failed group");
}
export async function main(): Promise<void> {
  await drop(false);
  print("unreached");
}
