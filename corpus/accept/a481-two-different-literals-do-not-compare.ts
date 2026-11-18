// corpus: accept/a481-two-different-literals-do-not-compare
// purpose: Widened operands, equal literals, and prior literal tests compare.
// observable: Each accepted comparison prints its result.
// exercises: const, let, equality, switch, narrowing, conditional
// questions: compiler.md §330 rules 1 to 4
// tsc: accepts; js-comparable: yes
const MODE = 1;
function annotated(): boolean { const n: i32 = 1; return n === 2; }
function widened(): boolean { let m = MODE; return m === 2; }
function either(): boolean { return MODE === 1 || MODE === 2; }
function branch(): boolean {
  if (MODE === 1) { return true; } else { return MODE === 2; }
}
function returned(): boolean { if (MODE === 1) { return true; } return MODE === 2; }
function both(): boolean { return MODE !== 1 && MODE === 2; }
function nested(): boolean { return MODE === 1 ? true : (MODE === 2 ? false : true); }
function switched(): boolean {
  if (MODE === 1) { return true; }
  switch (MODE) { case 2: return false; default: return true; }
}
function defaulted(): boolean {
  switch (MODE) { case 1: return true; default: return MODE === 2; }
}
function hex(): boolean { const n = 0x10; return n === 16; }
function exponent(): boolean { const n = 1e2; return n === 100; }
function order(): boolean { return MODE < 2; }
function same(): boolean { return MODE === 1; }
export function main(): void {
  print(`annotated ${annotated()}`);
  print(`let ${widened()}`);
  print(`or ${either()}`);
  print(`if ${branch()}`);
  print(`return ${returned()}`);
  print(`and ${both()}`);
  print(`nested ${nested()}`);
  print(`switch ${switched()}`);
  print(`default ${defaulted()}`);
  print(`hex ${hex()}`);
  print(`exponent ${exponent()}`);
  print(`order ${order()}`);
  print(`same ${same()}`);
}
