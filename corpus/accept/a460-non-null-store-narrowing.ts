// corpus: accept/a460-non-null-store-narrowing
// purpose: Keeps narrowing through non-null stores and reads of a const global.
// exercises: loop-head, non-null-store, const-global, const-capture
// questions: compiler.md §304, §162, collisions.md C17
// tsc: accepts; js-comparable: yes

class A { x: i32 = 1; }
class H { f: A | null = null; }
function mk(): A | null { return new A(); }
function other(): void {}
const GC: A | null = mk();
function row_const_copy(): void { let a: A | null = new A(); let s: i32 = 0; for (let i: i32 = 0; i < 2; i++) { s = s + a.x; const n = new A(); a = n; } print(`${s}`); }
function row_kept_local(): void { let a: A | null = new A(); const b: A | null = new A(); let c: A | null = b; let s: i32 = 0; for (let i: i32 = 0; i < 2; i++) { s = s + a.x + c.x; a = c; } print(`${s}`); }
function row_same_field(): void { const h = new H(); h.f = new A(); let s: i32 = 0; for (let i: i32 = 0; i < 2; i++) { s = s + h.f.x; h.f = new A(); } print(`${s}`); }
function row_other_receiver_loop(): void { const h = new H(); const k = new H(); h.f = new A(); let s: i32 = 0; for (let i: i32 = 0; i < 2; i++) { s = s + h.f.x; k.f = new A(); } print(`${s}`); }
function row_other_receiver(): void { const h = new H(); const k = new H(); h.f = new A(); if (h.f !== null) { k.f = new A(); print(`${h.f.x}`); } }
function row_break_field(): void { const h = new H(); h.f = new A(); while (true) { print(`${h.f.x}`); h.f = new A(); break; } }
function row_global_lambda(): void { if (GC === null) { return; } const g = (): i32 => GC.x; print(`${g()}`); }
function row_global_call(): void { if (GC === null) { return; } other(); print(`${GC.x}`); }
export function main(): void { row_const_copy(); row_kept_local(); row_same_field(); row_other_receiver_loop(); row_other_receiver(); row_break_field(); row_global_lambda(); row_global_call(); }
