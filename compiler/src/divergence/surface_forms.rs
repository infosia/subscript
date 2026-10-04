//! Rejection fragments for the remaining subset forms.

use super::DivergenceEntry;

pub(super) const SYNCHRONOUSMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(): i32 { return 1; } } export function main(): void { new C().f; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERICSYNCHRONOUSMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { f<T>(x: T): T { return x; } } export function main(): void { new C().f; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERATORFUNCTIONVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "function* gen(): Generator<i32> { yield 1; } export function main(): void { const f = gen; }",
    subscript: "no equivalent; use a lambda that calls the target",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const NULLABLECALLNONNULLFLOW: DivergenceEntry = DivergenceEntry {
    ts: "function g(x: i32): i32 { return x; } function f(): void { const cb: ((x: i32)=>i32)|null = g; cb(1); }",
    subscript: "no equivalent; write an explicit null check",
    why: "No nullable narrowing from initializer, assignment, or terminal-call facts is decided.",
    collision: "C24", // row 26,
};

pub(super) const NULLABLEMEMBERNONNULLFLOW: DivergenceEntry = DivergenceEntry {
    ts: "class C{x:i32=1;} export function main():void {const c:C|null=new C(); print(`${c.x}`);}",
    subscript: "no equivalent; write an explicit null check",
    why: "No nullable narrowing from initializer, assignment, or terminal-call facts is decided.",
    collision: "C24", // row 26,
};

pub(super) const LIBCONSTRUCTORNAME: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Object(); }",
    subscript: "no equivalent; use an admitted constructor",
    why: "The deterministic lib subset excludes this constructor.",
    collision: "stdlib.md §0",
};

pub(super) const MODULEVARDECLARATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "var x:i32=1;\nexport function main(): void {  }",
    subscript: "no equivalent; use let or const",
    why: "The language has no undefined value for the function-wide var binding.",
    collision: "C24", // row 2,
};

pub(super) const MODULEINITIALIZERMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "let x:i32;\nexport function main(): void {  }",
    subscript: "no equivalent; supply an initializer",
    why: "A binding needs an initializer because the language has no undefined value.",
    collision: "C24", // row 3,
};

pub(super) const RETURNFLOWCOVERAGE: DivergenceEntry = DivergenceEntry {
    ts: "enum E {a,b} function f(e:E):i32 {switch(e){case E.a:return 1;case E.b:return 2;}}\nexport function main(): void {  }",
    subscript: "no equivalent; add a default branch",
    why: "Return-flow analysis proves switch coverage with a default or a closed literal alias only.",
    collision: "compiler.md §42.1",
};

pub(super) const LOCALVARDECLARATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { var x:i32=1; }",
    subscript: "no equivalent; use let or const",
    why: "The language has no undefined value for the function-wide var binding.",
    collision: "C24", // row 2,
};

pub(super) const LOCALINITIALIZERMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let x:i32; }",
    subscript: "no equivalent; supply an initializer",
    why: "A binding needs an initializer because the language has no undefined value.",
    collision: "C24", // row 3,
};

pub(super) const GENERATORRETURNVALUE: DivergenceEntry = DivergenceEntry {
    ts: "function *f():Generator<i32> {yield 1;return 2;}\nexport function main(): void {  }",
    subscript: "no equivalent; return without a value",
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
    subscript: "no equivalent; use if, await in the body, or a const head",
    why: "Switch dispatch uses constant scalar cases; for await reads async iteration; no lowering is decided for assigning iteration heads.",
    collision: "C24", // row 24,
};

pub(super) const FOROFVARBINDINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { for(var x of [1]) {} }",
    subscript: "no equivalent; use let or const",
    why: "The language has no undefined value for the function-wide var binding.",
    collision: "C24", // row 2,
};

pub(super) const FOROFBINDINGKINDFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let x:i32=0;for(x of [1]) {} }",
    subscript: "no equivalent; use if, await in the body, or a const head",
    why: "Switch dispatch uses constant scalar cases; for await reads async iteration; no lowering is decided for assigning iteration heads.",
    collision: "C24", // row 24,
};

pub(super) const FOROFSPREADCALL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const xs:i32[]=[1];for(const x of xs.values(...([] as []))) {} }",
    subscript: "no equivalent; pass arguments directly",
    why: "Spread arguments require variadic parameters, which the language does not have.",
    collision: "stdlib.md §14.4",
};

pub(super) const SWITCHDISCRIMINANTKINDFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { switch(true){case true:break;} }",
    subscript: "no equivalent; use if, await in the body, or a const head",
    why: "Switch dispatch uses constant scalar cases; for await reads async iteration; no lowering is decided for assigning iteration heads.",
    collision: "C24", // row 24,
};

pub(super) const GENERATORMETHODFORM: DivergenceEntry = DivergenceEntry {
    ts: "class A { *f():Generator<i32> {yield 1;} }\nexport function main(): void {  }",
    subscript: "no equivalent; use TypeScript private or a static generator method",
    why: "No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.",
    collision: "C24", // row 11,
};

pub(super) const IDENTIFIERFIELDNAME: DivergenceEntry = DivergenceEntry {
    ts: "class A { [\"x\"]:i32=1; }\nexport function main(): void {  }",
    subscript: "no equivalent; write an identifier field name",
    why: "Field names must be identifiers.",
    collision: "collisions.md Q28",
};

pub(super) const FIELDTYPEWITHOUTINITIALIZER: DivergenceEntry = DivergenceEntry {
    ts: "class C { x; constructor() { this.x = 1; } }\nexport function main(): void {}",
    subscript: "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {}",
    why: "A field type comes from its annotation or initializer. Constructor assignments do not supply a declaration type.",
    collision: "compiler.md §156",
};

pub(super) const CONSTRUCTORPARAMETERPROPERTYFORM: DivergenceEntry = DivergenceEntry {
    ts: "class A { constructor(public x:i32) {} }\nexport function main(): void {  }",
    subscript: "no equivalent; write an identifier member or a field and constructor assignment",
    why: "No lowering is decided for string-literal enum member names or constructor parameter properties.",
    collision: "C24", // row 8,
};

pub(super) const ENUMSTRINGMEMBERNAMEFORM: DivergenceEntry = DivergenceEntry {
    ts: "enum E { \"a\"=1 }\nexport function main(): void {  }",
    subscript: "no equivalent; write an identifier member or a field and constructor assignment",
    why: "No lowering is decided for string-literal enum member names or constructor parameter properties.",
    collision: "C24", // row 8,
};

pub(super) const GENERICSOURCEALIASFORM: DivergenceEntry = DivergenceEntry {
    ts: "type A<T> = \"a\" | \"b\";\nexport function main(): void {  }",
    subscript: "no equivalent; use the aliased type directly",
    why: "No lowering is decided for transparent or generic aliases, or repeated literal members.",
    collision: "C24", // row 7,
};

pub(super) const SOURCEALIASNOTLITERALUNIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "type A = i32;\nexport function main(): void {  }",
    subscript: "no equivalent; use the aliased type directly",
    why: "No lowering is decided for transparent or generic aliases, or repeated literal members.",
    collision: "C24", // row 7,
};

pub(super) const DUPLICATELITERALALIASMEMBERFORM: DivergenceEntry = DivergenceEntry {
    ts: "type A = \"a\" | \"a\";\nexport function main(): void {  }",
    subscript: "no equivalent; use the aliased type directly",
    why: "No lowering is decided for transparent or generic aliases, or repeated literal members.",
    collision: "C24", // row 7,
};

pub(super) const MIRRORMODULEDECLARATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\n\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare namespace A {}\n",
    subscript: "no equivalent; declare C header items directly",
    why: "A mirror declares C header items, and C has no namespace.",
    collision: "C24", // row 9,
};

pub(super) const NAMESPACEIMPORTTARGETMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "import \"./missing\";\nexport function main(): void {  }",
    subscript: "no equivalent; supply the imported module file",
    why: "The program is its files; node cannot load an absent module.",
    collision: "C24", // row 10,
};

pub(super) const NAMEDIMPORTMODULEMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "import \"./missing\"; export {};",
    subscript: "no equivalent; supply the imported module file",
    why: "The program is its files; node cannot load an absent module.",
    collision: "C24", // row 10,
};

pub(super) const FUNCTIONRETURNANNOTATIONMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f() {}\nexport function main(): void {  }",
    subscript: "no equivalent; supply a type annotation",
    why: "A function result annotation states the contract of a call, and the host binds its declared type.",
    collision: "C24", // row 4,
};

pub(super) const QUALIFIEDSOURCETYPENAMEFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: globalThis.String): void {}\nexport function main(): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};

pub(super) const ARRAYTYPEARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "type Array<T,U>=T[]; function f(x:Array<i32,string>):void {}\nexport function main(): void {}",
    subscript: "no equivalent; supply one element type argument",
    why: "Array in type position is a builtin that no declaration shadows.",
    collision: "stdlib.md §9.0",
};

pub(super) const GENERATORYIELDTYPEMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: Generator): void {}\nexport function main(): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};

pub(super) const LIBTYPENAME: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: ReadonlyArray<i32>): void {}\nexport function main(): void {}",
    subscript: "no equivalent; name an admitted type",
    why: "The deterministic lib subset excludes this global type name.",
    collision: "stdlib.md §0",
};

pub(super) const INTERSECTIONTYPEANNOTATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C {} class D {} function f(x: C & D): void {}\nexport function main(): void {}",
    subscript: "no equivalent; name the declared type",
    why: "An annotation names a declared or builtin type; types are nominal.",
    collision: "C24", // row 17,
};

pub(super) const CONSTRUCTORTYPEANNOTATIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: new () => Error): void {}\nexport function main(): void {}",
    subscript: "no equivalent; use a directly named class or enum member",
    why:
        "Classes lower to C layouts and enums to integer constants; neither has a run-time object.",
    collision: "C24", // row 14,
};

pub(super) const DISTINCTNOMINALCONTAINERASSIGNMENTFORM: DivergenceEntry = DivergenceEntry {
    ts: "function f(x:Map<i32,i32>):void { const y:Map<i64,i64>=x; }\nexport function main(): void {}",
    subscript: "no equivalent; use the identical type",
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
    subscript: "export function main(): void { const x = 1 !== 0 && 2 !== 0; }",
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
    subscript: "no equivalent; iterate with for-of and yield each value",
    why: "A generator supplies one yield type through next, with a zero finished value; no lowering is decided for yield delegation.",
    collision: "C24", // row 16,
};

pub(super) const PRIVATEMEMBERREADFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { #x: i32 = 1; f(): i32 { return this.#x; } }\nexport function main(): void { new C().f(); }",
    subscript: "no equivalent; use TypeScript private or a static generator method",
    why: "No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.",
    collision: "C24", // row 11,
};

pub(super) const ARRAYINDEXNOTINTFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; const i: i64 = 0; a[i]; }",
    subscript: "no equivalent; convert the index to i32 or call string.at",
    why: "Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.",
    collision: "C24", // row 20,
};

pub(super) const FIXEDARRAYINDEXNOTINTFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; const i: i64 = 0; a[i]; }",
    subscript: "no equivalent; convert the index to i32 or call string.at",
    why: "Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.",
    collision: "C24", // row 20,
};

pub(super) const FIXEDARRAYCONSTANTINDEXBOUNDSFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; a[2]; }",
    subscript: "no equivalent; convert the index to i32 or call string.at",
    why: "Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.",
    collision: "C24", // row 20,
};

pub(super) const NONINDEXABLERECEIVERFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const s: string = \"x\"; s[0]; }",
    subscript: "no equivalent; convert the index to i32 or call string.at",
    why: "Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.",
    collision: "C24", // row 20,
};

pub(super) const ARRAYMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; const f = a.push; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const FIXEDARRAYMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; const f = a.map; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const MAPMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const m = new Map<i32,i32>(); const f = m.get; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const SETMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = new Set<i32>().add; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const STRINGMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const s: string = \"x\"; const f = s.slice; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERATORRESULTDONEWRITEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = /x/.test; }",
    subscript: "no equivalent; use a lambda that calls the method",
    why: "A method value loses its receiver; a bound value captures it, and capturing values cannot escape.",
    collision: "C24", // row 12,
};

pub(super) const GENERATORRESULTVALUEWRITEFORM: DivergenceEntry = DivergenceEntry {
    ts: "function* gen(): IterableIterator<i32> { yield 1; }\nexport function main(): void { const s = gen().next(); s.done = true; }",
    subscript: "no equivalent; iterate with for-of and yield each value",
    why: "A generator supplies one yield type through next, with a zero finished value; no lowering is decided for yield delegation.",
    collision: "C24", // row 16,
};

pub(super) const LAMBDARETURNFLOWCOVERAGE: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A, B }\nexport function main(): void { const f = (x: E): i32 => { switch (x) { case E.A: return 1; case E.B: return 2; } }; }",
    subscript: "no equivalent; add a default branch",
    why: "Return-flow analysis proves switch coverage with a default or a closed literal alias only.",
    collision: "compiler.md §42.1",
};

pub(super) const BIGINTLITERAL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n = 1n; }",
    subscript: "no equivalent; use a sized integer literal",
    why: "The language uses i64 and u64 and has no BigInt.",
    collision: "stdlib.md §7",
};

pub(super) const TEMPLATEINTERPOLATIONKINDFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; }\nexport function main(): void { print(`${new C()}`); }",
    subscript: "no equivalent; join the array or call a formatting method",
    why: "Interpolation formats scalars, strings, enums, and literal aliases.",
    collision: "C24", // row 22,
};

pub(super) const GENERICFUNCTIONVALUE: DivergenceEntry = DivergenceEntry {
    ts: "function id<T>(x:T):T{return x;} function apply<T>(f:(x:T)=>T,x:T):T{return f(x);} export function main():void { apply(id,3); }",
    subscript: "no equivalent; call the function directly or use a lambda",
    why: "Generic function values require instantiation outside the admitted inference surface.",
    collision: "compiler.md §149.1",
};

pub(super) const ENUMOBJECTVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A }\nexport function main(): void { const x = E; }",
    subscript: "no equivalent; use a directly named class or enum member",
    why:
        "Classes lower to C layouts and enums to integer constants; neither has a run-time object.",
    collision: "C24", // row 14,
};

pub(super) const FOREIGNFUNCTIONVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\n\nexport function main(): void { const x = host; }\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function host(): void;\n",
    subscript: "no equivalent; use a lambda that calls the target",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const AMBIENTFUNCTIONVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const x = print; }",
    subscript: "no equivalent; use a lambda that calls the target",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const LIBGLOBALVALUE: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const x = String; }",
    subscript: "no equivalent; use an admitted value",
    why: "The deterministic lib subset excludes this global name as a value.",
    collision: "stdlib.md §0",
};

pub(super) const EMPTYARRAYINFERENCEFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a = []; }",
    subscript: "no equivalent; annotate the array element type",
    why: "The language has no any array or evolving array type.",
    collision: "C24", // row 23,
};

pub(super) const DESCRIPTORLITERALSPREADFORM: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { ...{x: 1} }; }",
    subscript: "no equivalent; write identifier data members",
    why: "A descriptor literal fills data members by identifier.",
    collision: "C24", // row 21,
};

pub(super) const DESCRIPTORLITERALQUOTEDKEYFORM: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { \"x\": 1 }; }",
    subscript: "no equivalent; write identifier data members",
    why: "A descriptor literal fills data members by identifier.",
    collision: "C24", // row 21,
};

pub(super) const DESCRIPTORLITERALACCESSORFORM: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { x!: i32; }\nexport function main(): void { const a: D = { get x(): i32 { return 1; } }; }",
    subscript: "no equivalent; write identifier data members",
    why: "A descriptor literal fills data members by identifier.",
    collision: "C24", // row 21,
};

pub(super) const LIBGLOBALCALL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const s=String(1);const n=Number(\"1\");if(n){} }",
    subscript: "no equivalent; use an admitted conversion or call",
    why: "The deterministic lib subset excludes this global call.",
    collision: "stdlib.md §0",
};

pub(super) const GENERATORYIELDTYPENOTKNOWNFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function before(): void { gen(); } function* gen() { yield 1; }\nexport function main(): void { gen(); }",
    subscript: "no equivalent; annotate the Generator result type",
    why: "The yield type comes from the generator body.",
    collision: "C24", // row 27,
};

pub(super) const COROUTINERETURNORTHROWCALLFORM: DivergenceEntry = DivergenceEntry {
    ts: "function* gen() { yield 1; }\nexport function main(): void { gen().return(undefined); }",
    subscript: "no equivalent; iterate with for-of and yield each value",
    why: "A generator supplies one yield type through next, with a zero finished value; no lowering is decided for yield delegation.",
    collision: "C24", // row 16,
};

pub(super) const CONSTRUCTORNOTNAMEDCLASSFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C {}\nexport function main(): void { new (true ? C : C)(); }",
    subscript: "no equivalent; use a directly named class or enum member",
    why:
        "Classes lower to C layouts and enums to integer constants; neither has a run-time object.",
    collision: "C24", // row 14,
};

pub(super) const STATICMETHODVALUEFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { static f(): void {} }\nexport function main(): void { const f = C.f; }",
    subscript: "no equivalent; use a lambda that calls the target",
    why: "No first-class value is decided for a direct call target.",
    collision: "C24", // row 13,
};

pub(super) const ENUMOBJECTMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "enum E { A }\nexport function main(): void { E.toString; }",
    subscript: "no equivalent; use a directly named class or enum member",
    why:
        "Classes lower to C layouts and enums to integer constants; neither has a run-time object.",
    collision: "C24", // row 14,
};

pub(super) const THISINMETHODARROW: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; f(): i32 { const fn = (): i32 => this.x; return fn(); } }\nexport function main(): void { new C().f(); }",
    subscript: "no equivalent; capture a const self local",
    why: "A lambda captures const locals only; it cannot capture this.",
    collision: "C24", // row 15,
};

pub(super) const NONNULLASSERTIONEXPRESSIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { x: i32 = 1; }\nexport function main(): void { const c: C = new C(); c!.x; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};

pub(super) const FUNCTIONEXPRESSIONFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = function*() { yield 1; }; }",
    subscript: "no equivalent; capture a const self local",
    why: "A lambda captures const locals only; it cannot capture this.",
    collision: "C24", // row 15,
};

pub(super) const LOGICALORPOWERASSIGNMENTFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let a: boolean = true; a &&= false; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};

pub(super) const NONPLACEASSIGNMENTTARGETFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { let x: i32 = 1; (x as i32) = 2; }",
    subscript: "no equivalent; use Math.pow, an explicit comparison, or a null check",
    why: "No lowering is decided for this operator or expression form.",
    collision: "C24", // row 18,
};

pub(super) const PRIVATEMEMBERASSIGNMENTFORM: DivergenceEntry = DivergenceEntry {
    ts: "class C { #x: i32 = 1; f(): void { this.#x = 2; } }\nexport function main(): void { new C().f(); }",
    subscript: "no equivalent; use TypeScript private or a static generator method",
    why: "No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.",
    collision: "C24", // row 11,
};

pub(super) const COLLECTIONCALLBACKTYPEMISMATCHFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; a.map((x: i64): i64 => x); }",
    subscript: "no equivalent; use the identical type",
    why: "Types require identical arguments and parameters; the language has no implicit conversion, structural substitution, or variance.",
    collision: "C24", // row 25,
};

pub(super) const TYPEPARAMETERDEFAULT: DivergenceEntry = DivergenceEntry {
    ts: "function f<T = i32>(x: T): void {}",
    subscript: "no equivalent; pass an explicit type argument",
    why: "No lowering is decided for type-parameter defaults.",
    collision: "C24", // row 28,
};
pub(super) const ARRAYPUSHARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { const xs: i32[] = []; xs.push(1,2); }",
    subscript: "no equivalent; call push once per element",
    why: "The array push surface appends exactly one element.",
    collision: "collisions.md Q27",
};
pub(super) const GENERATORNEXTARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
    ts: "function* g(): Generator<i32> { yield 1; } function f(): void { g().next(1); }",
    subscript: "no equivalent; call next without arguments",
    why: "The coroutine next call drives the frame without an input value.",
    collision: "C8",
};
pub(super) const ARRAYCONCATARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { const xs: i32[] = []; xs.concat(xs,xs); }",
    subscript: "no equivalent; call concat once per array",
    why: "Array concat takes exactly one array argument.",
    collision: "stdlib.md §9",
};

pub(super) const STRINGCONCATARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
    ts: "function f(): void { \"a\".concat(\"b\", \"c\"); }",
    subscript: "no equivalent; call concat once per string",
    why: "String concat takes exactly one string argument.",
    collision: "stdlib.md §8",
};

pub(super) const NULLABLENOMINALASSIGNMENTNONNULLFLOW: DivergenceEntry = DivergenceEntry {
    ts: "class C {} function use(c: C): void {} function f(): void { const c: C|null = new C(); use(c); }",
    subscript: "no equivalent; write an explicit null check",
    why: "No nullable narrowing from initializer, assignment, or terminal-call facts is decided.",
    collision: "C24", // row 26
};

/// C24 row 18 excludes const assertions.
pub(super) const CONSTASSERTIONEXPRESSION: DivergenceEntry = DivergenceEntry {
    ts: "export {}; const x = 1 as const;",
    subscript: "no equivalent; use an explicit type annotation",
    why: "No lowering is decided for const assertions that preserve literal types.",
    collision: "C24", // row 18
};

pub(super) const BLOCKLAMBDARETURNANNOTATIONMISSINGFORM: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = () => { return 1; }; }",
    subscript: "no equivalent; supply a type annotation",
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
    why: "Generic argument inference checks callback parameters before the callee supplies their context; annotate each callback parameter.",
    collision: "C24", // row 4
};

pub(super) const GENERICCLASSDEFAULTPARAMETERANNOTATIONNEEDED: DivergenceEntry = DivergenceEntry {
    ts: "class Cell<T> { v: T; constructor(v: T) { this.v = v; } pair(other = this.v): string { return `${other}`; } }",
    subscript: "no equivalent; annotate the parameter",
    why: "A generic-class default needs an annotation to preserve the uninstantiated class parameter type that TypeScript uses.",
    collision: "C24", // row 4
};
