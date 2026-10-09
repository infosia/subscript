//! Fixed examples for the listed §182 emission sites.
use crate::divergence::DivergenceEntry;

pub(crate) const GROUP: DivergenceEntry = DivergenceEntry {
    ts: "function* values(): Generator<i32> { new TaskGroup(); yield 1; }\nexport function main(): void {}",
    subscript: "async function values(): Promise<void> { const g = new TaskGroup(); await g.join(); }\nexport function main(): void {}",
    why: "A dropped generator does not execute a lexical scope exit. Put the group in an async function and await its join.",
    collision: "collisions.md C24",
};

pub(crate) const DATE: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const d = new Date(0); const g = d.getTime; }",
    subscript: "export function main(): void { const d = new Date(0); const g = (): i64 => d.getTime(); g(); }",
    why: "A Date method lowers to a direct operation. Use a lambda that calls the method on the Date value.",
    collision: "stdlib.md §3; collisions.md C24 row 12",
};

pub(crate) const FUNCTION_MAP: DivergenceEntry = DivergenceEntry {
    ts: "function id(x: i32): i32 { return x; } export function main(): void { [1].map((x: i32): ((x: i32) => i32) => id); }",
    subscript: "function id(x: i32): i32 { return x; } export function main(): void { const fs: ((x: i32) => i32)[] = []; for (const x of [1]) { fs.push(id); } }",
    why: "map does not support a function result. Use a typed array and push each function in a for-of loop.",
    collision: "stdlib.md §9",
};

pub(crate) const ASYNC_MAP: DivergenceEntry = DivergenceEntry {
    ts: "async function probe(): Promise<void> { const hs = [1].map(async (v: i32): Promise<i32> => v); await Promise.all(hs); }\nexport function main(): void {}",
    subscript: "class Job { n: i32; constructor(n: i32) { this.n = n; } async run(): Promise<i32> { return this.n; } } async function probe(): Promise<void> { const hs: Promise<i32>[] = []; for (const v of [1]) { const job = new Job(v); hs.push(job.run()); } await Promise.all(hs); }\nexport function main(): void {}",
    why: "map cannot transfer a counted callback result. Use a for-of loop, push each handle, and await the handle array.",
    collision: "compiler.md §171",
};

pub(crate) const THEN: DivergenceEntry = DivergenceEntry {
    ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().then(null, (e: Error): i32 => 0); }",
    subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch((e: Error): i32 => 0); const s: string = await leaf().then((x: i32): string => `${x}`); }",
    why: "then takes a fulfillment callback and an optional rejection callback, and no type arguments. Use catch for a rejection callback alone.",
    collision: "collisions.md C8; compiler.md §186",
};

pub(crate) const CATCH: DivergenceEntry = DivergenceEntry {
    ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch<i32>((e: Error): i32 => 0); }",
    subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch((e: Error): i32 => 0); }",
    why: "catch takes one callback with an optional Error parameter, and no type arguments.",
    collision: "collisions.md C8; compiler.md §186",
};

pub(crate) const FINALLY: DivergenceEntry = DivergenceEntry {
    ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().finally(); }",
    subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().finally((): void => {}); }",
    why: "finally takes one callback with no parameter, and no type arguments.",
    collision: "collisions.md C8; compiler.md §180, §186",
};

pub(crate) const RECEIVER: DivergenceEntry = DivergenceEntry {
    ts: "class C { n: i32 = 1; f(): void { const job = async (): Promise<i32> => this.n; } }\nexport function main(): void {}",
    subscript: "class C { n: i32 = 1; f(): void { const self = this; const job = async (): Promise<i32> => self.n; } }\nexport function main(): void {}",
    why: "The receiver must appear as an explicit const capture or a class field.",
    collision: "collisions.md C24",
};

pub(crate) const REACTION_RECEIVER: DivergenceEntry = DivergenceEntry {
    ts: "async function leaf(): Promise<i32> { return 1; }\nclass C { n: i32 = 5; async run(): Promise<i32> { return await leaf().then((v: i32): i32 => v + this.n); } }\nexport async function main(): Promise<void> { print(`${await new C().run()}`); }",
    subscript: "async function leaf(): Promise<i32> { return 1; }\nclass C { n: i32 = 5; async run(): Promise<i32> { const n = this.n; return await leaf().then((v: i32): i32 => v + n); } }\nexport async function main(): Promise<void> { print(`${await new C().run()}`); }",
    why: "A reaction callback owns const captures. A direct this capture remains outside the accepted surface.",
    collision: "collisions.md C24; compiler.md §186",
};

pub(crate) const REACTION_RECEIVER_RULE: &str =
    "A reaction callback owns const captures. Copy the needed field into a const.";

pub(crate) const RECEIVER_MESSAGE: &str = "an async arrow cannot capture `this` directly; copy the receiver into a const or use a class field with an async method";
pub(crate) const RECEIVER_RULE: &str = "An async arrow owns const captures. A direct this capture remains outside the accepted surface.";
