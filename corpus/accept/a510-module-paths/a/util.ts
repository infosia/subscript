// corpus: accept/a510-module-paths/a/util
// purpose: Declares a function, a class, an enum, and a module variable with the names that b/util.ts declares.
// exercises: module-export, module-global, module-initializer, class, enum
// questions: compiler.md §354 rules 1 and 9
export let count: i32 = 10;
export function label(): string { return "a"; }
export class Shape {
  size: i32;
  constructor(size: i32) { this.size = size; }
  area(): i32 { return this.size * 2; }
}
export enum Kind { Low = 1, High = 2 }
print(`a/util init ${count}`);
