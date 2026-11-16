// corpus: reject/r535-conditional-handle-value
// purpose: A conditional operand cannot discharge both parameters.
// exercises: async-function, await, held-handle
// questions: compiler.md §317, §188, §257
// tsc: accepts
// expected-error: S013 at line 8
async function v(n: i32): Promise<i32> { return n; }
async function h(c: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { return await (c ? p : q); }
export function main(): void {}
