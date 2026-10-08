//! Fragment records for the group a rejection guards.

use super::DivergenceEntry;

pub(super) const UNARY_NUMERIC_COERCION: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const x = -\"2\"; }",
    subscript: "no equivalent; convert the source to a sized numeric before negation",
    why: "Unary negation takes a sized numeric operand; it does not coerce a string to a number.",
    collision: "C3",
};

pub(super) const BITWISE_INTEGER_OPERAND: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const n: f64 = 1; const x = ~n; }",
    subscript: "no equivalent; convert the operand to a sized integer before the bitwise operation",
    why: "Bitwise operators take sized integer operands and preserve their integer width.",
    collision: "collisions.md Q18",
};

pub(super) const DELETE_PROPERTY: DivergenceEntry = DivergenceEntry {
    ts: "type L = \"a\"|\"b\"; @Descriptor class D { x?: L; }\nexport function main(): void { const d: D = {}; delete d.x; }",
    subscript: "no equivalent; use Context.free to release an instance",
    why: "A class has a fixed C layout; delete cannot remove a property. Context.free releases an entire instance.",
    collision: "collisions.md Q6",
};

pub(super) const OPTIONAL_METHOD_CALL: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(): void {} } function probe(c: C|null): void { c?.f?.(); }\nexport function main(): void {  }",
    subscript: "no equivalent; test the nullable function with a null comparison before a direct call",
    why: "The optional-chain surface tests a nullable receiver for a field read or a method call; it does not test a callable.",
    collision: "C7",
};

pub(super) const OPTIONAL_FUNCTION_CALL: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const f = (): i32 => 1; f?.(); }",
    subscript: "no equivalent; test the nullable function with a null comparison before a direct call",
    why: "The optional-chain surface tests a nullable receiver for a field read or a method call; it does not test a callable.",
    collision: "C7",
};

pub(super) const UNDEFINED_EQUALITY_PAIR: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const b = undefined === undefined; }",
    subscript: "no equivalent; compare null for nullable references or test an absence-capable descriptor member",
    why: "The undefined token appears only in a presence comparison with an absence-capable descriptor member.",
    collision: "C7",
};

pub(super) const UNDEFINED_EQUALITY_NON_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const x: i32 = 1; const b = undefined === x; }",
    subscript: "no equivalent; compare null for nullable references or test an absence-capable descriptor member",
    why: "The undefined token appears only in a presence comparison with an absence-capable descriptor member.",
    collision: "C7",
};

pub(super) const BARE_YIELD_NON_VOID: DivergenceEntry = DivergenceEntry {
    ts: "function* gen() { yield 1; yield; }\nexport function main(): void { gen(); }",
    subscript: "no equivalent; give yield an operand or declare Generator<void>",
    why: "A bare yield carries no value, so only a void generator can use it.",
    collision: "compiler.md §151.1",
};

pub(super) const ASYNC_METHOD_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class C { async f(): Promise<i32> { return 1; } }\nexport function main(): void { new C().f; }",
    subscript: "no equivalent; call the async instance method directly",
    why: "An async method is a direct call target and has no first-class function value.",
    collision: "compiler.md §37.1",
};

pub(super) const GENERIC_ASYNC_METHOD_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class C { async f<T>(x: T): Promise<T> { return x; } }\nexport function main(): void { new C().f; }",
    subscript: "no equivalent; call the async instance method with explicit type arguments",
    why: "A generic async method is a direct call target and has no first-class function value.",
    collision: "compiler.md §93.1",
};

pub(super) const FIXED_ARRAY_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const a: FixedArray<i32,1> = [1]; a.constructor; }",
    subscript: "no equivalent; use length, indexing, or an admitted callback method",
    why: "A fixed array supplies length, numeric elements, and the admitted callback methods; it has no object reflection members.",
    collision: "collisions.md Q3",
};

pub(super) const MAP_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { new Map<i32,i32>().constructor; }",
    subscript: "no equivalent; use an admitted Map member",
    why: "A Map supplies the declared container API; it has no Object reflection members.",
    collision: "stdlib.md §10.4",
};

pub(super) const SET_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { new Set<i32>().constructor; }",
    subscript: "no equivalent; use an admitted Set member",
    why: "A Set supplies the declared container API; it has no Object reflection members.",
    collision: "stdlib.md §10.4",
};

pub(super) const GENERATOR_RESULT_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "function* gen(): IterableIterator<i32> { yield 1; }\nexport function main(): void { const f = gen().next().toString; }",
    subscript: "no equivalent; read done or value",
    why: "A coroutine step result contains done and value only; it has no Object methods.",
    collision: "C8",
};

pub(super) const NUMERIC_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const n: i32 = 1; n.valueOf; }",
    subscript: "no equivalent; use an admitted numeric formatting method",
    why: "A sized numeric supplies the admitted Number formatting methods; it has no Object reflection members.",
    collision: "stdlib.md §11",
};

pub(super) const BOUNDARY_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\n\nexport function main(): void { const o = getObj(); o.toString; }\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function getObj(): object;",
    subscript: "no equivalent; narrow the boundary object to its declared class before member access",
    why: "The boundary object type has no field shape; member access requires a checked class narrowing.",
    collision: "C7",
};

pub(super) const WORKER_STATIC_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { Worker.valueOf(); }",
    subscript: "no equivalent; call Worker.spawn with a directly named entry",
    why: "The Worker static surface supplies spawn only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const WORKER_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { const w = Worker.spawn(entry); w.valueOf(); }",
    subscript: "no equivalent; use an admitted Worker method",
    why: "A Worker supplies post, poll, close, and join only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const INBOX_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function f(i: Inbox<M>): void { i.valueOf(); }\nexport function main(): void {  }",
    subscript: "no equivalent; call wait or poll",
    why: "An Inbox supplies wait and poll only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const OUTBOX_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function f(o: Outbox<M>): void { o.valueOf(); }\nexport function main(): void {  }",
    subscript: "no equivalent; call post",
    why: "An Outbox supplies post only; it has no Object methods.",
    collision: "stdlib.md §16.1",
};

pub(super) const FIXED_ARRAY_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const a: FixedArray<i32,1> = [1]; a.valueOf(); }",
    subscript: "no equivalent; use an admitted fixed-array method",
    why: "A fixed array supplies length, numeric elements, and the admitted callback methods; it has no Object methods.",
    collision: "collisions.md Q3",
};

pub(super) const NUMERIC_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const n: f64 = 1; n.valueOf(); }",
    subscript: "no equivalent; use an admitted numeric formatting method",
    why:
        "A sized numeric supplies the admitted Number formatting methods; it has no Object methods.",
    collision: "stdlib.md §11",
};

pub(super) const STRING_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { \"x\".valueOf(); }",
    subscript: "no equivalent; use an admitted string member",
    why: "A string supplies the admitted UTF-8 string API; it has no Object reflection members.",
    collision: "stdlib.md §8",
};

pub(super) const MAP_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { new Map<i32,i32>().valueOf(); }",
    subscript: "no equivalent; use an admitted Map method",
    why: "A Map supplies the admitted container API; it has no Object methods.",
    collision: "stdlib.md §10.4",
};

pub(super) const SET_OBJECT_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { new Set<i32>().valueOf(); }",
    subscript: "no equivalent; use an admitted Set method",
    why: "A Set supplies the admitted container API; it has no Object methods.",
    collision: "stdlib.md §10.4",
};

pub(super) const ARRAY_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const a: i32[] = [1]; a.valueOf(); }",
    subscript: "no equivalent; use an admitted array member",
    why: "An array supplies the admitted container API; it has no Object reflection members.",
    collision: "stdlib.md §9",
};

pub(super) const REGEX_COMPILE: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { new RegExp(\"x\").compile(\"y\"); }",
    subscript: "no equivalent; construct a new RegExp for the new pattern",
    why: "A RegExp pattern is fixed at construction; the admitted surface has no compile method.",
    collision: "stdlib.md §15.3",
};

pub(super) const FRACTIONAL_INTEGER_LITERAL: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const n: i32 = 1.5; }",
    subscript: "no equivalent; use a floating annotation or an explicit numeric conversion",
    why: "A fractional literal requires a floating context; an integer context cannot represent its fraction.",
    collision: "C4",
};

pub(super) const FIXED_ARRAY_LITERAL_LENGTH: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const a: FixedArray<i32,2> = [1]; }",
    subscript: "no equivalent; supply the annotated number of elements",
    why: "A fixed-array annotation sets its C array length; the constructing literal must supply exactly that many elements.",
    collision: "collisions.md Q3",
};

pub(super) const BYTE_ARGUMENT_IDENTITY: DivergenceEntry = DivergenceEntry {
    ts: "const x: FixedArray<i32,1> = [1];\nexport function main(): void { Context.bytesOf<FixedArray<i64,1>>(x); }",
    subscript: "no equivalent; use the exact aggregate type argument that the value carries",
    why: "A byte operation reads the exact declared aggregate type; a structurally compatible aggregate has a different storage layout.",
    collision: "stdlib.md §18.1",
};

pub(super) const GENERIC_CONSTRUCTOR_TYPE_ARGUMENTS: DivergenceEntry = DivergenceEntry {
    ts: "class C<T> { constructor(x: T) {} }\nexport function main(): void { new C(1); }",
    subscript: "no equivalent; write the constructor type arguments explicitly",
    why: "A generic constructor requires explicit type arguments; constructor argument inference is outside the admitted generic-call surface.",
    collision: "compiler.md §149.1",
};

pub(super) const WORKER_SPAWN_SPREAD: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { const entries: [(i: Inbox<M>, o: Outbox<M>) => void] = [entry]; Worker.spawn(...entries); }",
    subscript: "no equivalent; pass the module-level entry function directly",
    why: "Worker.spawn takes one directly named entry function; a spread argument does not supply that direct source form.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_ENTRY_LOCAL_VALUE: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { const e = entry; Worker.spawn(e); }",
    subscript: "no equivalent; pass the module-level function name directly",
    why: "A worker entry must directly name a module-level function; a local function value has no entry identity.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_ENTRY_GENERIC: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry<T>(i: Inbox<M>, o: Outbox<M>): void {}\nexport function main(): void { Worker.spawn<M,M>(entry); }",
    subscript: "no equivalent; declare a non-generic module-level entry function",
    why: "A worker entry must name a non-generic module-level function with the exact synchronous entry shape.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_ENTRY_STRUCTURAL_ENDPOINTS: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } class I { wait(): M|null { return null; } poll(): M|null { return null; } } class O { post(m: M): void {} } function entry(i: I, o: O): void {}\nexport function main(): void { Worker.spawn<M,M>(entry); }",
    subscript: "no equivalent; declare the entry parameters as Inbox<In> and Outbox<Out>",
    why: "A worker entry uses the builtin Inbox and Outbox identities; same-shaped source classes are different endpoint types.",
    collision: "compiler.md §40.1",
};

pub(super) const WORKER_EXPLICIT_MESSAGE_IDENTITY: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {} class N { x: i32 = 1; }\nexport function main(): void { Worker.spawn<N,N>(entry); }",
    subscript: "no equivalent; use the message class names from the entry endpoint annotations",
    why: "Explicit Worker.spawn arguments must name the entry message classes; structurally equal classes have different nominal identities.",
    collision: "C1",
};

pub(super) const AWAIT_NON_HANDLE: DivergenceEntry = DivergenceEntry {
    ts: " async function probe(): Promise<void> { await 1; }\nexport function main(): void {}",
    subscript: "no equivalent; await an async handle or a direct async call",
    why: "An await consumes an async handle or a direct admitted async call; a synchronous value carries no completion.",
    collision: "compiler.md §26.1",
};

pub(super) const AWAIT_UNDECLARED_ASYNC_FUNCTION: DivergenceEntry = DivergenceEntry {
    ts: " async function probe(): Promise<void> { await print(\"x\"); }\nexport function main(): void {}",
    subscript: "no equivalent; call synchronous ambient functions without await",
    why: "An awaited named call must resolve to a declared async function; an ambient synchronous call carries no async completion.",
    collision: "compiler.md §26.1",
};

pub(super) const AWAIT_SYNCHRONOUS_FUNCTION: DivergenceEntry = DivergenceEntry {
    ts: "function g(): i32 { return 1; } async function probe(): Promise<void> { await g(); }\nexport function main(): void {}",
    subscript: "no equivalent; remove await from the synchronous call",
    why: "An await requires an async completion; a synchronous function call supplies only its immediate result.",
    collision: "compiler.md §26.1",
};

pub(super) const AWAIT_COMPUTED_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class C { async f(): Promise<i32> { return 1; } } async function probe(): Promise<void> { const c = new C(); await c[\"f\"](); }\nexport function main(): void {}",
    subscript: "no equivalent; write await receiver.method(arguments)",
    why: "The admitted awaited method form directly names the instance method with an identifier.",
    collision: "compiler.md §37.1",
};

pub(super) const AWAIT_NON_CLASS_METHOD: DivergenceEntry = DivergenceEntry {
    ts: " async function probe(): Promise<void> { await \"x\".toString(); }\nexport function main(): void {}",
    subscript: "no equivalent; call the synchronous method without await",
    why: "An awaited instance method belongs to an admitted reference class; a primitive method is synchronous.",
    collision: "compiler.md §37.1",
};

pub(super) const AWAIT_SYNCHRONOUS_METHOD: DivergenceEntry = DivergenceEntry {
    ts: "class C { f(): i32 { return 1; } } async function probe(): Promise<void> { await new C().f(); }\nexport function main(): void {}",
    subscript: "no equivalent; remove await from the synchronous method call",
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
    ts: "\nexport function main(): void { const a: i32[] = [1]; a.unshift(); }",
    subscript: "no equivalent; supply one element to unshift",
    why: "The unshift surface inserts exactly one element; it has no zero-element overload.",
    collision: "stdlib.md §9",
};

pub(super) const ARRAY_CALLBACK_THIS_ARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { const a: i32[] = [1]; a.map((x: i32): i32 => x, undefined); }",
    subscript: "no equivalent; capture the required const values in an arrow callback",
    why: "An array callback receives values and an optional index; the callback API has no thisArg parameter.",
    collision: "stdlib.md §9",
};

pub(super) const MAP_CALLBACK_THIS_ARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { new Map<i32,i32>().forEach((v: i32,k: i32): void => {}, undefined); }",
    subscript: "no equivalent; capture the required const values in an arrow callback",
    why: "Map.forEach takes one callback; its API has no thisArg parameter.",
    collision: "stdlib.md §10.4",
};

pub(super) const SET_CALLBACK_THIS_ARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { new Set<i32>().forEach((v: i32,k: i32): void => {}, undefined); }",
    subscript: "no equivalent; capture the required const values in an arrow callback",
    why: "Set.forEach takes one callback; its API has no thisArg parameter.",
    collision: "stdlib.md §10.4",
};

pub(super) const MAP_GROUP_BY_ARRAY_SOURCE: DivergenceEntry = DivergenceEntry {
    ts: "class C extends Array<i32> {}\nexport function main(): void { Map.groupBy(new C(),(x: i32): i32 => x); }",
    subscript: "no equivalent; pass a dynamic array to Map.groupBy",
    why: "Map.groupBy takes a dynamic array; a source class that inherits Array has a different nominal identity.",
    collision: "stdlib.md §10.4",
};

pub(super) const MAP_GROUP_BY_VOID_KEY: DivergenceEntry = DivergenceEntry {
    ts: "\nexport function main(): void { Map.groupBy([1],(x: i32): void => {}); }",
    subscript: "no equivalent; return an admitted key value from the callback",
    why: "Map.groupBy needs a storable key value; a void callback supplies no key.",
    collision: "C21",
};

pub(super) const WORKER_ENTRY_SIGNATURE: DivergenceEntry = DivergenceEntry {
    ts: "class M { x: i32 = 1; } function entry(i: Inbox<M>): void {}\nexport function main(): void { Worker.spawn(entry); }",
    subscript: "no equivalent; declare the exact synchronous Inbox and Outbox entry shape",
    why: "A worker entry must have exactly two endpoint parameters, return void, and have no default parameter.",
    collision: "compiler.md §40.1",
};

pub(super) const CLASS_INHERITED_OBJECT_MEMBER: DivergenceEntry = DivergenceEntry {
    ts: "class C {} export function main(): void { new C().toString; new C().toString(); new C().toString = (): string => \"x\"; } async function probe(): Promise<void> { await new C().toString(); }",
    subscript: "no equivalent; declare the required member in the class",
    why: "A nominal class has only its declared members; it does not inherit the JavaScript Object API.",
    collision: "compiler.md §6",
};

pub(super) const CLASS_RUNTIME_OBJECT: DivergenceEntry = DivergenceEntry {
    ts: "class C {} class G<T> {} export function main(): void { const value = C; C.name; G.name; }",
    subscript: "no equivalent; construct an instance or use a declared static member",
    why: "A class names its nominal type and static declarations; the language has no runtime constructor object.",
    collision: "compiler.md §71.1",
};

pub(super) const MAP_COPY_NULLABLE_SOURCE: DivergenceEntry = DivergenceEntry {
    ts: "function f(source: Map<i32,i32>|null): void { new Map<i32,i32>(source); }\nexport function main(): void {}",
    subscript: "no equivalent; narrow the source Map with a null comparison before the copy",
    why: "A Map copy traverses a live source Map; a nullable source does not supply that map.",
    collision: "stdlib.md §10.9",
};
