// corpus: trap/t112-enum-switch-non-member-value
// purpose: A value that is not an enum member reaches the trap at the end of a switch over every member.
// exercises: enum, switch, return-flow, bytes-of
// questions: compiler.md §200.1 rule 2
// tsc: accepts
// js-comparable: no C2: The ValueType decorator has no JavaScript shim.
// tier-policy: all three tiers trap
// expected-trap: unreachable-reached at 20:3
enum Shape {
  Circle,
  Square,
}

@ValueType
class Wire {
  shape: Shape = Shape.Circle;
}

function sides(s: Shape): i32 {
  switch (s) {
    case Shape.Circle:
      return 0;
    case Shape.Square:
      return 4;
  }
}

export function main(): void {
  const bytes: u8[] = [9, 0, 0, 0];
  const w = Context.fromBytes<Wire>(bytes, 0);
  print("before");
  print(`${sides(w.shape)}`);
}
