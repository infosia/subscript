// corpus: accept/a446-a-capturing-lambda-that-escapes-owns-its-environment
// purpose: A lambda that escapes owns its captures.
// observable: Captures stay valid after the maker returns.
// exercises: lambda, return, field, array, Map, constructor, generic
// questions: compiler.md §274
// tsc: accepts; js-comparable: yes
interface Maker { make(n: i32): i32; }
class A implements Maker { make(n: i32): i32 { return n + 1; } }
class B implements Maker { make(n: i32): i32 { return n + 2; } }
class P { constructor(public f: () => i32) {} }
function build<D extends Maker>(d: D): P {
    const dd = d;
    return new P((): i32 => dd.make(1));
}
function returned(): () => i32 { const k: i32 = 3; return (): i32 => k; }
function field(): P { const k: i32 = 4; const p = new P(returned()); p.f = (): i32 => k; return p; }
function array(): (() => i32)[] { const k: i32 = 5; return [(): i32 => k]; }
function map(): Map<i32, () => i32> { const k: i32 = 6; const m = new Map<i32, () => i32>(); m.set(0, (): i32 => k); return m; }
class Value { n: i32 = 8; }
function counted(): () => void {
    const s: string = "value";
    const a: i32[] = [7];
    const v = new Value();
    return (): void => { print(`${s} ${a[0]} ${v.n}`); };
}
export function main(): void {
    print(`${build(new A()).f()} ${build(new B()).f()}`);
    const f = returned();
    for (let i: i32 = 0; i < 10; i++) { print(`${f()}`); }
    print(`${field().f()} ${array()[0]()}`);
    const g = map().get(0);
    if (g !== undefined) { print(`${g()}`); }
    counted()();
}
