// corpus: accept/a296-async-main-alias
// purpose: Pins one ordered runner call per async host target.
// exercises: async-root-order, host-alias
// questions: compiler.md §129, Q34, C8
// tsc: accepts; js-comparable: no C8: The corpus runner starts async exports differently.
export async function main(): Promise<void> { print("main"); }
export { main as start };
