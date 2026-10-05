// corpus: accept/a333-narrowing-flow
// purpose: Keeps narrowing facts through negation, loops, and lambda captures.
// exercises: negation, loop-head, loop-exit, const-capture
// questions: compiler.md §162, collisions.md C7, C17
// tsc: accepts; js-comparable: yes

class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
function cond(): boolean { return true; }
function row_negation(): void { const a = mk(); if (!(a === null)) { print(`${a.x}`); } }
function row_negated_return(): i32 { const a = mk(); if (!(a !== null)) { return 0; } print(`${a.x}`); return 1; }
function row_assignment_negation(): void { let a: A | null = null; if (!((a = mk()) === null)) { print(`${a.x}`); } }
function row_loop_head(): void { let e: A | null = new A(); let s: i32 = 0; for (let i: i32 = 0; i < 2; i++) { s = s + e.x; e = new A(); } print(`${s}`); }
function row_break_exit(): void { let a: A | null = null; while (true) { a = new A(); if (cond()) { break; } } print(`${a.x}`); }
function row_condition_exit(): void { let a = mk(); while (a === null) { a = mk(); } print(`${a.x}`); }
function row_guarded_break(p: A | null): void { let a = p; for (;;) { if (a !== null) { break; } a = mk(); } print(`${a.x}`); }
function row_lambda_guard(): void { const a = mk(); if (a === null) { return; } const xs: i32[] = [2]; const ys = xs.map((v: i32): i32 => v + a.x); print(`${ys[0]}`); }
function row_lambda_initializer(): void { const a: A | null = new A(); const g = (): i32 => a.x; print(`${g()}`); }
export function main(): void { row_negation(); row_negated_return(); row_assignment_negation(); row_loop_head(); row_break_exit(); row_condition_exit(); row_guarded_break(null); row_lambda_guard(); row_lambda_initializer(); }
