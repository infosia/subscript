// corpus: reject/r534-conditional-handle-binding
// purpose: A conditional binding cannot discharge both handles.
// exercises: async-function, await, held-handle
// questions: compiler.md §317, §188, §257
// tsc: accepts
// expected-error: S013 at line 8
async function v(n: i32): Promise<i32> { return n; }
async function g(c: boolean): Promise<i32> { const p = v(1); const q = v(2); const r = c ? p : q; return await r; }
export function main(): void {}
