// corpus: accept/a294-async-host-order
// purpose: Pins one ordered runner call per async host target.
// exercises: async-root-order, host-alias
// questions: compiler.md §129, Q34, C8
// tsc: accepts; js-comparable: no C8: The corpus runner starts async exports differently.
export async function zeta(): Promise<void> { print("zeta"); }
export async function alpha(): Promise<void> { print("alpha"); }
export function main(): void { print("main"); }
