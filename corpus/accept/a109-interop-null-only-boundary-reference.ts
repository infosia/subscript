// corpus: accept/a109-interop-null-only-boundary-reference
// interpreter: no — calls the synthetic native interop library
// purpose: Passes null to a nullable boundary-struct parameter without constructing or accessing the boundary class.
// exercises: null-only-boundary-reference, boundary-struct-pointer, referenced-type-reachability, foreign-call
// questions: OBS-1
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §36. SubBoundaryStringRecord has no non-null use: its only
// program-side position is the null argument accepted by the fixture call.

export function main(): void {
  print(`${subBoundaryStringCheck(null, 0)}`);
}
