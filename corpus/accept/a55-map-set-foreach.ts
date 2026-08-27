// corpus: accept/a55-map-set-foreach
// purpose: Exercises fixed-arity Map/Set forEach callbacks and compiles
//          the callback trap path used by the cross-tier trap gate.
// exercises: map-foreach, set-foreach, callback-abi, callback-trap-path
// questions: Q24, Q22, C5, C6
// tsc: accepts; js-comparable: yes
let seen: string = "";
const probe: i32[] = [7];

function visitValue(value: i32, key: string): void {
  seen += `${key}:${value}|`;
  if (value < 0) {
    print(`${probe[value]}`);
  }
}

export function main(): void {
  const map: Map<string, i32> = new Map<string, i32>();
  map.set("a", 1);
  map.set("b", 2);
  map.forEach(visitValue);

  const set: Set<i32> = new Set<i32>();
  set.add(4);
  set.add(5);
  set.forEach((key: i32): void => {
    seen += `s${key}|`;
  });
  print(seen);
}
