// corpus: reject/r386-task-group-generator-body
// purpose: A dropped generator cannot release a lexical task group.
// exercises: task-group, generator, dropped-iterator
// questions: compiler.md §170 rules 2 and 7
// tsc: accepts
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
// expected-error: S009 at the group constructor

async function fail(): Promise<void> { throw new Error("lost"); }
const jobs: Promise<void>[] = [];
function consume(job: Promise<void>): void { jobs.push(job); }
function* values(): Generator<i32> {
  const group: TaskGroup = new TaskGroup();
  group.add(fail());
  yield 1;
  consume(group.join());
}
export async function main(): Promise<void> {
  for (const value of values()) { print(`${value}`); break; }
  await Context.suspend();
  print("end");
}
