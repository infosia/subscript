//! Fragments for type erasure and field storage.
use super::DivergenceEntry;
pub(super) const DECLAREDFIELDWITHOUTVALUE: DivergenceEntry = DivergenceEntry {
    ts: "class A { declare x: i32; }",
    subscript: "class A { x: i32 = 0; }",
    why: "No storage initialization is decided for a declare field.",
    collision: "C24",
};
pub(super) const ERASEDASSIGNABLETYPEMISMATCH: DivergenceEntry = DivergenceEntry {
    ts: "function f(xs: i32[]): void { const ys: f64[] = xs; }",
    subscript: "function f(xs: i32[]): void { const ys = xs.map((x: i32): f64 => x as f64); }",
    why: "This subset requires identical type arguments where TypeScript permits structural assignment.",
    collision: "C24",
};
pub(super) const NONNULLABLENULLEQUALITY: DivergenceEntry = DivergenceEntry {
    ts: "function f(n: i32): void { if (n === null) {} }",
    subscript: "function f(n: i32): void { if (false) {} }",
    why: "Only nullable types carry a null value.",
    collision: "C7",
};
pub(super) const ERASEDASSIGNABLEEQUALITY: DivergenceEntry = DivergenceEntry {
 ts: "function f(xs:i32[],ys:f64[]):void{xs === ys;}",
 subscript: "no equivalent: Arrays with different element layouts cannot share an identity.",
 why: "Equality requires an admitted scalar type; TypeScript compares types after numeric erasure.",
 collision: "C24",
};

pub(super) const DYNAMICIMPORTCALL: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\nimport(\"./other\");\n// file: other.ts\nexport const x:i32=1;",
    subscript: "// file: main.ts\nimport { x } from \"./other\";\n// file: other.ts\nexport const x: i32 = 1;",
    why: "The module surface resolves named and namespace imports statically; it has no dynamic module loader.",
    collision: "C18",
};

pub(super) const ABSTRACTMETHODBODYMISSING: DivergenceEntry = DivergenceEntry {
    ts: "abstract class B { abstract f(): i32; }",
    subscript: "class B { f(): i32 { return 0; } }",
    why: "No lowering is decided for abstract members.",
    collision: "C24",
};

pub(super) const THISSTATICMETHODMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "class C { static x:i32=0; static f():i32 { return this.x; } }",
    subscript: "class C { static x:i32=0; static f():i32 { return C.x; } }",
    why: "A static method has no instance receiver; it must name its class explicitly.",
    collision: "C24",
};

pub(super) const STRINGENUMMEMBERVALUE: DivergenceEntry = DivergenceEntry {
    ts: "enum Color { Red = \"red\" }",
    subscript: "enum Color { Red = 0 }",
    why: "Only integer enum values have a representation; no lowering is decided for string enum values.",
    collision: "compiler.md §72.1",
};

pub(super) const RESTPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "function sum(...xs: i32[]): i32 { return 0; }",
    subscript: "function sum(xs: i32[]): i32 { return 0; }",
    why: "The language has no variadic parameters; each function declares a fixed parameter count.",
    collision: "stdlib.md §14.4",
};

pub(super) const FUNCTION_VALUE_OPTIONAL_ARGUMENTS: DivergenceEntry = DivergenceEntry {
    ts: "let h = (a: i32, b: i32 = 5): i32 => a + b; h(1);",
    subscript: "let h = (a: i32, b: i32 = 5): i32 => a + b; h(1, 5);",
    why: "A function type has no optional parameter (C7).",
    collision: "C24",
};

/// Q32 and compiler.md section 259 rule 5 keep the array layouts distinct.
pub(super) const ALIAS_ARRAY_TO_STRING_ARRAY: DivergenceEntry = DivergenceEntry {
    ts: "type Mode = \"a\" | \"b\"; function f(ms: Mode[]): void { const xs: string[] = ms; }",
    subscript: "type Mode = \"a\" | \"b\"; function f(ms: Mode[]): void { const xs = ms.map((m: Mode): string => `${m}`); }",
    why: "Alias arrays hold i32 discriminants. String arrays hold string handles. A conversion requires a new array and costs depend on its length.",
    collision: "collisions.md Q32",
};
