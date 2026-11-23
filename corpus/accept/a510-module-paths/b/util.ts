// corpus: accept/a510-module-paths/b/util
// purpose: Declares a function, a class, an enum, and a module variable with the names that a/util.ts declares.
// exercises: module-export, module-global, module-initializer, class, enum
// questions: compiler.md §354 rules 1 and 9
export let count: i32 = 20;
export function label(): string { return "b"; }
export class Shape {
  size: i32;
  constructor(size: i32) { this.size = size; }
  area(): i32 { return this.size * 3; }
}
export enum Kind { Low = 5, High = 6 }
print(`b/util init ${count}`);
