// corpus: accept/a318-namespace-import/lib
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Exports every namespace member kind.
export let count: i32 = 4;
export function bump(): void { count += 1; }
export function add(x: i32, y: i32): i32 { return x + y; }
export class C { value: i32 = 7; }
export class G<T> { value: T; constructor(value: T) { this.value = value; } }
export enum E { M = 9 }
export type A = "ok" | "no";
export function id<T>(value: T): T { return value; }
