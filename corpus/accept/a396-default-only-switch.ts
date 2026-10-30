// corpus: accept/a396-default-only-switch
// purpose: A default-only switch evaluates its string or i32 value once before its branch.
// exercises: switch, default, effects
// questions: compiler.md §211 rules 1-2
// tsc: accepts; js-comparable: yes
let effects: i32 = 0;

function text(): string {
  effects += 1;
  print(`text:${effects}`);
  return "value";
}

function number(): i32 {
  effects += 1;
  print(`number:${effects}`);
  return 7;
}

export function main(): void {
  switch (text()) {
    default:
      print(`string-default:${effects}`);
  }
  switch (number()) {
    default:
      print(`i32-default:${effects}`);
  }
  print(`effects:${effects}`);
}
