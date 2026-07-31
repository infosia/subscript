// corpus: reject/r89-literal-union-cross-alias
// purpose: Rejects assignment across nominal Q32 aliases with identical members.
// exercises: string-literal-union, nominal-alias-identity
// questions: Q32
// tsc-clean: stock TypeScript accepts this structurally; keep this entry out of tsconfig.
// expected-error: S100 type mismatch at the cross-alias initializer

type IndexFormat = "uint16" | "uint32";
type TwinFormat = "uint16" | "uint32";

export function main(): void {
  const source: IndexFormat = "uint16";
  const target: TwinFormat = source;
  print(`${target}`);
}
