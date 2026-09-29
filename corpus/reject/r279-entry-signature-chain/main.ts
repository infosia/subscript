// corpus: reject/r279-entry-signature-chain/main
// purpose: Rejects an invalid signature at the exposing export.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts
// collision: C18
// expected-error: S100 at line 8
export { read as expose } from "./bridge";
export function main(): void {}
