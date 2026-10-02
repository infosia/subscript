// corpus: accept/a318-namespace-import/bridge
// exercises: namespace-import, module-export
// questions: compiler.md §148
// purpose: Starts a named re-export chain.
export { add as sum } from "./lib";
