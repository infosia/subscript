// corpus: accept/a295-async-host-aliases
// purpose: Pins one ordered runner call per async host target.
// exercises: async-root-order, host-alias
// questions: compiler.md §129, Q34, C8
// tsc: accepts; js-comparable: no C8: The corpus runner starts async exports differently.
async function work(): Promise<void> { print("work"); }
export { work as first, work as second };
export function main(): void { print("main"); }
