// corpus: reject/r383-unjoined-task-group
// purpose: A local group must reach a join in its declaring scope.
// exercises: task-group, async-origin
// questions: compiler.md §170 rule 3, §70
// tsc: accepts
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
// expected-error: S013 at the group creation with no join

export function main(): void {
  const group: TaskGroup = new TaskGroup();
}
