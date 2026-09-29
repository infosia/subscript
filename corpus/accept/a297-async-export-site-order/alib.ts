// corpus: accept/a297-async-export-site-order/alib
// purpose: Supplies async library targets for entry export-site ordering.
// exercises: async-roots, re-exports, input-order
// questions: compiler.md §129, Q34, C8
// tsc: accepts; js-comparable: no C8: The corpus runner starts async exports differently.
export async function beta(): Promise<void> { print("beta"); }
