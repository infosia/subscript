//! Fragment records for the group a rejection guards.

use super::DivergenceEntry;

pub(super) const UNARY_NUMERIC_COERCION: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const x = -\"2\"; }",
    subscript: "export function main(): void { const x = -parseFloat(\"2\"); }",
    why: "Unary negation takes a sized numeric operand; it does not coerce a string to a number.",
    collision: "C3",
};

pub(super) const BITWISE_INTEGER_OPERAND: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n: f64 = 1; const x = ~n; }",
    subscript: "export function main(): void { const n: f64 = 1; const x = ~(n as i32); }",
    why: "Bitwise operators take sized integer operands and preserve their integer width.",
    collision: "collisions.md Q18",
};

pub(super) const DELETE_PROPERTY: DivergenceEntry = DivergenceEntry {
    ts: "type L = \"a\"|\"b\"; @Descriptor class D { x?: L; }\nexport function main(): void { let d: D = { x: \"a\" }; delete d.x; }",
    subscript: "type L = \"a\" | \"b\"; @Descriptor class D { x?: L; }\nexport function main(): void { let d: D = { x: \"a\" }; d = {}; }",
    why: "A descriptor has a fixed C layout; assignment of a literal without the optional member clears its presence.",
    collision: "collisions.md Q6",
};

pub(super) const OPTIONAL_METHOD_CALL: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(): void {} } function probe(c: C|null): void { c?.f?.(); }\nexport function main(): void {  }",
    subscript: "class C { f(): void {} } function probe(c: C | null): void { if (c !== null) { c.f(); } }",
    why: "The optional-chain surface tests a nullable receiver for a field read or a method call; it does not test a callable.",
    collision: "C7",
};

pub(super) const OPTIONAL_FUNCTION_CALL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const f = (): i32 => 1; f?.(); }",
    subscript: "export function main(): void { const f = (): i32 => 1; f(); }",
    why: "The optional-chain surface tests a nullable receiver for a field read or a method call; it does not test a callable.",
    collision: "C7",
};

pub(super) const UNDEFINED_EQUALITY_PAIR: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const b = undefined === undefined; }",
    subscript: "export function main(): void { const b: boolean = true; }",
    why: "The undefined token is a `T | undefined` or `void` result, or an absence comparison with a nullable value or an absence-capable descriptor member.",
    collision: "C7",
};

pub(super) const UNDEFINED_EQUALITY_NON_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const x: i32 = 1; const b = undefined === x; }",
    subscript: "export function main(): void { const x: i32 = 1; const b: boolean = false; }",
    why: "The undefined token is a `T | undefined` or `void` result, or an absence comparison with a nullable value or an absence-capable descriptor member.",
    collision: "C7",
};

pub(super) const BARE_YIELD_NON_VOID: DivergenceEntry = DivergenceEntry {
    ts: "function* gen() { yield 1; yield; }\nexport function main(): void { gen(); }",
    subscript: "class Item { value: i32 = 1; }\nfunction* gen(): Generator<Item | null> { yield new Item(); yield null; } export function main(): void { gen(); }",
    why: "A bare yield has no value; a nullable reference yield type can represent an absent numeric value.",
    collision: "compiler.md §151.1",
};

pub(super) const ASYNC_METHOD_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class C { async f(): Promise<i32> { return 1; } }\nexport function main(): void { new C().f; }",
    subscript: "class C { async f(): Promise<i32> { return 1; } }\nexport async function main(): Promise<void> { await new C().f(); }",
    why: "An async method is a direct call target and has no first-class function value.",
    collision: "compiler.md §37.1",
};

pub(super) const GENERIC_ASYNC_METHOD_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class C { async f<T>(x: T): Promise<T> { return x; } }\nexport function main(): void { new C().f; }",
    subscript: "class C { async f<T>(x: T): Promise<T> { return x; } }\nexport async function main(): Promise<void> { await new C().f<i32>(1); }",
    why: "A generic async method is a direct call target and has no first-class function value.",
    collision: "compiler.md §93.1",
};

pub(super) const FIXED_ARRAY_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; a.constructor; }",
    subscript: "no equivalent: The runtime has no constructor object for a fixed array.",
    why: "A fixed array supplies length, numeric elements, and the admitted callback methods; it has no object reflection members.",
    collision: "collisions.md Q3",
};

pub(super) const MAP_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Map<i32,i32>().constructor; }",
    subscript: "no equivalent: The runtime has no constructor object for a Map.",
    why: "A Map supplies the declared container API; it has no Object reflection members.",
    collision: "stdlib.md §10.4",
};

pub(super) const SET_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Set<i32>().constructor; }",
    subscript: "no equivalent: The runtime has no constructor object for a Set.",
    why: "A Set supplies the declared container API; it has no Object reflection members.",
    collision: "stdlib.md §10.4",
};

pub(super) const GENERATOR_RESULT_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "function* gen(): IterableIterator<i32> { yield 1; }\nexport function main(): void { const f = gen().next().toString; }",
    subscript: "no equivalent: A generator step has no Object method or default object formatter.",
    why: "A coroutine step result contains done and value only; it has no Object methods.",
    collision: "C8",
};

pub(super) const NUMERIC_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n: i32 = 1; n.valueOf; }",
    subscript: "export function main(): void { const n: i32 = 1; const valueOf = (): i32 => n; }",
    why: "A sized numeric supplies the admitted Number formatting methods; it has no Object reflection members.",
    collision: "stdlib.md §11",
};

pub(super) const BOUNDARY_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\n\nexport function main(): void { const o = getObj(); o.toString; }\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function getObj(): object;",
    subscript: "no equivalent: An opaque boundary object has no fields or default object formatter.",
    why: "The boundary object type has no field shape; member access requires a checked class narrowing.",
    collision: "C7",
};

pub(super) const WORKER_STATIC_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { Worker.valueOf(); }",
    subscript: "no equivalent: Worker has no runtime constructor object to return from valueOf.",
    why: "The Worker static surface supplies spawn only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const WORKER_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { const w = Worker.spawn(entry); w.valueOf(); }",
    subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { const w = Worker.spawn(entry); w; }",
    why: "A Worker supplies post, poll, close, and join only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const INBOX_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function f(i: Inbox<M>): void { i.valueOf(); }\nexport function main(): void {  }",
    subscript: "class M { x: i32 = 1; } function f(i: Inbox<M>): void { i; }",
    why: "An Inbox supplies wait and poll only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const OUTBOX_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function f(o: Outbox<M>): void { o.valueOf(); }\nexport function main(): void {  }",
    subscript: "class M { x: i32 = 1; } function f(o: Outbox<M>): void { o; }",
    why: "An Outbox supplies post only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const FIXED_ARRAY_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,1> = [1]; a.valueOf(); }",
    subscript: "export function main(): void { const a: FixedArray<i32, 1> = [1]; a; }",
    why: "A fixed array supplies length, numeric elements, and the admitted callback methods; it has no Object methods.",
    collision: "collisions.md Q3",
};

pub(super) const NUMERIC_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n: f64 = 1; n.valueOf(); }",
    subscript: "export function main(): void { const n: f64 = 1; n; }",
    why:
        "A sized numeric supplies the admitted Number formatting methods; it has no Object methods.",
    collision: "stdlib.md §11",
};

pub(super) const STRING_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { \"x\".valueOf(); }",
    subscript: "export function main(): void { \"x\"; }",
    why: "A string supplies the admitted UTF-8 string API; it has no Object reflection members.",
    collision: "stdlib.md §8",
};

pub(super) const MAP_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Map<i32,i32>().valueOf(); }",
    subscript: "export function main(): void { new Map<i32, i32>(); }",
    why: "A Map supplies the admitted container API; it has no Object methods.",
    collision: "stdlib.md §10.4",
};

pub(super) const SET_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Set<i32>().valueOf(); }",
    subscript: "export function main(): void { new Set<i32>(); }",
    why: "A Set supplies the admitted container API; it has no Object methods.",
    collision: "stdlib.md §10.4",
};

pub(super) const ARRAY_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; a.valueOf(); }",
    subscript: "export function main(): void { const a: i32[] = [1]; a; }",
    why: "An array supplies the admitted container API; it has no Object reflection members.",
    collision: "stdlib.md §9",
};

pub(super) const REGEX_COMPILE: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new RegExp(\"x\").compile(\"y\"); }",
    subscript: "export function main(): void { let r = new RegExp(\"x\"); r = new RegExp(\"y\"); }",
    why: "A RegExp pattern is fixed at construction; the admitted surface has no compile method.",
    collision: "stdlib.md §15.3",
};

pub(super) const FRACTIONAL_INTEGER_LITERAL: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const n: i32 = 1.5; }",
    subscript: "export function main(): void { const n: f64 = 1.5; }",
    why: "A fractional literal requires a floating context; an integer context cannot represent its fraction.",
    collision: "C4",
};

pub(super) const FIXED_ARRAY_LITERAL_LENGTH: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: FixedArray<i32,2> = [1]; }",
    subscript: "export function main(): void { const a: FixedArray<i32, 2> = [1, 0]; }",
    why: "A fixed-array annotation sets its C array length; the constructing literal must supply exactly that many elements.",
    collision: "collisions.md Q3",
};

pub(super) const BYTE_ARGUMENT_IDENTITY: DivergenceEntry = DivergenceEntry {
    ts: "const x: FixedArray<i32,1> = [1];\nexport function main(): void { Context.bytesOf<FixedArray<i64,1>>(x); }",
    subscript: "const x: FixedArray<i32, 1> = [1];\nexport function main(): void { Context.bytesOf<FixedArray<i32, 1>>(x); }",
    why: "A byte operation reads the exact declared aggregate type; a structurally compatible aggregate has a different storage layout.",
    collision: "stdlib.md §18.1",
};

pub(super) const UNINFERRED_CONTAINER_CONSTRUCTION: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const m = new Map(); }",
    subscript: "export function main(): void { const m: Map<string, i32> = new Map(); }",
    why: "A Map or Set construction with no type arguments and no contextual type has no candidate for its type arguments.",
    collision: "compiler.md §197.1",
};

pub(super) const WORKER_SPAWN_SPREAD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {} function f(): void { Worker.spawn(...[entry] as const); }",
    subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    why: "Worker.spawn takes one directly named entry function; a spread argument does not supply that direct source form.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_ENTRY_LOCAL_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { const e = entry; Worker.spawn(e); }",
    subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    why: "A worker entry must directly name a module-level function; a local function value has no entry identity.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_ENTRY_GENERIC: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry<T>(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn<M,M>(entry); }",
    subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    why: "A worker entry must name a non-generic module-level function with the exact synchronous entry shape.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_ENTRY_STRUCTURAL_ENDPOINTS: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } class I { wait(): M|null { return null; } poll(): M|null { return null; } } class O { post(m: M): void {} } function entry(i: I, o: O): void {}\nexport function main(): void { Worker.spawn<M,M>(entry); }",
    subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    why: "A worker entry uses the builtin Inbox and Outbox identities; same-shaped source classes are different endpoint types.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_EXPLICIT_MESSAGE_IDENTITY: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {} class N { x: i32 = 1; }\nexport function main(): void { Worker.spawn<N,N>(entry); }",
    subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    why: "Explicit Worker.spawn arguments must name the entry message classes; structurally equal classes have different nominal identities.",
    collision: "C1",
};

pub(super) const AWAIT_NON_HANDLE: DivergenceEntry = DivergenceEntry {
    ts: " async function probe(): Promise<void> { await 1; }\nexport function main(): void {}",
    subscript: "async function probe(): Promise<void> { 1; }",
    why: "An await consumes an async handle or a direct admitted async call; a synchronous value carries no completion.",
    collision: "compiler.md §26.1",
};

pub(super) const AWAIT_UNDECLARED_ASYNC_FUNCTION: DivergenceEntry = DivergenceEntry {
    ts: " async function probe(): Promise<void> { await print(\"x\"); }\nexport function main(): void {}",
    subscript: "async function probe(): Promise<void> { print(\"x\"); }",
    why: "An awaited named call must resolve to a declared async function; an ambient synchronous call carries no async completion.",
    collision: "compiler.md §26.1",
};

pub(super) const AWAIT_SYNCHRONOUS_FUNCTION: DivergenceEntry = DivergenceEntry {
    ts: "function g(): i32 { return 1; } async function probe(): Promise<void> { await g(); }\nexport function main(): void {}",
    subscript: "function g(): i32 { return 1; } async function probe(): Promise<void> { g(); }",
    why: "An await requires an async completion; a synchronous function call supplies only its immediate result.",
    collision: "compiler.md §26.1",
};

pub(super) const AWAIT_COMPUTED_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class C { async f(): Promise<i32> { return 1; } } async function probe(): Promise<void> { const c = new C(); await c[\"f\"](); }\nexport function main(): void {}",
    subscript: "class C { async f(): Promise<i32> { return 1; } } async function probe(): Promise<void> { const c = new C(); await c.f(); }",
    why: "The admitted awaited method form directly names the instance method with an identifier.",
    collision: "compiler.md §37.1",
};

pub(super) const AWAIT_NON_CLASS_METHOD: DivergenceEntry = DivergenceEntry {
    ts: " async function probe(): Promise<void> { await \"x\".toString(); }\nexport function main(): void {}",
    subscript: "async function probe(): Promise<void> { \"x\"; }",
    why: "An awaited instance method belongs to an admitted reference class; a primitive method is synchronous.",
    collision: "compiler.md §37.1",
};

pub(super) const AWAIT_SYNCHRONOUS_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(): i32 { return 1; } } async function probe(): Promise<void> { await new C().f(); }\nexport function main(): void {}",
    subscript: "class C { f(): i32 { return 1; } } async function probe(): Promise<void> { new C().f(); }",
    why: "An await requires an async completion; a synchronous method call supplies only its immediate result.",
    collision: "compiler.md §37.1",
};

pub(super) const AWAIT_INDIRECT_CALL: DivergenceEntry = DivergenceEntry {
    ts: " async function probe(): Promise<void> { await (() : i32 => 1)(); }\nexport function main(): void {}",
    subscript: "async function probe(): Promise<void> { (() : i32 => 1)(); }\nexport function main(): void {}",
    why: "The call returns a synchronous value. It supplies no async completion for an await.",
    collision: "compiler.md §167",
};

pub(super) const ARRAY_UNSHIFT_EMPTY: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; a.unshift(); }",
    subscript: "export function main(): void { const a: i32[] = [1]; a.length; }",
    why: "The unshift surface inserts exactly one element; it has no zero-element overload.",
    collision: "stdlib.md §9",
};

pub(super) const ARRAY_CALLBACK_THIS_ARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const a: i32[] = [1]; a.map((x: i32): i32 => x, undefined); }",
    subscript: "export function main(): void { const a: i32[] = [1]; a.map((x: i32): i32 => x); }",
    why: "An array callback receives values and an optional index; the callback API has no thisArg parameter.",
    collision: "stdlib.md §9",
};

pub(super) const MAP_CALLBACK_THIS_ARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Map<i32,i32>().forEach((v: i32,k: i32): void => {}, undefined); }",
    subscript: "export function main(): void { new Map<i32, i32>().forEach((v: i32, k: i32): void => {}); }",
    why: "Map.forEach takes one callback; its API has no thisArg parameter.",
    collision: "stdlib.md §10.4",
};

pub(super) const SET_CALLBACK_THIS_ARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { new Set<i32>().forEach((v: i32,k: i32): void => {}, undefined); }",
    subscript: "export function main(): void { new Set<i32>().forEach((v: i32): void => {}); }",
    why: "Set.forEach takes one callback; its API has no thisArg parameter.",
    collision: "stdlib.md §10.4",
};

pub(super) const MAP_GROUP_BY_ARRAY_SOURCE: DivergenceEntry = DivergenceEntry {
    ts: "class C extends Array<i32> {} function f(): void { Map.groupBy(new C(), (x: i32): i32 => x); }",
    subscript: "export function main(): void { const xs: i32[] = []; Map.groupBy(xs, (x: i32): i32 => x); }",
    why: "Map.groupBy takes a dynamic array; a source class that inherits Array has a different nominal identity.",
    collision: "stdlib.md §10.4",
};

pub(super) const MAP_GROUP_BY_VOID_KEY: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { Map.groupBy([1],(x: i32): void => {}); }",
    subscript: "export function main(): void { Map.groupBy([1], (x: i32): i32 => x); }",
    why: "Map.groupBy needs a storable key value; a void callback supplies no key.",
    collision: "C21",
};

pub(super) const WORKER_ENTRY_SIGNATURE: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    why: "A worker entry must have exactly two endpoint parameters, return void, and have no default parameter.",
    collision: "compiler.md §40.1",
};

pub(super) const CLASS_INHERITED_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "class C {} export function main(): void { new C().toString; new C().toString(); new C().toString = (): string => \"x\"; } async function probe(): Promise<void> { await new C().toString(); }",
    subscript: "class C { toString: () => string = (): string => \"x\"; }\nexport function main(): void { const c = new C(); c.toString(); c.toString = (): string => \"x\"; }",
    why: "A nominal class has only its declared members; it does not inherit the JavaScript Object API.",
    collision: "compiler.md §6",
};

pub(super) const CLASS_RUNTIME_OBJECT: DivergenceEntry = DivergenceEntry {
    ts: "class C {} class G<T> {} export function main(): void { const value = C; C.name; G.name; }",
    subscript: "no equivalent: Classes have no runtime constructor object or constructor name property.",
    why: "A class names its nominal type and static declarations; the language has no runtime constructor object.",
    collision: "compiler.md §71.1",
};

pub(super) const MAP_COPY_NULLABLE_SOURCE: DivergenceEntry = DivergenceEntry {
    ts: "function f(source: Map<i32,i32>|null): void { new Map<i32,i32>(source); }\nexport function main(): void {}",
    subscript: "function f(source: Map<i32, i32> | null): void { if (source !== null) { new Map<i32, i32>(source); } }",
    why: "A Map copy traverses a live source Map; a nullable source does not supply that map.",
    collision: "stdlib.md §10.9",
};
