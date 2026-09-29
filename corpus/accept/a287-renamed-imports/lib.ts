// corpus: accept/a287-renamed-imports/lib
// purpose: Exports each declaration kind for renamed imports.
// exercises: module-export
// questions: compiler.md §126
// tsc: accepts; js-comparable: yes

export function f(): i32 { return 1; }
export function g(): i32 { return 2; }
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
