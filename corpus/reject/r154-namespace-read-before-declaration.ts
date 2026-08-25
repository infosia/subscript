// corpus: reject/r154-namespace-read-before-declaration
// purpose: Rejects an ambient namespace read before a local declaration owns the name.
// exercises: block-scope, ambient-namespace, read-before-declaration
// questions: §67
// tsc-status: stock TypeScript reports TS2339, TS2448, and TS2454 for Math.
// expected-error: S100 at the Math read

export function main(): void {
  const result: f64 = Math.abs(-2.5);
  const Math: i32 = 3;
  print(`${result}:${Math}`);
}
