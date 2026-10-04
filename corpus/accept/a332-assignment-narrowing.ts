// corpus: accept/a332-assignment-narrowing
// purpose: Narrows nullable locations after non-null writes.
// exercises: initializer, assignment, null-check, conditional, nullable-call
// questions: compiler.md §159, collisions.md C7, C17
// tsc: accepts; js-comparable: yes

class A { x: i32 = 1; constructor(x: i32 = 1) { this.x = x; } value(): i32 { return this.x; } }
class H { a: A | null = null; }
let g: A | null = null;
function cond(): boolean { return true; }
function take(a: A): i32 { return a.x; }
function value(n: i32): i32 { return n; }
function row_let(): void { let a: A | null = new A(); print(`${a.x}`); print(`${a.value()}`); print(`${take(a)}`); }
function row_const(): void { const a: A | null = new A(); print(`${a.x}`); print(`${a.value()}`); print(`${take(a)}`); }
function row_assign(): void { let a: A | null = null; a = new A(); print(`${a.x}`); print(`${a.value()}`); print(`${take(a)}`); }
function row_guard(): void { let a: A | null = null; if (a === null) { a = new A(); } print(`${a.x}`); print(`${a.value()}`); print(`${take(a)}`); }
function row_field(): void { const h = new H(); h.a = new A(); print(`${h.a.x}`); print(`${h.a.value()}`); h.a = new A(); print(`${take(h.a)}`); }
function row_global(): void { g = new A(); print(`${g.x}`); print(`${g.value()}`); g = new A(); print(`${take(g)}`); }
function row_conditional(): void { let a: A | null = null; a = cond() ? new A() : new A(); print(`${a.x}`); print(`${a.value()}`); print(`${take(a)}`); }
function function_value(): void { let f: ((n: i32) => i32) | null = value; print(`${f(1)}`); f = null; f = value; print(`${f(2)}`); }
function field_guard(): void { const h = new H(); if (h.a === null) { h.a = new A(); } print(`${h.a.x}`); }
function global_guard(): void { g = null; if (g === null) { g = new A(); } print(`${g.x}`); }
function field_conditional(): void { const h = new H(); h.a = cond() ? new A() : new A(); print(`${h.a.x}`); }
function global_conditional(): void { g = cond() ? new A() : new A(); print(`${g.x}`); }
function assignment_consumers(): void {
    let a: A | null = null;
    const b = (a = new A(2)); print(`${b.x}`);
    print(`${(a = new A(2)).x}`);
    const typed: A = (a = new A(2)); print(`${typed.x}`);
    let f: ((n: i32) => i32) | null = null;
    const k = (f = value); print(`${k(1)}`);
    const h = new H(); const t = (h.a = new A(3)).x; print(`${t}`);
    if ((a = new A()).x > 0) { print("assignment-if"); }
    print(`${cond() ? (a = new A(4)).x : (a = new A(5)).x}`);
    print(`${!cond() ? (a = new A(4)).x : (a = new A(5)).x}`);
}
function mk(n: i32): A | null { return n > 0 ? new A(n) : null; }
function assignment_checks(): void {
    let a: A | null = null; let n: i32 = 2;
    while ((a = mk(n)) !== null) { print(`${a.x}`); n -= 1; }
    if ((a = mk(3)) !== null && a.x > 0) { print(`${a.x}`); }
    if ((a = mk(4)) === null) { print("null"); } else { print(`${a.x}`); }
    if ((a = mk(5)) != null) { print(`${a.x}`); }
    if (null !== (a = mk(6))) { print(`${a.x}`); }
    if ((a = mk(0)) === null) { print("assignment-null"); }
    const h = new H();
    if ((h.a = mk(7)) !== null && h.a.x > 0) { print(`${h.a.x}`); }
}
class Slot<T> { v: T | null = null; }
function generic_assignment_check<T>(unused: T, s: Slot<A>): i32 {
    if ((s.v = new A(4)) !== null) { return s.v.x; }
    return 0;
}
function nonnull_assignment_checks(): void {
    let a: A | null = null; const b = new A(8);
    if ((a = b) !== null) { print(`${a.x}`); }
    let cur: A | null = null; const items: A[] = [new A(9)];
    if ((cur = items[0]) === null) { print("null"); } else { print(`${cur.x}`); }
    const ok = (a = new A()) !== null; print(`${ok}`);
    const either = !cond() || (a = new A()) !== null; print(`${either}`);
    print(`${generic_assignment_check<i32>(0, new Slot<A>())}`);
    const next: A | null = new A(10);
    while ((cur = next) !== null) { print(`${cur.x}`); break; }
}
export function main(): void { nonnull_assignment_checks(); assignment_consumers(); assignment_checks(); row_let(); row_const(); row_assign(); row_guard(); row_field(); row_global(); row_conditional(); function_value(); field_guard(); global_guard(); field_conditional(); global_conditional(); }
