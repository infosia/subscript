//! Fragment records for the `undefined` result spelling (compiler.md §195).

use super::DivergenceEntry;

pub(super) const UNDEFINED_RESULT_POSITION: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction absent(): boolean { const b: Box | undefined = pick(); return b === undefined; }",
    subscript: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction absent(): boolean { const b = pick(); return b === undefined; }",
    why: "`T | undefined` is the result type of a function declaration or a class method; every other position spells the absence `T | null`.",
    collision: "C7",
};

pub(super) const UNDEFINED_NULL_UNION: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nfunction pick(): Box | null | undefined { return null; }",
    subscript: "class Box { v: i32 = 1; }\nfunction pick(): Box | null { return null; }",
    why: "A nullable type has one absence spelling; `null` and `undefined` share the one null pointer of `T | null`.",
    collision: "C7",
};

pub(super) const UNDEFINED_SPELLING_OUTSIDE_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction count(): i32 { const xs = [pick()]; return xs.length; }",
    subscript: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction count(): i32 { const xs: Box[] = []; const b = pick(); if (b !== undefined) { xs.push(b); } return xs.length; }",
    why: "The `undefined` spelling is the whole type of a value or a local; containers, type arguments, closure results, and storage hold `T | null`.",
    collision: "C7",
};

pub(super) const UNDEFINED_RESULT_RETURN: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nfunction pick(f: boolean): Box | undefined { if (f) { return; } }",
    subscript: "class Box { v: i32 = 1; }\nfunction pick(f: boolean): Box | undefined { if (f) { return new Box(); } return undefined; }",
    why: "A function whose result is `T | undefined` names its absent result with `return undefined;`.",
    collision: "C7",
};

pub(super) const STRICT_EQUALITY_ACROSS_SPELLINGS: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction absent(): boolean { return pick() === null; }",
    subscript: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction absent(): boolean { return pick() == null; }",
    why: "JavaScript gives `false` for `===` across a `null` and an `undefined` absence; one representation holds both here, so `==` tests the absence.",
    collision: "C7",
};

pub(super) const UNDEFINED_RESULT_BOUNDARY: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nexport function pick(): Box | undefined { return undefined; }",
    subscript: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nexport function main(): void { print(`${pick() === undefined}`); }",
    why: "C has one null pointer, and the host reads an absent result as `NULL`, so a crossing function has no `T | undefined`.",
    collision: "C7",
};

pub(super) const UNDEFINED_RESULT_FUNCTION_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction absent(): boolean { const f = pick; return f() === undefined; }",
    subscript: "class Box { v: i32 = 1; }\nfunction pick(): Box | undefined { return undefined; }\nfunction absent(): boolean { return pick() === undefined; }",
    why: "A function type cannot name a `T | undefined` result, so a function whose result is `T | undefined` is only a callee.",
    collision: "C7",
};

pub(super) const NULLABLE_ELEMENT_SEARCH: DivergenceEntry = DivergenceEntry {
    ts: "class Box { v: i32 = 1; }\nfunction stored(m: Map<string, Box | null>): boolean { return m.get(\"k\") === undefined; }",
    subscript: "class Box { v: i32 = 1; }\nfunction stored(m: Map<string, Box | null>): boolean { return m.has(\"k\"); }",
    why: "On a nullable value or element, the result needs both `null` and `undefined`; a miss and a stored `null` have one representation.",
    collision: "C7",
};
