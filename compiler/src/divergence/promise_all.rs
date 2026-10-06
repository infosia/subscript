use super::DivergenceEntry;

pub(super) const PROMISEALLVOIDVALUE: DivergenceEntry = DivergenceEntry {
    ts: "async function quiet(): Promise<void> {}\nexport async function main(): Promise<void> { const jobs: Promise<void>[] = [quiet()]; const xs = await Promise.all(jobs); print(`${xs.length}`); }",
    subscript: "async function quiet(): Promise<void> {}\nexport async function main(): Promise<void> { const jobs: Promise<void>[] = [quiet()]; await Promise.all(jobs); }",
    why: "The language has no void[] value.",
    collision: "C24",
};

pub(super) const PROMISEALLTYPEARGUMENTS: DivergenceEntry = DivergenceEntry {
    ts: "async function value(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const jobs: Promise<i32>[] = [value()]; await Promise.all<i32>(jobs); }",
    subscript: "async function value(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const jobs: Promise<i32>[] = [value()]; await Promise.all(jobs); }",
    why: "Promise.all derives its result type from its input array; explicit type arguments are outside the admitted form.",
    collision: "compiler.md §166",
};

pub(super) const PROMISEALLINPUT: DivergenceEntry = DivergenceEntry {
    ts: "export async function main(): Promise<void> { const xs: i32[] = [1]; await Promise.all(xs); }",
    subscript: "async function value(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const jobs: Promise<i32>[] = [value()]; await Promise.all(jobs); }",
    why: "Promise.all takes a homogeneous array of async handles. Other iterables, thenables, and ordinary values have no aggregate registration.",
    collision: "compiler.md §166",
};

pub(super) const PROMISEALLCOUNTEDRESULT: DivergenceEntry = DivergenceEntry {
    ts: "async function value(): Promise<i32> { return 7; }\nasync function result(): Promise<Promise<i32>[]> { const h: Promise<i32> = value(); const hs: Promise<i32>[] = [h]; await h; return hs; }\nexport async function main(): Promise<void> { const jobs: Promise<Promise<i32>[]>[] = [result()]; await Promise.all(jobs); }",
    subscript: "async function value(): Promise<i32> { return 7; }\nasync function result(): Promise<Promise<i32>[]> { const h: Promise<i32> = value(); const hs: Promise<i32>[] = [h]; await h; return hs; }\nexport async function main(): Promise<void> { const xs: Promise<i32>[] = await result(); await xs[0]; }",
    why: "An aggregate copies result bytes. An async handle or an array of async handles requires the counted store path.",
    collision: "C24",
};
