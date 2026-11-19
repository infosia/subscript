// corpus: accept/a491-optional-statement-leaf
// purpose: Optional calls execute only at selected statement leaves.
// exercises: optional-chain, conditional, logical, lambda
// questions: compiler.md §339 rule 7
// compiler.md §339: debug dev + ship: 0.370 s; 1 C build.
// tsc: accepts
// js-comparable: yes
class C { m(): void { print("m"); } }
function g(n: i32): void { print(`${n}`); }
function run(cb: (b: boolean) => void): void { cb(true); cb(false); }
function leaves(b: boolean, c: C | null): void {
  b ? c?.m() : g(1);
  b || c?.m();
  c !== null && c?.m();
  const captured = c;
  run((flag: boolean) => flag ? captured?.m() : g(2));
}
export function main(): void {
  leaves(true, new C());
  leaves(false, null);
}
