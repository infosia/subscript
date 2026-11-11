// corpus: accept/a433-a-context-free-list-of-functions-takes-the-type-that-each-fits
// purpose: A context-free array literal or conditional of function values takes the type of the first value that each other value fits.
// observable: each element and each branch is called, so each adapter prints.
// exercises: function-value, array-literal, conditional, lambda
// questions: compiler.md §248, §240, §243, §246
// tsc: accepts; js-comparable: yes
function none(): void {
  print("none");
}

function one(x: i32): void {
  print(`one ${x}`);
}

function nv(x: i32): i32 {
  print(`nv ${x}`);
  return x * 2;
}

const LV = (): void => {
  print("LV");
};

function pick(n: i32): boolean {
  return n > 0;
}

function lists(): void {
  const fs = [none, one];
  for (const f of fs) {
    f(1);
  }
  const gs = [one, none];
  for (const g of gs) {
    g(2);
  }
  const hs = [LV, nv, one];
  for (const h of hs) {
    h(3);
  }
  const ls = [(): void => print("literal"), one];
  for (const l of ls) {
    l(4);
  }
}

function conditionals(n: i32): void {
  const b = pick(n);
  const f = b ? none : one;
  f(5);
  const g = b ? one : nv;
  g(6);
  const h = b ? nv : one;
  h(7);
}

export function main(): void {
  lists();
  conditionals(1);
  conditionals(-1);
}
