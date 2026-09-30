// corpus: accept/a303-top-level-using
// purpose: A using declaration in a top-level block disposes at the block exit.
// observable: Each body marker precedes its disposal marker.
// exercises: using-declaration, top-level-block, if, for, function
// questions: compiler.md §139
// tsc: accepts; js-comparable: yes
class R {
  name: string;
  constructor(n: string) { this.name = n; }
  [Symbol.dispose](): void { print(`d ${this.name}`); }
}
{ using r: R = new R("a"); print("in a"); }
if (true) { using q: R = new R("b"); print("in b"); }
const resources: R[] = [new R("c")];
for (let i: i32 = 0; i < 1; i++) {
  using s: R = resources[i]; print("in c");
}
function f(): void { using t: R = new R("f"); print("in f"); }
f();
export function main(): void { { using u: R = new R("m"); print("in m"); } }
