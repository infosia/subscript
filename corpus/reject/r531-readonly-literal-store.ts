// corpus: reject/r531-readonly-literal-store
// purpose: Constructor stores preserve inferred readonly literal types.
// exercises: readonly, constructor, string
// questions: compiler.md §316, Q32
// tsc: rejects TS2322
// expected-error: S100 at line 9
class K {
  readonly w = "zzz";
  constructor() { this.w = "yyy"; }
}
export function main(): void { print(new K().w); }

// tsc-version: TypeScript 5.9.2; TS2322 at line 9, measured.
