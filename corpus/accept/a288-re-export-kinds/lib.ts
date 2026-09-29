// corpus: accept/a288-re-export-kinds/lib
// purpose: Declares every export kind and owns the mutable global storage.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

export function f(): i32 { return 1; }
export class Box { value: i32 = 3; }
export let count: i32 = 4;
export function bump(): void { count += 1; }
export enum Kind { Value = 6 }
export type Label = "ready" | "done";
export function pick<T>(value: T): T { return value; }
export class Holder<T> {
  value: T;
  constructor(value: T) { this.value = value; }
}
