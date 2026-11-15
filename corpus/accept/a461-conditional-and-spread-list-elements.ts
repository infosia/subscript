// corpus: accept/a461-conditional-and-spread-list-elements
// purpose: Conditional and spread elements take part in function list type selection.
// observable: Each element is called, so each adapter prints.
// exercises: function-value, array-literal, conditional, spread
// questions: compiler.md §305, §248
// tsc: accepts; js-comparable: yes
function none(): void {
  print("none");
}

function one(x: i32): void {
  print(`one ${x}`);
}

function nv(x: i32): i32 {
  print(`nv ${x}`);
  return x;
}

function row_conditional(b: boolean): void {
  const hs = [none, b ? one : nv];
  hs[0](1);
  hs[1](2);
}

function row_conditional_same(b: boolean): void {
  const hs = [none, b ? one : one];
  hs[0](3);
  hs[1](3);
}

function row_spread(): void {
  const base = [one];
  const hs = [none, ...base];
  hs[0](4);
  hs[1](5);
}

function row_spread_first(): void {
  const base = [none];
  const hs = [...base, one];
  hs[0](6);
  hs[1](7);
}

export function main(): void {
  row_conditional(true);
  row_conditional(false);
  row_conditional_same(true);
  row_spread();
  row_spread_first();
}
