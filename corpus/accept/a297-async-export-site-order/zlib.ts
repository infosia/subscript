// corpus: accept/a297-async-export-site-order/zlib
// purpose: Supplies async library targets for entry export-site ordering.
// exercises: async-roots, re-exports, input-order
// questions: compiler.md §129, Q34, C8
// tsc: accepts; js-comparable: no C8: The corpus runner starts async exports differently.
export async function root(): Promise<void> { print("main"); }
export async function zeta(): Promise<void> { print("zeta"); }
export async function alpha(): Promise<void> { print("alpha"); }
