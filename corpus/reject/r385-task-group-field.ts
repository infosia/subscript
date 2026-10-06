// corpus: reject/r385-task-group-field
// purpose: A field cannot hold a lexical task group.
// exercises: task-group, field
// questions: compiler.md §170 rules 2 and 15
// tsc: accepts
// js-comparable: no C8: TaskGroup has no JavaScript counterpart.
// expected-error: S009 at the field type

class Holder {
  group: TaskGroup;
  constructor(group: TaskGroup) { this.group = group; }
}
export function main(): void {}
