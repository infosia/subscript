// corpus: accept/a156-cstruct-this-by-value
// purpose: Uses a value-class receiver as a by-value result and argument.
// exercises: value-class, method-receiver, return-by-value, argument-by-value
// questions: §62, §68
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct({ align: 8 })
class V {
  x: f32;
  y: f32;
  constructor(x: f32, y: f32) { this.x = x; this.y = y; }
  self(): V { return this; }
  dot(other: V): f32 { return this.x * other.x + this.y * other.y; }
  length(): f32 { return this.dot(this); }
}
export function main(): void {
  const value: V = new V(3.0, 4.0);
  print(`${value.self().x},${value.length()}`);
}
