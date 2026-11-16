// corpus: accept/a464-readonly-literal-field
// purpose: Readonly literal fields follow compiler.md §316.
// observable: Member comparisons and switch labels retain their values.
// exercises: readonly, static, string-alias, switch
// questions: compiler.md §316, Q32
// tsc: accepts; js-comparable: yes
type Mode = "aaa" | "bbb";
class K {
  static readonly Z = "zzz"; static readonly A = "aaa";
  readonly w = "zzz"; readonly b = "bbb";
  constructor() { this.w = "zzz"; this.b = "bbb"; }
  static readonly S: string = "zzz";
}
function show(m: Mode, k: K): void {
  print(`${m === K.A}`);
  print(`${m === k.b}`);
  print(`${m === K.S}`);
  switch (m) { case K.A: print("A"); break; case k.b: print("b"); break; }
}
function make(): K { print("receiver"); return new K(); }
function receiver(m: Mode): void { print(`${m === make().b}`); }
export function main(): void { const k = new K(); show("aaa", k); show("bbb", k); receiver("bbb"); }
