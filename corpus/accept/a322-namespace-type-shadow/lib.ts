// corpus: accept/a322-namespace-type-shadow/lib
// purpose: Exports a class and a constructor helper.
// exercises: module-export, class
// questions: compiler.md §148
export class C {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
export function mk(value: i32): C { return new C(value); }
