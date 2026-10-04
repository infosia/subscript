// corpus: reject/r350-value-type-constructor
// purpose: Rejects a ValueType class with a private constructor.
// exercises: member-modifiers, ValueType
// questions: compiler.md §155 rule 6
// tsc: rejects TS1238, TS1270
// expected-error: S100 at line 8, Rejects a private constructor at the ValueType decorator.

@ValueType
class V {
  x: i32 = 0;
  private constructor() {}
  static make(): V { return new V(); }
}
export function main(): void { V.make(); }
