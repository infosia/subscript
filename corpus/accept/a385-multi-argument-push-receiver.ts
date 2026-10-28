// corpus: accept/a385-multi-argument-push-receiver
// purpose: A push with two or more arguments appends to the array that its receiver named before the arguments ran.
// exercises: array-push, evaluation-order
// questions: compiler.md §201.1 rule 4
// tsc: accepts; js-comparable: yes
let g: i32[] = [];

function resetGlobal(): i32 {
  g = [];
  return 7;
}

class Store {
  static items: i32[] = [];
}

function resetStatic(): i32 {
  Store.items = [];
  return 8;
}

export function main(): void {
  const oldGlobal = g;
  g.push(1, resetGlobal());
  print(`${oldGlobal.join(",")} | ${g.join(",")}`);

  const oldStatic = Store.items;
  const n = Store.items.push(2, resetStatic(), 3);
  print(`${n} ${oldStatic.join(",")} | ${Store.items.join(",")}`);

  let local: i32[] = [];
  const oldLocal = local;
  local.push(4, (local = [9]).length);
  print(`${oldLocal.join(",")} | ${local.join(",")}`);
}
