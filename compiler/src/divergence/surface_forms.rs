//! Rejection fragments for the remaining subset forms.

use super::DivergenceEntry;

pub(super) const CLASSIMPLEMENTSCLAUSE: DivergenceEntry = DivergenceEntry {
    ts: "class A {}\nclass B implements A {}",
    subscript: "class A {}\nclass B {}",
    why: "An implements clause names an interface; types are nominal, so a clause that names a class gives no conversion.",
    collision: "C24", // row 6,
};

pub(super) const INTERFACEMEMBERFORM: DivergenceEntry = DivergenceEntry {
    ts: "interface I { x: i32; }",
    subscript: "interface I { get x(): i32; }",
    why: "An interface declares method and read accessor signatures only, so each member is a direct call in each generic instance.",
    collision: "C24", // row 6,
};

pub(super) const INTERFACEVALUETYPE: DivergenceEntry = DivergenceEntry {
    ts: "interface I { f(): i32; }\nfunction g(x: I): i32 { return x.f(); }",
    subscript: "interface I { f(): i32; }\nfunction g<T extends I>(x: T): i32 { return x.f(); }",
    why: "A value of an interface type needs a dispatch on the run-time class; a constraint gives a direct call in each instance.",
    collision: "C24", // row 6,
};

pub(super) const INTERFACEMEMBERTYPE: DivergenceEntry = DivergenceEntry {
    ts: "interface I { a(x: i32): i32; }\nclass C implements I { a(x: i64): i32 { return 1; } }",
    subscript: "interface I { a(x: i32): i32; }\nclass C implements I { a(x: i32): i32 { return 1; } }",
    why: "Each instance calls the class member directly with the interface types, so the class member has the exact types; sized types do not convert.",
    collision: "C24", // row 6,
};

pub(super) const INTERFACECONSTRAINTUNDECLARED: DivergenceEntry = DivergenceEntry {
    ts: "interface I { f(): i32; }\nclass C { f(): i32 { return 1; } }\nfunction g<T extends I>(x: T): i32 { return x.f(); }\nfunction h(): i32 { return g(new C()); }",
    subscript: "interface I { f(): i32; }\nclass C implements I { f(): i32 { return 1; } }\nfunction g<T extends I>(x: T): i32 { return x.f(); }\nfunction h(): i32 { return g(new C()); }",
    why: "Types are nominal (C1), so a class satisfies an interface constraint only when it declares the interface.",
    collision: "C24", // row 6,
};

pub(super) const SYNCHRONOUSMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(): i32 { return 1; } } export function main(): void { new C().f; }",
    subscript: "class C { f(): i32 { return 1; } }\nexport function main(): void { const c = new C(); const f = (): i32 => c.f(); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERICSYNCHRONOUSMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { f<T>(x: T): T { return x; } } export function main(): void { new C().f; }",
    subscript: "class C { f<T>(x: T): T { return x; } }\nexport function main(): void { const c = new C(); const f = (x: i32): i32 => c.f<i32>(x); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERATORFUNCTIONVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "function* gen(): Generator<i32> { yield 1; } export function main(): void { const f = gen; }",
    subscript: "function* gen(): Generator<i32> { yield 1; }\nexport function main(): void { const f = (): Generator<i32> => gen(); }",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const LIBCONSTRUCTORNAME: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Object(); }",
    subscript: "class ObjectValue {}\nexport function main(): void { new ObjectValue(); }",
    why: "The deterministic lib subset excludes this constructor.",
    collision: "stdlib.md §0",
};

pub(super) const MODULEVARDECLARATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "var x:i32=1;\nexport function main(): void {  }",
    subscript: "let x: i32 = 1;",
    why: "The language has no undefined value for the function-wide var binding.",
    collision: "C24", // row 2,
};

pub(super) const MODULEINITIALIZERMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "let x:i32;\nexport function main(): void {  }",
    subscript: "let x:i32 = 0;\nexport function main(): void {  }",
    why: "A function can read a module variable before any assignment runs; the language has no undefined value.",
    collision: "C24", // row 3,
};

pub(super) const RETURNFLOWCOVERAGE: DivergenceEntry = DivergenceEntry {
    ts: "enum E {a,b} function f(e:E):i32 {switch(e){case E.a:return 1;case E.b:return 2;} print(\"after\");}\nexport function main(): void {  }",
    subscript: "enum E { a, b } function f(e: E): i32 { switch (e) { case E.a: return 1; case E.b: return 2; } }",
    why: "An exhaustive enum switch ends the return flow only in tail position; TypeScript also skips the code after it.",
    collision: "compiler.md §200",
};

pub(super) const LOCALVARDECLARATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { var x:i32=1; }",
    subscript: "export function main(): void { let x: i32 = 1; }",
    why: "The language has no undefined value for the function-wide var binding.",
    collision: "C24", // row 2,
};

pub(super) const LOCALTYPEWITHOUTINITIALIZERFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let x; }",
    subscript: "export function main(): void { let x:i32; }",
    why:
        "A local takes its type from its annotation or its initializer, not from later assignments.",
    collision: "C24", // row 3,
};

pub(super) const GENERATORRETURNVALUE: DivergenceEntry = DivergenceEntry {
    ts: "function *f():Generator<i32> {yield 1;return 2;}\nexport function main(): void {  }",
    subscript: "no equivalent: A finished generator result has no slot for a return value.",
    why: "A finished generator result holds the zero value, so a return value has no slot.",
    collision: "C8",
};

pub(super) const STATEMENTNONBOOLEANCONDITIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n:i32=3; if(n){} }",
    subscript: "export function main(): void { const n:i32=3; if (n !== 0) {} }",
    why: "The language has no implicit conversion in a truth test. A comparison states which value is false: 0, NaN, the empty string, or null.",
    collision: "C24", // row 1,
};

pub(super) const ASYNCFOROFFORM: DivergenceEntry = DivergenceEntry {
    ts: "async function f(xs:i32[]):Promise<void> {for await(const x of xs) {}}\nexport function main(): void {  }",
    subscript: "async function f(xs: i32[]): Promise<void> { for (const x of xs) {} }",
    why: "A synchronous array uses for-of; the runtime has no async iteration protocol.",
    collision: "C24", // row 24,
};

pub(super) const FOROFVARBINDINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { for(var x of [1]) {} }",
    subscript: "export function main(): void { for (const x of [1]) {} }",
    why: "The language has no undefined value for the function-wide var binding.",
    collision: "C24", // row 2,
};

pub(super) const FOROFBINDINGKINDFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let x:i32=0;for(x of [1]) {} }",
    subscript:
        "export function main(): void { let x: i32 = 0; for (const value of [1]) { x = value; } }",
    why: "An iteration head declares a binding; an assignment belongs in the loop body.",
    collision: "C24", // row 24,
};

pub(super) const FOROFSPREADCALL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const xs:i32[]=[1];for(const x of xs.values(...([] as []))) {} }",
    subscript: "export function main(): void { const xs: i32[] = [1]; for (const x of xs.values()) {} }",
    why: "Spread arguments require variadic parameters, which the language does not have.",
    collision: "stdlib.md §14.4",
};

pub(super) const SWITCHDISCRIMINANTKINDFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { switch(true){case true:break;} }",
    subscript: "export function main(): void { if (true) {} }",
    why: "Switch discriminants are integers, enums, strings, or string-literal union aliases; a boolean condition uses if.",
    collision: "C24", // row 24,
};

pub(super) const GENERATORMETHODFORM: DivergenceEntry = DivergenceEntry {
    ts: "class A { x: i32 = 1; *f(): Generator<i32> { yield this.x; } }\nexport function main(): void {  }",
    subscript: "class A { x: i32 = 1; static *f(a: A): Generator<i32> { yield a.x; } }",
    why: "A generator frame has no implicit receiver; a static method takes the receiver as an explicit parameter.",
    collision: "C24", // row 11,
};

pub(super) const IDENTIFIERFIELDNAME: DivergenceEntry = DivergenceEntry {
    ts: "class A { [\"x\"]:i32=1; }\nexport function main(): void {  }",
    subscript: "class A { x: i32 = 1; }",
    why: "Field names must be identifiers.",
    collision: "collisions.md Q28",
};

pub(super) const FIELDTYPEWITHOUTINITIALIZER: DivergenceEntry = DivergenceEntry {
    ts: "class C { x; constructor() { this.x = 1; } }\nexport function main(): void {}",
    subscript: "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {}",
    why: "A field type comes from its annotation or initializer. Constructor assignments do not supply a declaration type.",
    collision: "compiler.md §156",
};

pub(super) const PARAMETERPROPERTYTYPEANNOTATION: DivergenceEntry = DivergenceEntry {
    ts: "class C { constructor(public count = 1 + 1) {} }\nexport function main(): void {}",
    subscript: "class C { constructor(public count: i32 = 1 + 1) {} }\nexport function main(): void {}",
    why: "A field type comes from its declaration, so a parameter property with a default that is not a literal needs an annotation.",
    collision: "compiler.md §198",
};

pub(super) const ENUMSTRINGMEMBERNAMEFORM: DivergenceEntry = DivergenceEntry {
    ts: "enum E { \"a\"=1 }\nexport function main(): void {  }",
    subscript: "enum E { a = 1 }",
    why: "No lowering is decided for string-literal enum member names.",
    collision: "C24", // row 8,
};

pub(super) const GENERICSOURCEALIASFORM: DivergenceEntry = DivergenceEntry {
    ts: "type A<T> = \"a\" | \"b\";\nexport function main(): void {  }",
    subscript: "type A = \"a\" | \"b\";",
    why: "No lowering is decided for generic aliases or repeated literal members.",
    collision: "C24", // row 7,
};

pub(super) const SOURCEALIASRECURSIVEFORM: DivergenceEntry = DivergenceEntry {
    ts: "type L = L[];\nexport function main(): void {  }",
    subscript: "class L { items: L[] = []; }\nexport function main(): void {  }",
    why: "A type of unbounded depth has no finite type in this language.",
    collision: "compiler.md §202",
};

pub(super) const DUPLICATELITERALALIASMEMBERFORM: DivergenceEntry = DivergenceEntry {
    ts: "type A = \"a\" | \"a\";\nexport function main(): void {  }",
    subscript: "type A = \"a\" | \"b\";",
    why: "No lowering is decided for generic aliases or repeated literal members.",
    collision: "C24", // row 7,
};

pub(super) const MIRRORMODULEDECLARATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\n\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare namespace A {}\n",
    subscript: "no equivalent: C header declarations have no namespace identity.",
    why: "A mirror declares C header items, and C has no namespace.",
    collision: "C24", // row 9,
};

pub(super) const NAMESPACEIMPORTTARGETMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "import \"./missing\";\nexport function main(): void {  }",
    subscript:
        "// file: main.ts\nimport \"./missing\";\nexport {};\n// file: missing.ts\nexport {};",
    why: "The program is its files; node cannot load an absent module.",
    collision: "C24", // row 10,
};

pub(super) const NAMEDIMPORTMODULEMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "import \"./missing\"; export {};",
    subscript:
        "// file: main.ts\nimport \"./missing\";\nexport {};\n// file: missing.ts\nexport {};",
    why: "The program is its files; node cannot load an absent module.",
    collision: "C24", // row 10,
};

pub(super) const FUNCTIONRETURNANNOTATIONMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f() {}\nexport function main(): void {  }",
    subscript: "function f(): void {}",
    why: "A function result annotation states the contract of a call, and the host binds its declared type.",
    collision: "C24", // row 4,
};

pub(super) const QUALIFIEDSOURCETYPENAMEFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: globalThis.String): void {}\nexport function main(): void {}",
    subscript: "function f(x: string): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};

pub(super) const ARRAYTYPEARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "type Array<T,U>=T[]; function f(x:Array<i32,string>):void {}\nexport function main(): void {}",
    subscript: "function f(x: Array<i32>): void {}",
    why: "Array in type position is a builtin that no declaration shadows.",
    collision: "stdlib.md §9.0",
};

pub(super) const GENERATORYIELDTYPEMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: Generator): void {}\nexport function main(): void {}",
    subscript: "function f(x: Generator<i32>): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};

pub(super) const LIBTYPENAME: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: ReadonlyArray<i32>): void {}\nexport function main(): void {}",
    subscript: "function f(x: i32[]): void {}",
    why: "The array type is mutable; a function that only reads its parameter can accept an ordinary array.",
    collision: "stdlib.md §0",
};

pub(super) const INTERSECTIONTYPEANNOTATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C {} class D {} function f(x: C & D): void {}\nexport function main(): void {}",
    subscript: "class C {} class D {} class Both { c: C = new C(); d: D = new D(); }\nfunction f(x: Both): void {}",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};

pub(super) const CONSTRUCTORTYPEANNOTATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: new () => Error): void {}\nexport function main(): void {}",
    subscript: "function f(x: () => Error): void {}",
    why:
        "A constructor has no first-class value; a factory function can construct the named class.",
    collision: "C24", // row 14,
};

pub(super) const DISTINCTNOMINALCONTAINERASSIGNMENTFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x:Map<i32,i32>):void { const y:Map<i64,i64>=x; }\nexport function main(): void {}",
    subscript: "function f(x: Map<i32, i32>): void { const y = new Map<i64, i64>(); x.forEach((v: i32, k: i32): void => { y.set(k as i64, v as i64); }); }",
    why: "Types require identical arguments and parameters; the language has no implicit conversion, structural substitution, or variance.",
    collision: "C24", // row 25,
};

pub(super) const LOGICALNOTNONBOOLEANFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n: i32 = 3; const x = !n; }",
    subscript: "export function main(): void { const n: i32 = 3; const x = n === 0; }",
    why: "The language has no implicit conversion in a truth test. A comparison states which value is false: 0, NaN, the empty string, or null.",
    collision: "C24", // row 1,
};

pub(super) const LOGICALNONBOOLEANOPERANDFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const x = 1 && 2; }",
    subscript: "export function main(): void { const a: i32 = 1; const b: i32 = 2; const x = a !== 0 && b !== 0; }",
    why: "The language has no implicit conversion in a truth test. A comparison states which value is false: 0, NaN, the empty string, or null.",
    collision: "C24", // row 1,
};

pub(super) const CONDITIONALNONBOOLEANCONDITIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n: i32 = 3; const x = n ? 1 : 2; }",
    subscript: "export function main(): void { const n: i32 = 3; const x = n !== 0 ? 1 : 2; }",
    why: "The language has no implicit conversion in a truth test. A comparison states which value is false: 0, NaN, the empty string, or null.",
    collision: "C24", // row 1,
};

pub(super) const YIELDDELEGATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "function* gen(): IterableIterator<i32> { yield* [1]; }\nexport function main(): void { gen(); }",
    subscript: "function* gen(): Generator<i32> { for (const value of [1]) { yield value; } }",
    why: "Yield delegation has no lowering; an explicit loop yields each value.",
    collision: "C24", // row 16,
};

pub(super) const ARRAYINDEXNOTINTFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; const i: i64 = 0; a[i]; }",
    subscript:
        "export function main(): void { const a: i32[] = [1]; const i: i64 = 0; a[i as i32]; }",
    why: "An array index has type i32; a different width requires an explicit conversion.",
    collision: "C24", // row 20,
};

pub(super) const FIXEDARRAYINDEXNOTINTFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; const i: i64 = 0; a[i]; }",
    subscript: "export function main(): void { const a: FixedArray<i32, 1> = [1]; const i: i64 = 0; a[i as i32]; }",
    why: "A fixed array index has type i32; a different width requires an explicit conversion.",
    collision: "C24", // row 20,
};

pub(super) const FIXEDARRAYCONSTANTINDEXBOUNDSFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; a[2]; }",
    subscript: "no equivalent: A fixed array of length one has no element at index two.",
    why: "An out-of-range fixed array index always traps.",
    collision: "C24", // row 20,
};

pub(super) const NONINDEXABLERECEIVERFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const s: string = \"x\"; s[0]; }",
    subscript: "export function main(): void { const s: string = \"x\"; s.at(0); }",
    why: "String indexing can produce undefined; at returns a nullable string.",
    collision: "C24", // row 20,
};

pub(super) const ARRAYMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; const f = a.push; }",
    subscript: "export function main(): void { const a: i32[] = [1]; const f = (x: i32): i32 => a.push(x); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const FIXEDARRAYMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; const f = a.map; }",
    subscript: "export function main(): void { const a: FixedArray<i32, 1> = [1]; const f = (cb: (x: i32) => i32): i32[] => a.map(cb); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const MAPMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const m = new Map<i32,i32>(); const f = m.get; }",
    subscript: "export function main(): void { const m = new Map<i32, i32>(); const f = (key: i32): i32 => m.getOr(key, 0); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const SETMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = new Set<i32>().add; }",
    subscript: "export function main(): void { const s = new Set<i32>(); const f = (x: i32): Set<i32> => s.add(x); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const STRINGMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const s: string = \"x\"; const f = s.slice; }",
    subscript: "export function main(): void { const s: string = \"x\"; const f = (start: i32, end: i32): string => s.slice(start, end); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERATORRESULTDONEWRITEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = /x/.test; }",
    subscript: "export function main(): void { const r = /x/; const f = (s: string): boolean => r.test(s); }",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERATORRESULTVALUEWRITEFORM: DivergenceEntry = DivergenceEntry {
    ts: "function* gen(): IterableIterator<i32> { yield 1; }\nexport function main(): void { const s = gen().next(); s.done = true; }",
    subscript: "no equivalent: The generator owns its step result; source code cannot overwrite its done flag.",
    why: "A generator step result exposes read-only done and value fields.",
    collision: "C24", // row 16,
};

pub(super) const LAMBDARETURNFLOWCOVERAGE: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A, B }\nexport function main(): void { const f = (x: E): i32 => { switch (x) { case E.A: return 1; case E.B: return 2; } print(\"after\"); }; }",
    subscript: "enum E { A, B }\nexport function main(): void { const f = (x: E): i32 => { switch (x) { case E.A: return 1; case E.B: return 2; } }; }",
    why: "An exhaustive enum switch ends the return flow only in tail position; TypeScript also skips the code after it.",
    collision: "compiler.md §200",
};

pub(super) const BIGINTLITERAL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n = 1n; }",
    subscript: "export function main(): void { const n: i64 = 1; }",
    why: "The language uses i64 and u64 and has no BigInt.",
    collision: "stdlib.md §7",
};

pub(super) const TEMPLATEINTERPOLATIONKINDFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; }\nexport function main(): void { print(`${new C()}`); }",
    subscript: "class C { x: i32 = 1; }\nexport function main(): void { print(`${new C().x}`); }",
    why: "Interpolation formats scalars, strings, enums, and literal aliases.",
    collision: "C24", // row 22,
};

pub(super) const GENERICFUNCTIONVALUE: DivergenceEntry = DivergenceEntry {
    ts: "function id<T>(x:T):T{return x;} function apply<T>(f:(x:T)=>T,x:T):T{return f(x);} export function main():void { apply(id,3); }",
    subscript: "function id<T>(x: T): T { return x; } function apply(f: (x: i32) => i32, x: i32): i32 { return f(x); } export function main(): void { apply((x: i32): i32 => id<i32>(x), 3); }",
    why: "Generic function values require instantiation outside the admitted inference surface.",
    collision: "compiler.md §149.1",
};

pub(super) const ENUMOBJECTVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A }\nexport function main(): void { const x = E; }",
    subscript: "no equivalent: An enum has no runtime object that holds its members.",
    why:
        "Classes lower to C layouts and enums to integer constants; neither has a run-time object.",
    collision: "C24", // row 14,
};

pub(super) const FOREIGNFUNCTIONVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\n\nexport function main(): void { const x = host; }\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function host(): void;\n",
    subscript: "// file: main.ts\nexport function main(): void { const x = (): void => host(); }\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function host(): void;",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const AMBIENTFUNCTIONVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const x = print; }",
    subscript: "export function main(): void { const x = (s: string): void => print(s); }",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const LIBGLOBALVALUE: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const x = String; }",
    subscript: "export function main(): void { const x = (n: i32): string => `${n}`; }",
    why: "The deterministic lib subset excludes this global name as a value.",
    collision: "stdlib.md §0",
};

pub(super) const EMPTYARRAYINFERENCEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a = []; }",
    subscript: "export function main(): void { const a: i32[] = []; }",
    why: "The language has no any array or evolving array type.",
    collision: "C24", // row 23,
};

pub(super) const DESCRIPTORLITERALSPREADFORM: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { ...{x: 1} }; }",
    subscript: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { x: 1 }; }",
    why: "A descriptor literal fills data members by identifier.",
    collision: "C24", // row 21,
};

pub(super) const DESCRIPTORLITERALQUOTEDKEYFORM: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { \"x\": 1 }; }",
    subscript: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { x: 1 }; }",
    why: "A descriptor literal fills data members by identifier.",
    collision: "C24", // row 21,
};

pub(super) const DESCRIPTORLITERALACCESSORFORM: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { get x(): i32 { return 1; } }; }",
    subscript: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { x: 1 }; }",
    why: "A descriptor literal fills data members by identifier.",
    collision: "C24", // row 21,
};

pub(super) const LIBGLOBALCALL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const s=String(1);const n=Number(\"1\");if(n){} }",
    subscript: "export function main(): void { const s = `${1}`; const n = parseFloat(\"1\"); if (n !== 0) {} }",
    why: "The deterministic lib subset excludes this global call.",
    collision: "stdlib.md §0",
};

pub(super) const GENERATORYIELDTYPENOTKNOWNFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function before(): void { gen(); } function* gen() { yield 1; }\nexport function main(): void { gen(); }",
    subscript: "export function before(): void { gen(); } function* gen(): Generator<i32> { yield 1; }",
    why: "The yield type comes from the generator body.",
    collision: "C24", // row 27,
};

pub(super) const COROUTINERETURNORTHROWCALLFORM: DivergenceEntry = DivergenceEntry {
    ts: "function* gen() { yield 1; }\nexport function main(): void { gen().return(undefined); }",
    subscript:
        "no equivalent: The generator API has no return or throw operation to force completion.",
    why: "A generator exposes next only; it has no return or throw operation.",
    collision: "C24", // row 16,
};

pub(super) const CONSTRUCTORNOTNAMEDCLASSFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C {}\nexport function main(): void { new (true ? C : C)(); }",
    subscript: "class C {}\nexport function main(): void { new C(); }",
    why:
        "Classes lower to C layouts and enums to integer constants; neither has a run-time object.",
    collision: "C24", // row 14,
};

pub(super) const STATICMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { static *g(): Generator<i32> { yield 1; } }\nexport function main(): void { const f = C.g; }",
    subscript: "class C { static *g(): Generator<i32> { yield 1; } }\nexport function main(): void { const f = (): Generator<i32> => C.g(); }",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const METHODASSIGNMENT: DivergenceEntry = DivergenceEntry {
    ts: "class C { v(): i32 { return 1; } }\nfunction two(): i32 { return 2; }\nfunction f(c: C): void { c.v = two; }",
    subscript: "class C { v: () => i32 = (): i32 => 1; }\nfunction two(): i32 { return 2; }\nfunction f(c: C): void { c.v = two; }",
    why: "A method is a C function with no slot in the instance, so an assignment cannot replace it; a function field has a slot.",
    collision: "C24", // row 12,
};

pub(super) const ENUMOBJECTMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A }\nexport function main(): void { E.toString; }",
    subscript: "no equivalent: An enum has no runtime object or object method value.",
    why:
        "Classes lower to C layouts and enums to integer constants; neither has a run-time object.",
    collision: "C24", // row 14,
};

pub(super) const THISINVALUETYPEARROW: DivergenceEntry = DivergenceEntry {
    ts: "@ValueType class C { x: i32 = 1; f(): i32 { const fn = (): i32 => this.x; return fn(); } }\nexport function main(): void { new C().f(); }",
    subscript: "class C { x: i32 = 1; f(): i32 { const fn = (): i32 => this.x; return fn(); } }\nexport function main(): void { new C().f(); }",
    why: "A ValueType receiver capture is a copy (C2); a write through it does not reach the receiver.",
    collision: "C24", // row 15,
};

pub(super) const WRITE_THROUGH_VALUE_TEMPORARY: DivergenceEntry = DivergenceEntry {
    ts: "function pair(): FixedArray<i32, 2> { return [1, 2]; }\nexport function main(): void { pair()[0] = 9; }",
    subscript: "function pair(): FixedArray<i32, 2> { return [1, 2]; }\nexport function main(): void { const kept = pair(); kept[0] = 9; print(`${kept[0]}`); }",
    why: "A FixedArray or value class result is a copy (C2), so a write through it has no reader.",
    collision: "C2",
};

pub(super) const ELEMENT_METHOD_MOVES_ARRAY: DivergenceEntry = DivergenceEntry {
    ts: "@ValueType class V { a: i32 = 1; grow(all: V[]): void { all.push(new V()); this.a = 2; } }\nexport function main(): void { const all: V[] = [new V()]; all[0].grow(all); }",
    subscript: "@ValueType class V { a: i32 = 1; grow(all: V[]): void { all.push(new V()); this.a = 2; } }\nexport function main(): void { const all: V[] = [new V()]; const t: V = all[0]; t.grow(all); all[0] = t; print(`${all[0].a}`); }",
    why: "`this` on a dynamic array element is an address into the array storage (C2), so a move of the array leaves `this` on old storage.",
    collision: "C2",
};

pub(super) const NONNULLASSERTIONEXPRESSIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; }\nexport function main(): void { const c: C = new C(); c!.x; }",
    subscript:
        "class C { x: i32 = 1; }\nexport function main(): void { const c: C = new C(); c.x; }",
    why: "A non-null value needs no assertion; a nullable value requires a null check.",
    collision: "C24", // row 18,
};

pub(super) const FUNCTIONEXPRESSIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = function*() { yield 1; }; }",
    subscript: "function* f(): Generator<i32> { yield 1; }\nexport function main(): void { f(); }",
    why: "A generator expression has no lowering; a named generator declaration supplies the direct call target.",
    collision: "C24", // row 15,
};

pub(super) const POWERASSIGNMENTFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let a: i32 = 2; a **= 2; }",
    subscript: "export function main(): void { let a: i32 = 2; a = Math.pow(a as f64, 2) as i32; }",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};

pub(super) const NONPLACEASSIGNMENTTARGETFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let x: i32 = 1; (x as i32) = 2; }",
    subscript: "export function main(): void { let x: i32 = 1; x = 2; }",
    why: "An assignment requires a storage place; an assertion expression does not supply one.",
    collision: "C24", // row 18,
};

pub(super) const COLLECTIONCALLBACKTYPEMISMATCHFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; a.map((x: i64): i64 => x); }",
    subscript: "export function main(): void { const a: i32[] = [1]; a.map((x: i32): i64 => x as i64); }",
    why: "Types require identical arguments and parameters; the language has no implicit conversion, structural substitution, or variance.",
    collision: "C24", // row 25,
};

pub(super) const TYPEPARAMETERDEFAULT: DivergenceEntry = DivergenceEntry {
    ts: "function f<T = i32>(x: T): void {}",
    subscript: "function f<T>(x: T): void {}\nf<i32>(1);",
    why: "No lowering is decided for type-parameter defaults.",
    collision: "C24", // row 28,
};
pub(super) const GENERATORNEXTARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
    ts: "function* g(): Generator<i32> { yield 1; } function f(): void { g().next(1); }",
    subscript: "function* g(): Generator<i32> { yield 1; } function f(): void { g().next(); }",
    why: "The coroutine next call drives the frame without an input value.",
    collision: "C8",
};
pub(super) const ARRAYCONCATARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { const xs: i32[] = []; xs.concat(xs,xs); }",
    subscript: "function f(): void { const xs: i32[] = []; xs.concat(xs).concat(xs); }",
    why: "Array concat takes exactly one array argument.",
    collision: "stdlib.md §9",
};

pub(super) const STRINGCONCATARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { \"a\".concat(\"b\", \"c\"); }",
    subscript: "function f(): void { \"a\".concat(\"b\").concat(\"c\"); }",
    why: "String concat takes exactly one string argument.",
    collision: "stdlib.md §8",
};

/// C24 row 18 excludes const assertions.
pub(super) const CONSTASSERTIONEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "export {}; const x = 1 as const;",
    subscript: "export {}; const x: i32 = 1;",
    why: "No lowering is decided for const assertions that preserve literal types.",
    collision: "C24", // row 18
};

pub(super) const BLOCKLAMBDARETURNANNOTATIONMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = () => { return 1; }; }",
    subscript: "export function main(): void { const f = (): i32 => { return 1; }; }",
    why: "A block result has no inference that agrees with tsc for every program.",
    collision: "C24", // row 4,
};

pub(super) const FUNCTIONVALUEPARAMETERANNOTATIONNEEDED: DivergenceEntry = DivergenceEntry {
    ts: "function f(n: i32, k = f): void {}",
    subscript: "function f(n: i32, k = 0): void {} export function main(): void {}",
    why: "A function value needs decided parameter types; use a default without the recursive signature dependency.",
    collision: "C24", // row 4
};

pub(super) const GENERICCALLBACKPARAMETERANNOTATIONNEEDED: DivergenceEntry = DivergenceEntry {
    ts: "function each<T>(xs: T[], f: (x: T) => void): void {} each([1, 2], (x) => {}); export function main(): void {}",
    subscript: "function each<T>(xs: T[], f: (x: T) => void): void {} each([1, 2], (x: i32): void => {}); export function main(): void {}",
    why: "A type parameter that no non-literal argument fixes gives a callback no parameter types (compiler.md §197 rule 3, §249 rule 2a). Annotate each callback parameter.",
    collision: "C24", // row 4
};

pub(super) const GENERICCLASSDEFAULTPARAMETERANNOTATIONNEEDED: DivergenceEntry = DivergenceEntry {
    ts: "class Cell<T> { v: T; constructor(v: T) { this.v = v; } pair(other = this.v): string { return `${other}`; } }",
    subscript: "class Cell<T> { v: T; constructor(v: T) { this.v = v; } pair(other: T = this.v): string { return `${other}`; } }",
    why: "A generic-class default needs an annotation to preserve the uninstantiated class parameter type that TypeScript uses.",
    collision: "C24", // row 4
};

/// A receiver capture in a parameter default.
pub(super) const THISINPARAMETERDEFAULTARROW: DivergenceEntry = DivergenceEntry {
    ts: "class C { n:i32=1; m(f:()=>i32=():i32=>this.n):i32 { return f(); } } export function main():void {}",
    subscript: "class C { n:i32=1; m():i32 { const f=():i32=>this.n; return f(); } } export function main():void {}",
    why: "A parameter default executes outside the method frame that holds the receiver; its lambda cannot capture that receiver.",
    collision: "C5", // compiler.md §157 rule 4
};

/// C24 row 36 requires immutable async captures.
pub(super) const ASYNC_ARROW_CAPTURE: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { let n: i32 = 1; const job = async (): Promise<i32> => n; }",
    subscript: "function f(): void { let n: i32 = 1; const copy = n; const job = async (): Promise<i32> => copy; }\nexport function main(): void {}",
    why: "An async arrow owns immutable captures. A mutable binding needs an explicit const copy or a class field.",
    collision: "C24",
};
