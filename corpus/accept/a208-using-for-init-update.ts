// corpus: accept/a208-using-for-init-update
// purpose: Omits disposal after a loop that cannot fall through.
// exercises: using-declaration, scope-exit, infinite-loop
// questions: compiler section 101
// tsc: accepts; js-comparable: yes
class R { [Symbol.dispose](): void { print("dispose"); } }
function probe(): void {
  using r: R = new R();
  for (let i: i32 = 0; ; i = i + 1) { print(`${i}`); return; }
}
export function main(): void {
  probe();
  print("returned");
}
