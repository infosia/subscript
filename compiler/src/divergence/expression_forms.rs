//! Distinct fragments for syntax and operand restrictions.
use super::DivergenceEntry;
pub(super) const LOCALCLASSDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { class C {} }",
    subscript: "class C {}\nfunction f(): void {}",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const LOCALFUNCTIONREADSENCLOSINGBINDING: DivergenceEntry = DivergenceEntry {
    ts: "function f(): i32 { const k: i32 = 1; function g(): i32 { return k; } return g(); }",
    subscript: "function f(): i32 { const k: i32 = 1; const g = (): i32 => k; return g(); }",
    why: "A local function is a module function, which has no environment for a binding of a function.",
    collision: "compiler.md §207",
};
pub(super) const LOCALENUMDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { enum E { A } }",
    subscript: "enum E { A }\nfunction f(): void {}",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const LOCALALIASDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { type A = i32; }",
    subscript: "type A = i32;\nfunction f(): void {}",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const LOCALINTERFACEDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { interface I {} }",
    subscript: "class I {}\nfunction f(): void {}",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const SOURCENAMESPACEDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "namespace N {}",
    subscript: "// file: main.ts\nimport * as N from \"./other\";\n// file: other.ts\nexport {};",
    why: "The module surface excludes namespace declarations.",
    collision: "C18",
};
pub(super) const FORINSTATEMENT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { for (const k in [1]) {} }",
    subscript:
        "function f(): void { const xs: i32[] = [1]; for (let k: i32 = 0; k < xs.length; k++) {} }",
    why: "No lowering is decided for dynamic property enumeration.",
    collision: "C24", // row 29,
};
pub(super) const LABELEDSTATEMENT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { outer: while (true) { while (true) { break outer; } } }",
    subscript: "function f(): void { let done: boolean = false; while (!done) { while (true) { done = true; break; } } }",
    why: "No lowering is decided for labeled statements.",
    collision: "C24", // row 29,
};
pub(super) const STATICBLOCKDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "class C { static {} }",
    subscript: "class C { static init(): void {} }\nC.init();",
    why: "A static block has no lowering; a named static method can initialize module state.",
    collision: "C24", // row 11,
};
pub(super) const AUTOACCESSORDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "class C { accessor x: i32 = 1; }",
    subscript: "class C { x: i32 = 1; }",
    why: "An accessor field has no lowering; a declared field supplies storage.",
    collision: "C24", // row 11,
};
pub(super) const TUPLEANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: [i32, i32]): void {}",
    subscript: "class Pair { first: i32 = 0; second: i32 = 0; }\nfunction f(x: Pair): void {}",
    why: "The language has no tuple type.",
    collision: "stdlib.md §14",
};
pub(super) const THISANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(x: this): void {} }",
    subscript: "class C { f(x: C): void {} }",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const QUERYANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "const x: i32 = 1; function f(v: typeof x): void {}",
    subscript: "const x: i32 = 1; function f(v: i32): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const STRUCTURALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: { value: i32 }): void {}",
    subscript: "class C { value: i32 = 0; }\nfunction f(x: C): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const CONDITIONALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f<T>(x: T extends i32 ? i32 : string): void {}",
    subscript:
        "no equivalent: The type system cannot select a parameter type through a conditional type.",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const OPERATORANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: readonly i32[]): void {}",
    subscript: "function f(x: i32[]): void {}",
    why: "The array type is mutable; a function that only reads its parameter can accept an ordinary array.",
    collision: "C24", // row 17,
};
pub(super) const INDEXEDANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; } function f(x: C[\"x\"]): void {}",
    subscript: "class C { x: i32 = 1; } function f(x: i32): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const MAPPEDANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: { [K in \"a\"]: i32 }): void {}",
    subscript: "class C { a: i32 = 0; }\nfunction f(x: C): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const PREDICATEANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "class C {} function f(x: C): x is C { return true; }",
    subscript: "class C {} function f(x: C): boolean { return true; }",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const STRINGLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: \"a\"): void {}",
    subscript: "type A = \"a\" | \"b\"; function f(x: A): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const NUMBERLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: 3): void {}",
    subscript: "function f(x: i32): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const BOOLEANLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: true): void {}",
    subscript: "function f(x: boolean): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const BIGINTLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: 3n): void {}",
    subscript: "function f(x: i64): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const TEMPLATELITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: `a${string}`): void {}",
    subscript: "function f(x: string): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const NEVERANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: never): void {}",
    subscript: "no equivalent: The type system has no uninhabited parameter type.",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const UNKNOWNANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function log(x: unknown): void { print(`${x}`); }",
    subscript: "function log<T>(x: T): void { print(`${x}`); }",
    why: "A generic parameter preserves the argument type; explicit formatting accepts each concrete type without an unknown value.",
    collision: "C24", // row 17,
};
pub(super) const SYMBOLANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: symbol): void {}",
    subscript: "no equivalent: The runtime has no Symbol values.",
    why: "The deterministic lib subset has no Symbol type.",
    collision: "stdlib.md §7",
};
pub(super) const BIGINTANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: bigint): void {}",
    subscript: "function f(x: i64): void {}",
    why: "The language uses i64 and u64 and has no BigInt type.",
    collision: "stdlib.md §7",
};
pub(super) const FUNCTIONTYPERESTPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: (...args: i32[]) => void): void {}",
    subscript: "function f(x: (args: i32[]) => void): void {}",
    why: "A function type has fixed arity; one array parameter can carry the arguments.",
    collision: "C24", // row 17,
};
pub(super) const FUNCTIONTYPEARRAYPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: ([a]: i32[]) => void): void {}",
    subscript: "function f(x: (a: i32[]) => void): void {}",
    why: "A function type parameter uses an identifier; a binding pattern belongs in the function body.",
    collision: "C24", // row 17,
};
pub(super) const FUNCTIONTYPEOBJECTPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "class C { a: i32 = 1; } function f(x: ({a}: C) => void): void {}",
    subscript: "class C { a: i32 = 1; } function f(x: (a: C) => void): void {}",
    why: "A function type parameter uses an identifier; a binding pattern belongs in the function body.",
    collision: "C24", // row 17,
};
pub(super) const TYPEOFOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: string | null): void { if (typeof x === \"string\") { print(x); } }",
    subscript: "function f(x: string | null): void { if (x !== null) { print(x); } }",
    why: "Types are static; the runtime has no typeof operator.",
    collision: "C24", // row 18,
};
pub(super) const VOIDOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { void 1; }",
    subscript: "function f(): void { 1; }",
    why: "The void operator has no lowering; a statement discards its result.",
    collision: "C24", // row 18,
};
pub(super) const UNARYPLUSOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { const n: i32 = 1; +n; }",
    subscript: "function f(): void { const n: i32 = 1; n; }",
    why: "A sized numeric already has a numeric type; unary plus has no lowering.",
    collision: "C24", // row 18,
};
pub(super) const INOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; } function f(c: C): void { \"x\" in c; }",
    subscript: "class C { x: i32 = 1; } function f(c: C): void { true; }",
    why: "A nominal class has fixed declared fields; the runtime has no dynamic property lookup.",
    collision: "C24", // row 18,
};
pub(super) const EXPONENTOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { 2 ** 3; }",
    subscript: "function f(): void { Math.pow(2, 3); }",
    why: "Exponentiation uses Math.pow; the exponent operator has no lowering.",
    collision: "C24", // row 18,
};
pub(super) const COMPOUNDENUMOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { let e: E = E.A; e += 1; }",
    subscript: "enum E { A } function f(): void { let e: i32 = E.A as i32; e += 1; }",
    why: "Enum arithmetic requires an explicit conversion to its integer representation.",
    collision: "C24", // row 19,
};
pub(super) const COMPOUNDSTRINGOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { let s: string = \"a\"; s += 1; }",
    subscript: "function f(): void { let s: string = \"a\"; s += `${1}`; }",
    why: "String concatenation requires explicit formatting of a numeric operand.",
    collision: "C24", // row 19,
};
pub(super) const BINARYENUMOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { E.A + 1; }",
    subscript: "enum E { A } function f(): void { (E.A as i32) + 1; }",
    why: "Enum arithmetic requires an explicit conversion to its integer representation.",
    collision: "C24", // row 19,
};
pub(super) const STRINGRELATIONALOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { \"a\" < \"b\"; }",
    subscript: "function f(): void { \"a\".charCodeAt(0) < \"b\".charCodeAt(0); }",
    why: "No lowering is decided for relational comparisons on two string operands.",
    collision: "C24", // row 31,
};
pub(super) const BINARYSTRINGOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { \"a\" + 1; }",
    subscript: "function f(): void { `a${1}`; }",
    why: "String concatenation requires explicit formatting of a numeric operand.",
    collision: "C24", // row 19,
};
pub(super) const BOOLEANRELATIONALOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { true < false; }",
    subscript: "function f(): void { !true && false; }",
    why: "No lowering is decided for relational comparisons on two boolean operands.",
    collision: "C24", // row 31,
};
pub(super) const IDENTITYASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "class C {} function f(c: C): void { c as C; }",
    subscript: "class C {} function f(c: C): void { c; }",
    why: "A value already has its nominal type; an identity assertion adds no conversion.",
    collision: "C24", // row 19,
};
pub(super) const INTEGERENUMASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(n: i32): E { return n as E; }",
    subscript: "enum E { A } function f(n: i32): E { switch (n) { case 0: return E.A; default: throw new Error(\"invalid E\"); } }",
    why: "An integer assertion has no lowering; a switch checks the integer and returns a declared enum member.",
    collision: "C24", // row 19,
};
pub(super) const NULLABLECLASSASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "class C {} function f(c: C|null): void { c as C; }",
    subscript: "class C {} function f(c: C | null): void { if (c !== null) { c; } }",
    why: "An assertion does not check a nullable value; use a null check before member access.",
    collision: "C24", // row 18,
};
pub(super) const STRINGALIASASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "type A = \"a\" | \"b\"; function f(s: string): void { s as A; }",
    subscript: "type A = \"a\" | \"b\"; function f(s: string): A { if (s === \"a\") return \"a\"; if (s === \"b\") return \"b\"; throw new Error(\"invalid A\"); }",
    why: "A string assertion does not check alias membership; an explicit test selects a declared literal.",
    collision: "C24", // row 19,
};
pub(super) const ANGLEASSERTIONEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function f(n: i32): void { <i32>n; }",
    subscript: "function f(n: i32): void { n; }",
    why: "Angle assertions have no lowering; a value already has its declared type.",
    collision: "C24", // row 18,
};
pub(super) const INSTANTIATIONEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function id<T>(x: T): T { return x; } function f(): void { id<i32>; }",
    subscript: "function id<T>(x: T): T { return x; } function f(): void { const g = (x: i32): i32 => id<i32>(x); }",
    why: "A generic function needs a concrete wrapper before it can supply a function value.",
    collision: "C24", // row 18,
};
pub(super) const COMMAEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function a(): i32 { return 1; } function f(): void { (a(), a()); }",
    subscript: "function a(): i32 { return 1; } function f(): void { a(); a(); }",
    why: "Separate statements preserve the evaluation order without a comma expression.",
    collision: "C24", // row 18,
};
pub(super) const TAGGEDTEMPLATEEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function tag(s: TemplateStringsArray): void {} function f(): void { tag`hello`; }",
    subscript: "function tag(s: string): void {} function f(): void { tag(\"hello\"); }",
    why: "A tagged template has no lowering; an ordinary call accepts an explicit string.",
    collision: "C24", // row 18,
};
pub(super) const CLASSEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { const C = class {}; }",
    subscript: "class C {} function f(): void {}",
    why: "A class declaration belongs at module scope; a class expression has no runtime constructor value.",
    collision: "C24", // row 18,
};
pub(super) const METAPROPERTYEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { import.meta; }",
    subscript: "no equivalent: The runtime has no module metadata object.",
    why: "The runtime has no module metadata object.",
    collision: "C24", // row 18,
};
pub(super) const BOOLEANMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "function f(b: boolean): void { b.toString; }",
    subscript: "function f(b: boolean): void { const value = b; const toString = (): string => `${value}`; }",
    why: "A boolean has no method values; a lambda can format the scalar.",
    collision: "C24", // row 30,
};
pub(super) const FUNCTIONMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "function g(): void {} function f(): void { g.length; }",
    subscript: "function g(): void {} function f(): void { const length: i32 = 0; }",
    why: "A function pointer carries no arity property; its declaration fixes the parameter count.",
    collision: "C24", // row 30,
};
pub(super) const GENERATORMEMBER: DivergenceEntry = DivergenceEntry {
 ts: "function* g(): Generator<i32> { yield 1; } function f(): void { g().next; }", subscript: "function* g(): Generator<i32> { yield 1; } function f(): void { const it = g(); const next = () => it.next(); }", why: "A generator method needs a named receiver; a lambda can make the next call.", collision: "C24", // row 12,
};
pub(super) const ENUMMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { E.A.toString; }",
    subscript: "enum E { A } function f(): void { const toString = (): string => `${E.A}`; }",
    why: "An enum has no method values; a lambda can format the scalar.",
    collision: "C24", // row 30,
};
pub(super) const BOOLEANMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "function f(b: boolean): void { b.toString(); }",
    subscript: "function f(b: boolean): void { `${b}`; }",
    why: "Boolean formatting uses template interpolation; booleans have no methods.",
    collision: "C24", // row 30,
};
pub(super) const FUNCTIONMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "function g(): void {} function f(): void { g.call(null); }",
    subscript: "function g(): void {} function f(): void { g(); }",
    why: "A C function pointer has no call method; call the function directly.",
    collision: "C24", // row 30,
};
pub(super) const ENUMMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { E.A.toString(); }",
    subscript: "enum E { A } function f(): void { `${E.A}`; }",
    why: "Enum formatting uses template interpolation; enums have no methods.",
    collision: "C24", // row 30,
};
pub(super) const ENUMTOINTEGER: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(e: E): i32 { return e; }",
    subscript: "enum E { A } function f(e: E): i32 { return e as i32; }",
    why: "An enum requires an explicit conversion to its integer representation.",
    collision: "C24", // row 19,
};
pub(super) const ARRAYTOFIXEDARRAY: DivergenceEntry = DivergenceEntry {
    ts: "function f(a: i32[]): FixedArray<i32, 2> { return a; }",
    subscript: "function f(a: i32[]): FixedArray<i32, 2> { return [a[0], a[1]]; }",
    why: "Only an array literal constructs fixed storage; an array value has no fixed length.",
    collision: "collisions.md Q3",
};
pub(super) const FUNCTIONPARAMETERIDENTITY: DivergenceEntry = DivergenceEntry {
 ts: "function take(f: (x: i64) => void): void {} function g(x: i32): void {} function f(): void { take(g); }", subscript: "function take(f: (x: i64) => void): void {} function g(x: i32): void {} function f(): void { take((x: i64): void => g(x as i32)); }", why: "Types require identical arguments and parameters, with no implicit conversion or structural substitution. Exception: the nullable handle variance of a function type (compiler.md §235).", collision: "C24", // row 25,
};
pub(super) const IMPORTANNOTATION: DivergenceEntry = DivergenceEntry {
 ts: "// file: main.ts\nexport function f(x: import(\"./other\").C): void {}\n// file: other.ts\nexport class C { x: i32 = 1; }\n", subscript: "// file: main.ts\nimport * as Other from \"./other\";\nfunction f(x: Other.C): void {}\n// file: other.ts\nexport class C { x: i32 = 1; }", why: "The only type qualifier is a namespace import; an annotation names a declared or builtin type.", collision: "C24", // row 17
};

/// C24 row 32 uses a const copy for every index key.
pub(super) const INDEXEDREADNULLCHECK: DivergenceEntry = DivergenceEntry {
    ts: "class A { x: i32 = 1; } function f(xs: (A | null)[]): i32 { if (xs[0] !== null) { return xs[0].x; } return 0; }",
    subscript: "class A { x: i32 = 1; } function f(xs: (A | null)[]): i32 { const v = xs[0]; if (v !== null) { return v.x; } return 0; }",
    why: "An element is a shared location. A const copy reads it once and keeps its narrowing across calls for every key.",
    collision: "C24", // row 32
};
