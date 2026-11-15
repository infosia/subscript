// corpus: accept/a459-a-statement-place-drops-each-value-leaf
// purpose: A statement place drops every value leaf of a conditional, `&&`, and `||`, even with no void leaf.
// observable: printing value functions show both branches, evaluation order, and skipped right operands.
// exercises: conditional, logical-operator, assignment, lambda, counted-value
// questions: compiler.md §300, §258
// tsc: accepts; js-comparable: yes
function k(): i32 {
  print("k");
  return 1;
}

function five(): i32 {
  print("five");
  return 5;
}

function three(): i32 {
  print("three");
  return 3;
}

function one(): i32 {
  print("one");
  return 1;
}

function text(): string {
  print("s");
  return "s";
}

function run(cb: (b: boolean) => void): void {
  cb(true);
  cb(false);
}

function forms(b: boolean): void {
  let n: i32 = 0;
  print(`b ${b}`);
  b && k();
  b || k();
  // Printing counterparts of the literal and assignment rows.
  b && five();
  b && (n = three());
  print(`n ${n}`);
  b ? one() : text();
  // The exact literal rows also exercise the statement place.
  b && 5;
  b && (n = 3);
  b ? 1 : "s";
  print(`literal n ${n}`);
  // Controls, parentheses, and recursive statement places.
  b ? k() : five();
  k();
  5;
  (b ? b && one() : b || text());
}

export function main(): void {
  let s: string | null = null;
  s === null && (s = "z");
  print(s);
  s = null;
  s !== null || (s = "y");
  print(s);
  forms(true);
  forms(false);
  run((b: boolean) => b && k());
  run((b: boolean) => b || text());
  run((b: boolean) => b ? one() : text());
}
