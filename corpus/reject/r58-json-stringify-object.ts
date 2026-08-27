// corpus: reject/r58-json-stringify-object
// purpose: Rejects a boundary-opaque object with no static field shape.
// exercises: JSON.stringify, object, missing-static-shape
// expected-error: S014 at stringify
// questions: Q28, C7
// tsc: accepts
class Box {
  constructor() {}
}

export function main(): void {
  JSON.stringify(new Box() as object);
}
