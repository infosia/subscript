//! Fragments for type erasure and field storage.
use super::DivergenceEntry;
pub(super) const DECLAREDFIELDWITHOUTVALUE: DivergenceEntry = DivergenceEntry {
    ts: "class A { declare x: i32; }",
    subscript: "no equivalent; use a field initializer",
    why: "No storage initialization is decided for a declare field.",
    collision: "C24",
};
pub(super) const ERASEDASSIGNABLETYPEMISMATCH: DivergenceEntry = DivergenceEntry {
    ts: "function f(xs: i32[]): void { const ys: f64[] = xs; }",
    subscript: "no equivalent; use the identical type",
    why: "This subset requires identical type arguments where TypeScript permits structural assignment.",
    collision: "C24",
};
pub(super) const NONNULLABLENULLEQUALITY: DivergenceEntry = DivergenceEntry {
    ts: "function f(n: i32): void { if (n === null) {} }",
    subscript: "no equivalent; compare a nullable value",
    why: "Only nullable types carry a null value.",
    collision: "C7",
};
pub(super) const ERASEDASSIGNABLEEQUALITY: DivergenceEntry = DivergenceEntry {
 ts: "function f(xs:i32[],ys:f64[]):void{xs === ys;}",
 subscript: "no equivalent; use an admitted scalar comparison",
 why: "Equality requires an admitted scalar type; TypeScript compares types after numeric erasure.",
 collision: "C24",
};

pub(super) const DYNAMICIMPORTCALL: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\nimport(\"./other\");\n// file: other.ts\nexport const x:i32=1;",
    subscript: "no equivalent; use a named or namespace import",
    why: "The module surface resolves named and namespace imports statically; it has no dynamic module loader.",
    collision: "C18",
};

pub(super) const ABSTRACTMETHODBODYMISSING: DivergenceEntry = DivergenceEntry {
    ts: "abstract class B { abstract f(): i32; }",
    subscript: "no equivalent; provide a method body",
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
    ts: "const h = (a: i32, b: i32 = 5): i32 => a + b; h(1);",
    subscript: "const h = (a: i32, b: i32 = 5): i32 => a + b; h(1, 5);",
    why: "A function type has no optional parameter (C7).",
    collision: "C24",
};
