// corpus: accept/a206-using-for-return
// purpose: Omits disposal after a loop that cannot fall through.
// exercises: using-declaration, scope-exit, infinite-loop
// questions: compiler section 101
// tsc: accepts; js-comparable: yes
class R { [Symbol.dispose](): void { print("dispose"); } }
function probe(): void {
  using r: R = new R();
  for (;;) { return; }
}
export function main(): void {
  probe();
  print("returned");
}
