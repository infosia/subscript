// corpus: accept/a172-array-callback-mutation
// purpose: Static callbacks and function values use the same fixed Array method range.
// observable: Appends do not extend map, and removals shorten map.
// exercises: static-array-callback-loop, fixed-array-method-bound, mutation-during-map
// questions: Q22
// tsc: accepts; js-comparable: yes
let active: i32[] = [];

function grow(value: i32, index: i32): i32 {
  print(`grow:${value}:${index}`);
  if (index === 0) {
    active.push(3);
  }
  return value;
}

function shrink(value: i32, index: i32): i32 {
  print(`shrink:${value}:${index}`);
  if (index === 0) {
    active.pop();
    active.pop();
  }
  return value;
}

export function main(): void {
  active = [1, 2];
  print("grow-named");
  active.map(grow);
  print(`grown:${active.join(",")}`);

  active = [1, 2];
  const growValue: (value: i32, index: i32) => i32 = grow;
  print("grow-value");
  active.map(growValue);
  print(`grown:${active.join(",")}`);

  active = [1, 2, 3, 4];
  print("shrink-named");
  active.map(shrink);
  print(`shrunk:${active.join(",")}`);

  active = [1, 2, 3, 4];
  const shrinkValue: (value: i32, index: i32) => i32 = shrink;
  print("shrink-value");
  active.map(shrinkValue);
  print(`shrunk:${active.join(",")}`);
}
