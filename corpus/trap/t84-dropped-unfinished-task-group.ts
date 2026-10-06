// corpus: trap/t84-dropped-unfinished-task-group
// purpose: The declaring scope releases unfinished work.
// exercises: task-group, lexical-scope, conditional-join
// questions: compiler.md §170 rule 7, §94
// tsc: accepts
// tier-policy: both tiers trap
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
// expected-trap: task-group at the declaring scope exit; 1 unfinished, 0 failed

async function work(): Promise<void> { await Context.suspend(); }
async function drop(join: boolean): Promise<void> {
  const group: TaskGroup = new TaskGroup();
  group.add(work());
  if (join) { await group.join(); }
  print("release unfinished group");
}
export async function main(): Promise<void> {
  await drop(false);
  print("unreached");
}
