// corpus: reject/r280-cstruct-renamed
// purpose: Rejects the former value-class decorator.
// exercises: ValueType decorator rename
// questions: compiler.md §130
// tsc: rejects TS2304
// expected-error: S100 at line 7, the decorator names @ValueType
@CStruct class V { x: i32 = 0; }

export function main(): void {}
