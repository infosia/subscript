// corpus: accept/a320-namespace-shadow/lib
// purpose: Exports the namespace member that locals shadow.
// exercises: module-export
// questions: compiler.md §148
// tsc: accepts
export let count: i32 = 4;
