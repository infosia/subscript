// corpus: reject/r295-accessor-compound-type
// purpose: Rejects a string result assigned through a numeric accessor.
// exercises: accessor, compound-assignment
// questions: compiler.md §143
// tsc: rejects TS2322
// expected-error: S100 at line 14, The string result cannot be assigned to the numeric setter.

class Box {
  get p(): i32 { return 1; }
  set p(v: i32) {}
}
export function main(): void {
  const o = new Box();
  o.p += "a";
}
