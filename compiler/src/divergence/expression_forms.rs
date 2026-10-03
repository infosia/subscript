//! Distinct fragments for syntax and operand restrictions.
use super::DivergenceEntry;
pub(super) const LOCALCLASSDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { class C {} }",
    subscript: "no equivalent; declare the item at module scope",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const LOCALFUNCTIONDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { function g(): void {} }",
    subscript: "no equivalent; declare the item at module scope",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const LOCALENUMDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { enum E { A } }",
    subscript: "no equivalent; declare the item at module scope",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const LOCALALIASDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { type A = i32; }",
    subscript: "no equivalent; declare the item at module scope",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const LOCALINTERFACEDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { interface I {} }",
    subscript: "no equivalent; declare the item at module scope",
    why: "No lowering is decided for a local declaration.",
    collision: "C24", // row 5,
};
pub(super) const SOURCEINTERFACEDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "interface I { x: i32; }",
    subscript: "no equivalent; declare a nominal class",
    why: "An interface is structural, and types are nominal.",
    collision: "C24", // row 6,
};
pub(super) const SOURCENAMESPACEDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "namespace N {}",
    subscript: "no equivalent; use the admitted type or operation",
    why: "The module surface excludes namespace declarations.",
    collision: "C18",
};
pub(super) const DOWHILESTATEMENT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { do {} while(false); }",
    subscript: "no equivalent; use a while loop with an explicit first iteration",
    why: "No lowering is decided for do-while statements.",
    collision: "C24", // row 29,
};
pub(super) const FORINSTATEMENT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { for (const k in [1]) {} }",
    subscript: "no equivalent; iterate an admitted collection with for-of",
    why: "No lowering is decided for dynamic property enumeration.",
    collision: "C24", // row 29,
};
pub(super) const LABELEDSTATEMENT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { outer: while (true) { break outer; } }",
    subscript: "no equivalent; use a loop with an explicit exit flag",
    why: "No lowering is decided for labeled statements.",
    collision: "C24", // row 29,
};
pub(super) const DEBUGGERSTATEMENT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { debugger; }",
    subscript: "no equivalent; use the host debugger",
    why: "No lowering is decided for debugger statements.",
    collision: "C24", // row 29,
};
pub(super) const PRIVATEFIELDDECLARATION: DivergenceEntry = DivergenceEntry {
 ts: "class C { #x: i32 = 1; }", subscript: "no equivalent; use TypeScript private or a static generator method", why: "No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.", collision: "C24", // row 11,
};
pub(super) const PRIVATEMETHODDECLARATION: DivergenceEntry = DivergenceEntry {
 ts: "class C { #f(): void {} }", subscript: "no equivalent; use TypeScript private or a static generator method", why: "No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.", collision: "C24", // row 11,
};
pub(super) const STATICBLOCKDECLARATION: DivergenceEntry = DivergenceEntry {
 ts: "class C { static {} }", subscript: "no equivalent; use TypeScript private or a static generator method", why: "No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.", collision: "C24", // row 11,
};
pub(super) const AUTOACCESSORDECLARATION: DivergenceEntry = DivergenceEntry {
 ts: "class C { accessor x: i32 = 1; }", subscript: "no equivalent; use TypeScript private or a static generator method", why: "No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.", collision: "C24", // row 11,
};
pub(super) const TUPLEANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: [i32, i32]): void {}",
    subscript: "no equivalent; use the admitted type or operation",
    why: "The language has no tuple type.",
    collision: "stdlib.md §14",
};
pub(super) const THISANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(x: this): void {} }",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const QUERYANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "const x: i32 = 1; function f(v: typeof x): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const STRUCTURALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: { value: i32 }): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const CONDITIONALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f<T>(x: T extends i32 ? i32 : string): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const OPERATORANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: readonly i32[]): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const INDEXEDANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; } function f(x: C[\"x\"]): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const MAPPEDANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: { [K in \"a\"]: i32 }): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const PREDICATEANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "class C {} function f(x: C): x is C { return true; }",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const STRINGLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: \"a\"): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const NUMBERLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: 3): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const BOOLEANLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: true): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const BIGINTLITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: 3n): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const TEMPLATELITERALANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: `a${string}`): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const NEVERANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: never): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const UNKNOWNANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: unknown): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const SYMBOLANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: symbol): void {}",
    subscript: "no equivalent; use the admitted type or operation",
    why: "The deterministic lib subset has no Symbol type.",
    collision: "stdlib.md §7",
};
pub(super) const BIGINTANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: bigint): void {}",
    subscript: "no equivalent; use the admitted type or operation",
    why: "The language uses i64 and u64 and has no BigInt type.",
    collision: "stdlib.md §7",
};
pub(super) const FUNCTIONTYPERESTPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: (...args: i32[]) => void): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const FUNCTIONTYPEARRAYPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: ([a]: i32[]) => void): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const FUNCTIONTYPEOBJECTPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "class C { a: i32 = 1; } function f(x: ({a}: C) => void): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};
pub(super) const TYPEOFOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { typeof 1; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const VOIDOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { void 1; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const UNARYPLUSOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { const n: i32 = 1; +n; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const INOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; } function f(c: C): void { \"x\" in c; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const EXPONENTOPERATOR: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { 2 ** 3; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const COMPOUNDENUMOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { let e: E = E.A; e += 1; }",
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
    collision: "C24", // row 19,
};
pub(super) const COMPOUNDSTRINGOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { let s: string = \"a\"; s += 1; }",
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
    collision: "C24", // row 19,
};
pub(super) const BINARYENUMOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { E.A + 1; }",
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
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
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
    collision: "C24", // row 19,
};
pub(super) const BOOLEANRELATIONALOPERAND: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { true < false; }",
    subscript: "no equivalent; use an explicit boolean test",
    why: "No lowering is decided for relational comparisons on two boolean operands.",
    collision: "C24", // row 31,
};
pub(super) const IDENTITYASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "class C {} function f(c: C): void { c as C; }",
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
    collision: "C24", // row 19,
};
pub(super) const INTEGERENUMASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { 1 as E; }",
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
    collision: "C24", // row 19,
};
pub(super) const NULLABLECLASSASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "class C {} function f(c: C|null): void { c as C; }",
    subscript: "no equivalent; use the admitted type or operation",
    why: "An assertion does not check a nullable value; use a null check before member access.",
    collision: "C24", // row 18,
};
pub(super) const STRINGALIASASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "type A = \"a\" | \"b\"; function f(s: string): void { s as A; }",
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
    collision: "C24", // row 19,
};
pub(super) const ANGLEASSERTIONEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function f(n: i32): void { <i32>n; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const SATISFIESEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function f(n: i32): void { n satisfies i32; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const INSTANTIATIONEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function id<T>(x: T): T { return x; } function f(): void { id<i32>; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const COMMAEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function a(): i32 { return 1; } function f(): void { (a(), a()); }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const TAGGEDTEMPLATEEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function tag(s: TemplateStringsArray): void {} function f(): void { tag`hello`; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const CLASSEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { const C = class {}; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const METAPROPERTYEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { import.meta; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};
pub(super) const BOOLEANMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "function f(b: boolean): void { b.toString; }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const FUNCTIONMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "function g(): void {} function f(): void { g.length; }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const GENERATORMEMBER: DivergenceEntry = DivergenceEntry {
 ts: "function* g(): Generator<i32> { yield 1; } function f(): void { g().next; }", subscript: "no equivalent; use a lambda that calls the method", why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.", collision: "C24", // row 12,
};
pub(super) const ENUMMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { E.A.toString; }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const LITERALALIASMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "type A = \"a\"|\"b\"; function f(a: A): void { a.length; }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const BOOLEANMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "function f(b: boolean): void { b.toString(); }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const FUNCTIONMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "function g(): void {} function f(): void { g.call(null); }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const ENUMMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(): void { E.A.toString(); }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const LITERALALIASMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "type A = \"a\"|\"b\"; function f(a: A): void { a.toUpperCase(); }",
    subscript: "no equivalent; use an explicit admitted operation",
    why: "The lib surface is a subset, and a C function pointer has no properties.",
    collision: "C24", // row 30,
};
pub(super) const LITERALALIASTOSTRING: DivergenceEntry = DivergenceEntry {
    ts: "type A = \"a\"|\"b\"; function f(a: A): string { return a; }",
    subscript: "no equivalent; use the admitted type or operation",
    why: "A literal alias is a closed nominal set; assignment to plain string is rejected.",
    collision: "collisions.md Q32",
};
pub(super) const ENUMTOINTEGER: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A } function f(e: E): i32 { return e; }",
    subscript: "no equivalent; convert to the integer or use a template",
    why: "The language has no implicit conversion, and an assertion does not check membership.",
    collision: "C24", // row 19,
};
pub(super) const ARRAYTOFIXEDARRAY: DivergenceEntry = DivergenceEntry {
    ts: "function f(a: i32[]): FixedArray<i32, 2> { return a; }",
    subscript: "no equivalent; use the admitted type or operation",
    why: "Only an array literal constructs fixed storage; an array value has no fixed length.",
    collision: "collisions.md Q3",
};
pub(super) const FUNCTIONPARAMETERIDENTITY: DivergenceEntry = DivergenceEntry {
 ts: "function take(f: (x: i64) => void): void {} function g(x: i32): void {} function f(): void { take(g); }", subscript: "no equivalent; use the identical type", why: "Types require identical arguments and parameters; the language has no implicit conversion, structural substitution, or variance.", collision: "C24", // row 25,
};
pub(super) const IMPORTANNOTATION: DivergenceEntry = DivergenceEntry {
 ts: "// file: main.ts\nexport function f(x: import(\"./other\").C): void {}\n// file: other.ts\nexport class C { x: i32 = 1; }\n", subscript: "no equivalent; use a namespace import to qualify the type", why: "The only type qualifier is a namespace import; an annotation names a declared or builtin type.", collision: "C24", // row 17
};
