//! Divergences of `then`, `catch`, and `finally` on a handle (compiler.md §186).

use super::DivergenceEntry;

pub(super) const PROMISEREACTIONRESULT: DivergenceEntry = DivergenceEntry {
    ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const r = await leaf().catch((e: Error): string => e.message); print(`${r}`); }",
    subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const r: i32 = await leaf().catch((e: Error): i32 => e.message.length); print(`${r}`); }",
    why: "Rejection results match the fulfillment type. A void catch on a non-void handle requires an await statement; finally requires void.",
    collision: "C8",
};

pub(super) const PROMISEREACTIONPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const n: i64 = await leaf().then((b: i64): i64 => b); print(`${n}`); }",
    subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const n: i32 = await leaf().then((b: i32): i32 => b); print(`${n}`); }",
    why: "A fulfillment value must fit its callback parameter (compiler.md §334 rule 3). Integer widths stay distinct; optional parameters stay rejected.",
    collision: "C24", // row 25
};

pub(super) const PROMISEVOIDREACTIONPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "async function quiet(): Promise<void> {}\nexport async function main(): Promise<void> { const n: i32 = await quiet().then((v): i32 => 1); print(`${n}`); }",
    subscript: "async function quiet(): Promise<void> {}\nexport async function main(): Promise<void> { const n: i32 = await quiet().then((): i32 => 1); print(`${n}`); }",
    why: "A callback of a void value takes no parameter. The language has no void parameter type.",
    collision: "C8",
};
