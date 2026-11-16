// corpus: reject/r533-default-reads-earlier-handle
// purpose: A default cannot discharge an earlier parameter.
// exercises: async-function, await, held-handle
// questions: compiler.md §317, §188, §257
// tsc: accepts
// expected-error: S013 at line 8
async function v(n: i32): Promise<i32> { return n; }
async function f(p: Promise<i32>, q: Promise<i32> = p): Promise<i32> { return await q; }
export function main(): void {}
