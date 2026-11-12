// corpus: accept/a437-a-binding-named-undefined-is-a-binding
// purpose: A binding named undefined is read as the binding, in an arithmetic operand, an absence comparison, and a for-of binding.
// observable: each value is printed, so the read of each binding is visible.
// exercises: undefined, parameter, nullable, for-of
// questions: compiler.md §253.1 rules 1 and 2
// tsc: accepts; js-comparable: yes
function n(undefined: i32): i32 {
  return undefined + 1;
}

function absent(undefined: string | null): boolean {
  if (undefined === null) {
    return true;
  }
  return false;
}

export function main(): void {
  print(`n ${n(41)}`);
  print(`absent ${absent(null)} ${absent("x")}`);
  for (const undefined of [1, 2]) {
    print(`q ${undefined}`);
  }
}
