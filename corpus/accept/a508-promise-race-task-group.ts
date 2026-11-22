// corpus: accept/a508-promise-race-task-group
// purpose: A void race is a task of a TaskGroup; the join waits for the race, and the input that lost runs on.
// exercises: promise-race, task-group, try-catch
// questions: compiler.md §350 rules 1, 3, and 8, §170
// tsc: accepts
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
//
// The join completes with the race, not with the input that lost. The
// input that lost prints after the join. An exception that wins the race
// is the failure of the group.
async function nothing(): Promise<void> { return; }
async function wait(turns: i32): Promise<void> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
}
async function step(tag: string, turns: i32): Promise<void> {
  await wait(turns);
  print(`step ${tag}`);
}
async function fail(tag: string, turns: i32): Promise<void> {
  await wait(turns);
  throw new Error(tag);
}

export async function main(): Promise<void> {
  const group: TaskGroup = new TaskGroup();
  group.add(Promise.race([step("g1", 6), step("g2", 1)]));
  await group.join();
  print("joined");
  await wait(12);

  const failing: TaskGroup = new TaskGroup();
  failing.add(Promise.race([step("late", 6), fail("won", 1)]));
  try { await failing.join(); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  await wait(12);
  print("end");
}
