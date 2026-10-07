// corpus: accept/a342-awaited-nested-closure
// purpose: An awaited callback retains its caller's nested closure environment.
// exercises: coroutine, closure, reference-capture
// questions: compiler.md §172 rule 7
// tsc: accepts
// js-comparable: yes

async function tick(): Promise<void> {}
async function later(cb: () => i32): Promise<i32> { await tick(); return cb(); }
class Box { n: i32; constructor(n: i32) { this.n = n; } }
async function reference(): Promise<void> {
    const n = new Box(7);
    const f = (): i32 => n.n;
    const g = (): i32 => f() + 1;
    const r = await later(g);
    print(`${r}`);
}
export async function main(): Promise<void> {
    const n: i32 = 7;
    const f = (): i32 => n;
    const g = (): i32 => f() + 1;
    const r = await later(g);
    print(`${r}`);
    await reference();
}
