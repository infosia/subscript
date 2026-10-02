// corpus: accept/a316-field-initializer-earlier-fields
// purpose: Reads earlier initialized fields and orders construction effects.
// exercises: field-initializer, this-binding, generic-class, value-class
// questions: §147, collisions.md C9
// tsc: accepts; js-comparable: yes
function ValueType<T>(value: T): T { return value; }
class A { x: i32 = 3; y: i32 = this.x + 1; }
class LaterAssignment {
  x: i32 = 3;
  y: i32 = this.x + 1;
  constructor() { this.x = 9; }
}
@ValueType
class V { x: i32 = 5; y: i32 = this.x + 1; }
class G<T> { x: i32 = 7; y: i32 = this.x + 1; value: T; constructor(value: T) { this.value = value; } }
class StaticRead { static s: i32 = 10; x: i32 = 3; y: i32 = this.x + StaticRead.s; }
class B { v: i32 = 17; }
class MemberRead { b: B = new B(); y: i32 = this.b.v; }
function witness(label: string, value: i32): i32 { print(label); return value; }
class Ordered {
  x: i32 = witness("first", 20);
  y: i32 = witness("second", this.x + 1);
  argument: i32;
  constructor(argument: i32) { this.argument = argument; print("body"); }
}
export function main(): void {
  print(`${new A().y}`);
  const assigned: LaterAssignment = new LaterAssignment();
  print(`${assigned.x} ${assigned.y}`);
  print(`${new V().y}`);
  const generic: G<i32> = new G<i32>(2);
  print(`${generic.y} ${generic.value}`);
  print(`${new StaticRead().y}`);
  print(`${new MemberRead().y}`);
  const ordered: Ordered = new Ordered(witness("argument", 30));
  print(`${ordered.x} ${ordered.y} ${ordered.argument}`);
}
