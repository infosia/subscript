// corpus: reject/r532-readonly-number-boolean-store
// purpose: Constructor stores preserve inferred number and boolean literal types.
// exercises: readonly, constructor, number, boolean
// questions: compiler.md §316, Q32
// tsc: rejects TS2322
// expected-error: S100 at line 10
class K {
  readonly n = 0;
  readonly b = false;
  constructor(c: i32) { this.n = c; this.b = true; }
}
export function main(): void { const k = new K(1); print("unreachable"); }

// tsc-version: TypeScript 5.9.2; TS2322 at line 10 (both stores), measured.
