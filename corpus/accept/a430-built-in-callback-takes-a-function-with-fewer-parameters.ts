// corpus: accept/a430-built-in-callback-takes-a-function-with-fewer-parameters
// purpose: A built-in callback place takes a lambda literal or a named function with fewer parameters than the method passes.
// observable: each callback prints or counts, so the number of calls is visible.
// exercises: function-value, lambda, forEach, map, filter, reduce, Map, Set, FixedArray, conditional
// questions: compiler.md §244, §243, §242
// tsc: accepts; js-comparable: yes
let count: i32 = 0;

function none(): void {
  print("none");
}

function one(x: i32): void {
  print(`one${x}`);
}

function two(x: i32, i: i32): void {
  print(`two${x}@${i}`);
}

export function main(): void {
  const b: boolean = true;
  const m = new Map<string, i32>();
  m.set("a", 1);
  m.set("b", 2);
  const s = new Set<i32>();
  s.add(5);
  [1, 2].forEach(() => {
    count++;
  });
  print(`count=${count}`);
  [1].forEach(none);
  print(`${[1, 2].map((): i32 => 0).join(",")}`);
  print(`${[1, 2, 3].filter((): boolean => true).length}`);
  m.forEach((v: i32) => print(`v${v}`));
  m.forEach(() => print("m"));
  s.forEach(() => print("s"));
  print(`${[1, 2].reduce((acc: i32): i32 => acc + 1, 0)}`);
  [6].forEach(b ? two : (x: i32): void => {});
  [7].forEach(b ? one : none);
  const fixed: FixedArray<i32, 3> = [1, 2, 3];
  fixed.forEach(() => {
    count++;
  });
  print(`count=${count}`);
}
