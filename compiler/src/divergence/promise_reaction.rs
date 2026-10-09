//! Divergences of `then`, `catch`, and `finally` on a handle (compiler.md §186).

use super::DivergenceEntry;

pub(super) const PROMISEREACTIONRESULT: DivergenceEntry = DivergenceEntry {
    ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const r = await leaf().catch((e: Error): string => e.message); print(`${r}`); }",
    subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const r: i32 = await leaf().catch((e: Error): i32 => e.message.length); print(`${r}`); }",
    why: "A rejection callback returns the value type of its handle, and a finally callback returns void. The language has no union result.",
    collision: "C8",
};

pub(super) const PROMISEREACTIONPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "class Foo { n: i32 = 1; }\nasync function leaf(): Promise<Foo> { return new Foo(); }\nexport async function main(): Promise<void> { const n: i32 = await leaf().then((b: Foo | null): i32 => b === null ? 0 : b.n); print(`${n}`); }",
    subscript: "class Foo { n: i32 = 1; }\nasync function leaf(): Promise<Foo> { return new Foo(); }\nexport async function main(): Promise<void> { const n: i32 = await leaf().then((b: Foo): i32 => b.n); print(`${n}`); }",
    why: "A callback parameter has the value type of the handle, or Error for a rejection callback. Function types have no variance and no optional parameter.",
    collision: "C24", // row 25
};

pub(super) const PROMISEVOIDREACTIONPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "async function quiet(): Promise<void> {}\nexport async function main(): Promise<void> { const n: i32 = await quiet().then((v): i32 => 1); print(`${n}`); }",
    subscript: "async function quiet(): Promise<void> {}\nexport async function main(): Promise<void> { const n: i32 = await quiet().then((): i32 => 1); print(`${n}`); }",
    why: "A callback of a void value takes no parameter. The language has no void parameter type.",
    collision: "C8",
};
