// corpus: reject/r530-readonly-literal-nonmember
// purpose: Nonmember readonly literal fields retain the alias diagnostic.
// exercises: readonly, static, string-alias
// questions: compiler.md §316, Q32
// tsc: rejects TS2367
// expected-error: S100 at line 15
// additional-error: S100 at line 16
type Mode = "aaa" | "bbb";
class K {
  static readonly Z = "zzz"; static readonly A = "aaa";
  readonly w = "zzz"; readonly b = "bbb";
  static readonly S: string = "zzz";
}
function compare(m: Mode, k: K): void {
  m === K.Z;
  m === k.w;
}
export function main(): void { compare("aaa", new K()); }

// tsc-version: TypeScript 5.9.2; TS2367 at lines 15 and 16, measured.
