// corpus: accept/a210-using-for-block
// purpose: Omits disposal after a loop that cannot fall through.
// exercises: using-declaration, scope-exit, infinite-loop
// questions: compiler section 101
// tsc: accepts; js-comparable: yes
class R { [Symbol.dispose](): void { print("dispose"); } }
// main returns before the infinite loop; emission still covers its body.
function probe(stop: boolean): void {
  using r: R = new R();
  if (stop) { return; }
  { for (;;) { print("unreachable"); } }
}
export function main(): void {
  probe(true);
  print("returned");
}
