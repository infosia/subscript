// corpus: reject/r273-export-star/main
// purpose: Rejects a wildcard export with the C18 divergence block.
// exercises: module-export, module-import
// questions: compiler.md §128, C18
// tsc: accepts
// expected-error: S100 at line 8, the export specifier (C18)

export * from "./lib";
export function main(): void {}
