// corpus: accept/a442-a-conditional-in-a-statement-place-takes-void-leaves
// purpose: A conditional, `&&`, or `||` in a statement place takes `void` leaves, and each leaf runs only when its branch is taken.
// observable: each leaf prints a line of its own, and each form runs with both truth values.
// exercises: void, conditional, logical-operator, lambda, nullable-function
// questions: compiler.md §258, §237, §238, collisions.md C21
// tsc: accepts; js-comparable: yes
function g(n: i32): void {
  print(`g ${n}`);
}

function k(): i32 {
  print("k");
  return 7;
}

function run(cb: (n: i32) => void): void {
  cb(1);
  cb(0);
}

type CB = (n: i32) => void;

class H {
  cb: CB | null = null;
}

function forms(b: boolean, n: i32): void {
  print(`b ${b} n ${n}`);
  b && g(1);
  b || g(2);
  b ? g(3) : k();
  n > 1 ? g(4) : n > 0 ? g(5) : print("z");
  (n > 0 ? g(6) : g(7));
}

export function main(): void {
  forms(true, 2);
  forms(false, 1);
  forms(false, 0);
  const h = new H();
  h.cb !== null && h.cb(8);
  h.cb = (n: i32): void => g(n);
  h.cb !== null && h.cb(9);
  const f = (n: i32) => n > 0 ? g(n) : print("e");
  f(10);
  f(0);
  run((n: i32) => n > 0 && g(n + 10));
}
