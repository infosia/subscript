// corpus: accept/a109-interop-null-only-boundary-reference
// purpose: Passes null to a nullable boundary-struct parameter without constructing or accessing the boundary class.
// exercises: null-only-boundary-reference, boundary-struct-pointer, referenced-type-reachability, foreign-call
// questions: OBS-1
// tsc: accepts
// compiler.md §36. SubBoundaryStringRecord has no non-null use: its only
// program-side position is the null argument accepted by the fixture call.

export function main(): void {
  print(`${subBoundaryStringCheck(null, 0)}`);
}
