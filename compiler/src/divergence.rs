//! The divergence table (`specs/blocks/compiler.md` §79 rule 1).
//!
//! A reject diagnostic at a construct that stock `tsc` accepts is a
//! divergence from TypeScript. [`Divergence`] names one such topic, and
//! [`Divergence::entry`] gives the four facts a diagnostic shows: the
//! TypeScript form, the subscript form for the same intent, the reason,
//! and the record id.
//!
//! One `match` holds the content. No other place holds a fragment.
//!
//! The `collision` id is a `collisions.md` heading id (`C1`..`C14`)
//! where the record has one. Where it has none, the id names the
//! section that decided the rule (`compiler.md §67`, `stdlib.md §10`,
//! or `collisions.md Q29`).

/// One divergence topic: a construct that TypeScript accepts and this
/// language rejects.
///
/// A topic covers every reject corpus entry with the same reason, so one
/// variant can stand behind several entries.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Divergence {
    /// `any` in a declaration.
    AnyType,
    /// `eval`, `new Function`, and a write through `.prototype`.
    DynamicObjectModel,
    /// A class instance used where another same-shaped class is wanted.
    NominalClassIdentity,
    /// An object literal used as a plain class instance.
    ObjectLiteralConstruction,
    /// `extends` on a value class, and an alignment below the natural one.
    ValueClassLayout,
    /// Bare `number` in a declaration.
    BareNumber,
    /// Operands and arguments of two different numeric widths.
    SizedOperandWidths,
    /// Arithmetic in the storage-only `f16` type.
    StorageOnlyFloat16,
    /// An integer literal outside the range of its contextual type.
    IntegerLiteralRange,
    /// A `CEnum` wire value that is fractional, repeated, or too wide.
    WireEnumValues,
    /// A capturing lambda that escapes, and the container callback parameter.
    EscapingCapture,
    /// `throw` and the `try`/`catch`/`finally` statements.
    Exceptions,
    /// A general union type, and the `undefined` token.
    GeneralUnionAndUndefined,
    /// A nullish test on a non-nullable value.
    NullishNonNullable,
    /// An optional-chain test on a non-nullable value.
    OptionalChainNonNullable,
    /// A nullish assignment.
    NullishAssignment,
    /// A non-place nullish receiver in an initializer.
    NonPlaceNullishInitializer,
    /// An optional chain that will bind `undefined` in TypeScript.
    OptionalChainUnbound,
    /// A computed optional-chain step.
    OptionalChainIndex,
    /// An inline literal union, and assignment across two aliases.
    LiteralUnionAlias,
    /// An optional descriptor member, its default, and its presence read.
    OptionalDescriptorMember,
    /// The boundary-opaque `object` type in a general declaration.
    BoundaryOnlyObject,
    /// A conditional expression with no contextual type.
    ConditionalWithoutContext,
    /// `Promise` construction, statics, and combinators.
    PromiseObject,
    /// `await` in a synchronous function or at the top level.
    AwaitOutsideAsync,
    /// An async static method, generator, value-class method, or lambda.
    AsyncFunctionShape,
    /// An async call whose handle no holder awaits.
    DroppedAsyncHandle,
    /// `this` read from a field initializer.
    ThisInFieldInitializer,
    /// A class index signature without its accessors, and a compound write.
    ClassIndexSignature,
    /// A `using` declaration that is nullable, `await`ed, or inside a lambda.
    UsingDeclaration,
    /// A value-position write, a value-class write accessor, or a mirror accessor.
    NamedAccessor,
    /// A container view held as a value instead of iterated.
    IteratorTemporary,
    /// A name that the two languages resolve to different declarations.
    DeclarationScope,
    /// A module or static initializer that reads a later binding.
    ModuleInitializerOrder,
    /// A static member on a generic class, and `this` in a static method.
    StaticMemberSurface,
    /// `Math` as a value, and the variadic `Math.max`.
    MathSubset,
    /// Local-time, mutable, and current-clock `Date` forms.
    DateSubset,
    /// Locale-sensitive collation and case mapping.
    LocaleSensitiveString,
    /// `sort`, `find`, and `reduce` in their defaulted lib forms.
    ArrayMethodDefaults,
    /// An array method called with a variadic tail.
    VariadicArguments,
    /// A `Map` or `Set` key of a kind with no hash.
    MapKeyKind,
    /// `get` on a scalar-valued `Map`, which has no miss value.
    MapScalarGet,
    /// A pair-valued construction or view, which needs a tuple type.
    NoTupleType,
    /// A coercing numeric call, and an omitted radix or digit count.
    NumberCoercionAndArguments,
    /// A `JSON` input or parse target with no static field shape.
    JsonSubset,
    /// An aggregate or a stack frame past its byte limit.
    AggregateLayoutLimit,
    /// `exec`, `matchAll`, `lastIndex`, `groups`, and sticky matching.
    RegExpSubset,
    /// `replaceAll` with a literal that has no `g` flag.
    ReplaceAllGlobalFlag,
    /// A capturing or async `Worker.spawn` entry.
    WorkerEntryShape,
    /// A worker message or handle that leaves its Context.
    WorkerContextAffinity,
    /// A `switch` over a literal-union alias that is partial or repeated.
    SwitchOverAlias,
    /// `unreachable()` in a value position.
    UnreachableInValuePosition,
    /// `new` on a literal-constructible descriptor class.
    DescriptorConstruction,
    /// `Context.bytesOf` on a target with no C-identical layout.
    ByteAccessTarget,
    /// A plain literal-union alias in a host-callable entry signature.
    EntryParameterType,
    /// A chain header copied out of its enclosing extension.
    EmbeddedHeaderCopy,
    /// A generic method call that supplies no type arguments.
    GenericMethodTypeArguments,
    /// A generic method declared on a generic class.
    GenericMethodOnGenericClass,
    /// An `async` method that declares type parameters.
    AsyncGenericMethod,
}

/// The four facts that a divergence diagnostic shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DivergenceEntry {
    /// A TypeScript fragment that stock `tsc` accepts.
    pub ts: &'static str,
    /// The fragment this language accepts for the same intent, or the
    /// sentence `no equivalent; <what to do instead>`.
    pub subscript: &'static str,
    /// One sentence, 25 words or fewer, that gives the reason.
    pub why: &'static str,
    /// The record id: a `collisions.md` heading id (`C1`..`C14`), or a
    /// section id where the record has no heading.
    pub collision: &'static str,
}

impl Divergence {
    /// Every divergence topic, each one time.
    pub const ALL: &'static [Divergence] = &[
        Divergence::AnyType,
        Divergence::DynamicObjectModel,
        Divergence::NominalClassIdentity,
        Divergence::ObjectLiteralConstruction,
        Divergence::ValueClassLayout,
        Divergence::BareNumber,
        Divergence::SizedOperandWidths,
        Divergence::StorageOnlyFloat16,
        Divergence::IntegerLiteralRange,
        Divergence::WireEnumValues,
        Divergence::EscapingCapture,
        Divergence::Exceptions,
        Divergence::GeneralUnionAndUndefined,
        Divergence::NullishNonNullable,
        Divergence::OptionalChainNonNullable,
        Divergence::NullishAssignment,
        Divergence::NonPlaceNullishInitializer,
        Divergence::OptionalChainUnbound,
        Divergence::OptionalChainIndex,
        Divergence::LiteralUnionAlias,
        Divergence::OptionalDescriptorMember,
        Divergence::BoundaryOnlyObject,
        Divergence::ConditionalWithoutContext,
        Divergence::PromiseObject,
        Divergence::AwaitOutsideAsync,
        Divergence::AsyncFunctionShape,
        Divergence::DroppedAsyncHandle,
        Divergence::ThisInFieldInitializer,
        Divergence::ClassIndexSignature,
        Divergence::UsingDeclaration,
        Divergence::NamedAccessor,
        Divergence::IteratorTemporary,
        Divergence::DeclarationScope,
        Divergence::ModuleInitializerOrder,
        Divergence::StaticMemberSurface,
        Divergence::MathSubset,
        Divergence::DateSubset,
        Divergence::LocaleSensitiveString,
        Divergence::ArrayMethodDefaults,
        Divergence::VariadicArguments,
        Divergence::MapKeyKind,
        Divergence::MapScalarGet,
        Divergence::NoTupleType,
        Divergence::NumberCoercionAndArguments,
        Divergence::JsonSubset,
        Divergence::AggregateLayoutLimit,
        Divergence::RegExpSubset,
        Divergence::ReplaceAllGlobalFlag,
        Divergence::WorkerEntryShape,
        Divergence::WorkerContextAffinity,
        Divergence::SwitchOverAlias,
        Divergence::UnreachableInValuePosition,
        Divergence::DescriptorConstruction,
        Divergence::ByteAccessTarget,
        Divergence::EntryParameterType,
        Divergence::EmbeddedHeaderCopy,
        Divergence::GenericMethodTypeArguments,
        Divergence::GenericMethodOnGenericClass,
        Divergence::AsyncGenericMethod,
    ];

    /// The four facts for this topic.
    ///
    /// This `match` is the whole table (§79 rule 1).
    #[must_use]
    pub fn entry(self) -> DivergenceEntry {
        match self {
            Divergence::AnyType => DivergenceEntry {
                ts: "const value: any = 1;",
                subscript: "const value: i32 = 1;",
                why: "Every declaration must carry a C layout, and `any` carries none, \
                      so no storage can be given to it.",
                collision: "compiler.md §6",
            },
            Divergence::DynamicObjectModel => DivergenceEntry {
                ts: "class Greeter { message: string = \"hello\"; }\n\
                     Greeter.prototype.message = \"changed\";\n\
                     eval(\"print(1)\");",
                subscript: "class Greeter { message: string = \"hello\"; }\n\
                            const g: Greeter = new Greeter();\n\
                            g.message = \"changed\";",
                why: "The compiler runs ahead of time and a class lowers to a fixed C \
                      layout, so no code and no member appear at run time.",
                collision: "compiler.md §6",
            },
            Divergence::NominalClassIdentity => DivergenceEntry {
                ts: "class A { value: i32 = 1; }\n\
                     class B { value: i32 = 2; }\n\
                     const a: A = new B();",
                subscript: "class A { value: i32 = 1; }\n\
                            const a: A = new A();",
                why: "Each class declaration is one nominal type, so a class with the \
                      same shape is a different type.",
                collision: "C1",
            },
            Divergence::ObjectLiteralConstruction => DivergenceEntry {
                ts: "class Shape { value!: i32; }\n\
                     const s: Shape = { value: 1 };",
                subscript: "@Descriptor class Shape { value!: i32; }\n\
                            const s: Shape = { value: 1 };",
                why: "An object literal has no nominal identity, so only a `@Descriptor` \
                      class takes a literal as its construction.",
                collision: "C1",
            },
            Divergence::ValueClassLayout => DivergenceEntry {
                ts: "@CStruct class Base { value: i32 = 4; }\n\
                     @CStruct class Derived extends Base { extra: i32 = 5; }",
                subscript: "@CStruct class Base { value: i32 = 4; }\n\
                            @CStruct class Derived { base: Base = new Base(); extra: i32 = 5; }",
                why: "A value class lowers to a plain C struct, so it has no base class \
                      and no alignment below its natural one.",
                collision: "C2",
            },
            Divergence::BareNumber => DivergenceEntry {
                ts: "const count: number = 3;",
                subscript: "const count: i32 = 3;",
                why: "`number` is a 64-bit float with no C width, so every declaration \
                      names one of the sized types.",
                collision: "C3",
            },
            Divergence::SizedOperandWidths => DivergenceEntry {
                ts: "const left: i8 = 1;\n\
                     const right: i16 = 2;\n\
                     const value: i16 = left + right;",
                subscript: "const left: i8 = 1;\n\
                            const right: i16 = 2;\n\
                            const value: i16 = (left as i16) + right;",
                why: "An implicit conversion hides a width change, so every mixed-width \
                      operand and argument takes an explicit `as`.",
                collision: "C3",
            },
            Divergence::StorageOnlyFloat16 => DivergenceEntry {
                ts: "const left: f16 = 1.0;\n\
                     const right: f16 = 2.0;\n\
                     const value: f16 = left + right;",
                subscript: "const left: f16 = 1.0;\n\
                            const right: f16 = 2.0;\n\
                            const value: f16 = ((left as f32) + (right as f32)) as f16;",
                why: "`f16` is a storage format with no portable C arithmetic, so \
                      computation runs in `f32` and converts back.",
                collision: "compiler.md §16",
            },
            Divergence::IntegerLiteralRange => DivergenceEntry {
                ts: "const big: i32 = 3000000000;",
                subscript: "const big: i64 = 3000000000;",
                why: "A literal takes the sized type of its context, so a value outside \
                      that range has no representation.",
                collision: "C4",
            },
            Divergence::WireEnumValues => DivergenceEntry {
                ts: "type Wire = CEnum<{ \"m0\": 1.5 }>;",
                subscript: "type Wire = CEnum<{ \"m0\": 1 }>;",
                why: "A `CEnum` member carries a C constant, so each wire value is a \
                      distinct integer inside the `i32` range.",
                collision: "compiler.md §50",
            },
            Divergence::EscapingCapture => DivergenceEntry {
                ts: "function makeAdder(k: i32): (v: i32) => i32 {\n\
                     \x20 return (v: i32): i32 => v + k;\n\
                     }",
                subscript: "function add(k: i32, v: i32): i32 {\n\
                            \x20 return k + v;\n\
                            }",
                why: "A capturing lambda holds its environment on the stack, so it cannot \
                      outlive the function that made it.",
                collision: "C5",
            },
            Divergence::Exceptions => DivergenceEntry {
                ts: "function fail(): void {\n\
                     \x20 throw \"failure\";\n\
                     }",
                subscript: "function fail(): i32 {\n\
                            \x20 return -1;\n\
                            }",
                why: "Unwinding cannot cross the C ABI, so a fallible operation returns a \
                      result value and a fault traps.",
                collision: "C6",
            },
            Divergence::GeneralUnionAndUndefined => DivergenceEntry {
                ts: "class Choice { value: i32 | string = 0; }\n\
                     let maybe: i32 | undefined = undefined;",
                subscript: "class Cell { value: i32 = 0; }\n\
                            let maybe: Cell | null = null;",
                why: "A general union has no single C layout, so the one union form is a \
                      nullable reference and `undefined` stays out.",
                collision: "C7",
            },
            Divergence::NullishNonNullable => DivergenceEntry {
                ts: "class Box {}\nconst a: Box = new Box();\nconst b: Box = a ?? new Box();",
                subscript: "class Box {}\nconst a: Box | null = new Box();\nconst b: Box = a ?? new Box();",
                why: "The nullish test must inspect a nullable pointer, so a non-nullable value has no null branch.",
                collision: "C7",
            },
            Divergence::OptionalChainNonNullable => DivergenceEntry {
                ts: "class Box { value: i32 = 1; }\nconst a: Box = new Box();\nconst value: i32 = a?.value ?? 0;",
                subscript: "class Box { value: i32 = 1; }\nconst a: Box = new Box();\nconst value: i32 = a.value;",
                why: "The optional test must inspect a nullable pointer, so a non-nullable receiver has no null branch.",
                collision: "C7",
            },
            Divergence::NullishAssignment => DivergenceEntry {
                ts: "class Box {}\nlet a: Box | null = null;\na ??= new Box();",
                subscript: "class Box {}\nlet a: Box | null = null;\nif (a === null) { a = new Box(); }",
                why: "`??=` has no HIR form, so the explicit null test keeps assignment and evaluation order visible.",
                collision: "C7",
            },
            Divergence::NonPlaceNullishInitializer => DivergenceEntry {
                ts: "class Box {}\nfunction maybe(): Box | null { return null; }\nclass Holder { value: Box = maybe() ?? new Box(); }",
                subscript: "class Box {}\nconst candidate: Box | null = null;\nclass Holder { value: Box = candidate ?? new Box(); }",
                why: "A non-place receiver needs a synthetic local, and an initializer has no statement list that can declare it.",
                collision: "C7",
            },
            Divergence::OptionalChainUnbound => DivergenceEntry {
                ts: "class Box { value: i32 = 1; }\nconst x: Box | null = new Box();\nprint(`${x?.value}`);",
                subscript: "class Box { value: i32 = 1; }\nconst x: Box | null = new Box();\nprint(`${x?.value ?? 0}`);",
                why: "An unbound optional-chain result needs `undefined`, and this language has only `null`.",
                collision: "C7",
            },
            Divergence::OptionalChainIndex => DivergenceEntry {
                ts: "const values: i32[] | null = [];\nconst value = values?.[0];",
                subscript: "class Values { [i: u32]: i32; data: i32[] = [1];\n  get(i: u32): i32 { return this.data[i as i32]; }\n  set(i: u32, value: i32): void { this.data[i as i32] = value; } }\nconst values: Values | null = new Values();\nconst value: i32 = values !== null ? values[0] : 0;",
                why: "Computed optional access is outside the two chain forms that avoid binding `undefined`.",
                collision: "C7",
            },
            Divergence::LiteralUnionAlias => DivergenceEntry {
                ts: "type B = \"low\" | \"high\";\n\
                     function f(level: \"low\" | \"high\"): B { return level; }",
                subscript: "type Level = \"low\" | \"high\";\n\
                            function f(level: Level): Level { return level; }",
                why: "A closed literal set is nominal by its alias, so an inline set has \
                      no identity and two aliases stay distinct.",
                collision: "C7",
            },
            Divergence::OptionalDescriptorMember => DivergenceEntry {
                ts: "@Descriptor class D { value?: i32; }\n\
                     const d: D = { value: undefined };\n\
                     print(`${d.value}`);",
                subscript: "type Mode = \"fast\" | \"safe\";\n\
                            @Descriptor class D { value?: i32 = 1; mode?: Mode; }\n\
                            if (d.mode !== undefined) { print(`${d.mode}`); }",
                why: "An optional member must still hold a value, so it carries a default; \
                      only a closed literal set can be absent.",
                collision: "C7",
            },
            Divergence::BoundaryOnlyObject => DivergenceEntry {
                ts: "class Box { value: i32 = 1; }\n\
                     JSON.stringify(new Box() as object);",
                subscript: "class Box { value: i32 = 1; }\n\
                            JSON.stringify(new Box());",
                why: "`object` is the boundary-opaque handle with no field shape, so it is \
                      legal only at the C boundary.",
                collision: "C7",
            },
            Divergence::ConditionalWithoutContext => DivergenceEntry {
                ts: "const value = flag ? new BranchValue(7) : null;",
                subscript: "const value: BranchValue | null = flag ? new BranchValue(7) : null;",
                why: "The two arms have no common type without an annotation, and this \
                      compiler never infers a union.",
                collision: "compiler.md §45",
            },
            Divergence::PromiseObject => DivergenceEntry {
                ts: "const pending = Promise.resolve(1);\n\
                     leaf().then((v) => print(`${v}`));",
                subscript: "const value: i32 = await leaf();",
                why: "No event loop and no `Promise` object exist; `await` polls the frame \
                      that the Context owns.",
                collision: "C8",
            },
            Divergence::AwaitOutsideAsync => DivergenceEntry {
                ts: "await Context.suspend();",
                subscript: "export async function main(): Promise<void> {\n\
                            \x20 await Context.suspend();\n\
                            }",
                why: "Only an async function has the suspendable frame that `await` needs, \
                      so top-level `await` has no frame.",
                collision: "C8",
            },
            Divergence::AsyncFunctionShape => DivergenceEntry {
                ts: "class W { static async work(): Promise<void> {} }\n\
                     const work = async (): Promise<void> => {};",
                subscript: "class W { async work(): Promise<void> { await Context.suspend(); } }",
                why: "An async frame belongs to a Context-owned instance, so a static, a \
                      value class, a generator, and a lambda have none.",
                collision: "C8",
            },
            Divergence::DroppedAsyncHandle => DivergenceEntry {
                ts: "work();",
                subscript: "await work();",
                why: "No scheduler exists, so an async frame that no holder awaits will \
                      never run to completion.",
                collision: "compiler.md §70",
            },
            Divergence::ThisInFieldInitializer => DivergenceEntry {
                ts: "class C { tag: i32 = 2; value: i32 = this.tag + 1; }",
                subscript: "class C { tag: i32 = 2; value: i32 = 0; \
                            constructor() { this.value = this.tag + 1; } }",
                why: "A field initializer runs before the object is complete, so `this` is \
                      available only in a constructor or a method.",
                collision: "C9",
            },
            Divergence::ClassIndexSignature => DivergenceEntry {
                ts: "class Values { [i: u32]: i32; }\n\
                     const values: Values = new Values();\n\
                     const changed: i32 = values[0] = 2;",
                subscript: "class Values { [i: u32]: i32; get(i: u32): i32 { return 0; } \
                            set(i: u32, v: i32): void {} }\n\
                            values[0] = 2;\n\
                            const changed: i32 = values[0];",
                why: "Value-position writes and signatures without declared methods or on value \
                      classes stay out.",
                collision: "C10",
            },
            Divergence::UsingDeclaration => DivergenceEntry {
                ts: "using resource = maybeResource();\n\
                     const f = (): i32 => { using r = new Resource(); return 1; };",
                subscript: "const value: Resource | null = maybeResource();\n\
                            if (value !== null) { using resource = value; }",
                why: "A null binding skips its dispose silently, so the value narrows \
                      first; a lambda body and `await using` stay out.",
                collision: "C11",
            },
            Divergence::NamedAccessor => DivergenceEntry {
                ts: "class V { v: i32 = 1; get c(): i32 { return this.v; } \
                     set c(x: i32) { this.v = x; } }\n\
                     const a: V = new V();\n\
                     const changed: i32 = a.c = 2;",
                subscript: "class V { v: i32 = 1; get c(): i32 { return this.v; } \
                            set c(x: i32) { this.v = x; } }\n\
                            const a: V = new V();\n\
                            a.c = 2;\n\
                            const changed: i32 = a.c;",
                why: "Value-position writes, value-class write accessors, and mirror accessors \
                      stay out.",
                collision: "C12",
            },
            Divergence::IteratorTemporary => DivergenceEntry {
                ts: "const map: Map<i32, string> = new Map<i32, string>();\n\
                     const keys = map.keys();",
                subscript: "const map: Map<i32, string> = new Map<i32, string>();\n\
                            for (const key of map.keys()) { print(`${key}`); }",
                why: "A held iterator is a stateful value that outlives its call, so a view \
                      is a `for...of` subject only.",
                collision: "C13",
            },
            Divergence::DeclarationScope => DivergenceEntry {
                ts: "const outer: i32 = 3;\n\
                     { const read = (): i32 => outer; const outer: i32 = 4; }",
                subscript: "const outer: i32 = 3;\n\
                            { const read = (): i32 => outer; }",
                why: "The two languages resolve the name to different declarations, so this \
                      compiler rejects instead of giving a different value.",
                collision: "C14",
            },
            Divergence::ModuleInitializerOrder => DivergenceEntry {
                ts: "const g: Box = f();\n\
                     function f(): Box { return h; }\n\
                     const h: Box = new Box();",
                subscript: "const h: Box = new Box();\n\
                            function f(): Box { return h; }\n\
                            const g: Box = f();",
                why: "A module initializer runs in declaration order, so it must not read a \
                      binding that a later statement writes.",
                collision: "C14",
            },
            Divergence::StaticMemberSurface => DivergenceEntry {
                ts: "class Box<T> { static count: i32 = 0; }\n\
                     class C { static value: i32 = 1; static read(): i32 { return this.value; } }",
                subscript:
                    "class C { static value: i32 = 1; static read(): i32 { return C.value; } }",
                why: "A static member has one storage slot per class, so a generic class \
                      has no single slot and `this` has no receiver.",
                collision: "compiler.md §71",
            },
            Divergence::MathSubset => DivergenceEntry {
                ts: "const m = Math;\n\
                     print(`${Math.max(1, 2, 3)}`);",
                subscript: "print(`${Math.max(Math.max(1, 2), 3)}`);",
                why: "`Math` is a compiler namespace that lowers to intrinsics, so it is \
                      not a value and it takes no variadic call.",
                collision: "stdlib.md §1",
            },
            Divergence::DateSubset => DivergenceEntry {
                ts: "const d: Date = new Date();\n\
                     const y: i32 = d.getFullYear();\n\
                     print(`now: ${d}`);",
                subscript: "const d: Date = new Date(Date.now());\n\
                            const y: i32 = d.getUTCFullYear();\n\
                            print(`now: ${d.toISOString()}`);",
                why: "The current clock, a local time zone, and a mutable Date make output \
                      that depends on the host.",
                collision: "stdlib.md §3",
            },
            Divergence::LocaleSensitiveString => DivergenceEntry {
                ts: "const t: string = s.toLocaleUpperCase();\n\
                     const r: i32 = s.localeCompare(\"b\");",
                subscript: "const t: string = s.toUpperCase();\n\
                            const same: boolean = s === \"b\";",
                why: "Locale data is host state that changes the result, so only \
                      locale-independent case mapping and equality are in the subset.",
                collision: "stdlib.md §8",
            },
            Divergence::ArrayMethodDefaults => DivergenceEntry {
                ts: "xs.sort();\n\
                     const hit = xs.find((v: i32): boolean => v > 1);\n\
                     const total: i32 = xs.reduce((a: i32, v: i32): i32 => a + v);",
                subscript: "xs.sort((a: i32, b: i32): i32 => a - b);\n\
                            const hit: i32 = xs.findIndex((v: i32): boolean => v > 1);\n\
                            const total: i32 = xs.reduce((a: i32, v: i32): i32 => a + v, 0);",
                why: "The lib's defaults sort as strings, seed from the first element, and \
                      need a miss value that a scalar has not.",
                collision: "stdlib.md §9",
            },
            Divergence::VariadicArguments => DivergenceEntry {
                ts: "xs.splice(1, 2, 9, 9, 9);\n\
                     xs.unshift(-1, 0);",
                subscript: "xs.splice(1, 2);\n\
                            xs.unshift(-1);",
                why: "The language has no variadic parameter, so every call takes a fixed \
                      argument count.",
                collision: "stdlib.md §12",
            },
            Divergence::MapKeyKind => DivergenceEntry {
                ts: "const map: Map<i32[], i32> = new Map<i32[], i32>();",
                subscript: "const map: Map<i32, i32> = new Map<i32, i32>();",
                why: "A key needs a hash and an equality that the layout gives, so scalars, \
                      strings, and reference handles are the kinds.",
                collision: "stdlib.md §10",
            },
            Divergence::MapScalarGet => DivergenceEntry {
                ts: "print(`${map.get(1)}`);",
                subscript: "if (map.has(1)) { print(`${map.getOr(1, 0)}`); }",
                why: "A scalar has no null miss value, so a lookup is a presence check plus \
                      a defaulted read.",
                collision: "stdlib.md §10",
            },
            Divergence::NoTupleType => DivergenceEntry {
                ts: "const map: Map<i32, i32> = new Map<i32, i32>([[1, 2]]);\n\
                     for (const entry of map.entries()) { print(`${entry}`); }",
                subscript: "const map: Map<i32, i32> = new Map<i32, i32>();\n\
                            map.set(1, 2);\n\
                            for (const key of map.keys()) { print(`${key}`); }",
                why: "The language has no tuple type, so a pair has no element type to \
                      construct from or to yield.",
                collision: "stdlib.md §14",
            },
            Divergence::NumberCoercionAndArguments => DivergenceEntry {
                ts: "print(`${isNaN(1.0)}`);\n\
                     const value: f64 = Number(\"1\");\n\
                     print(value.toPrecision());",
                subscript: "print(`${Number.isNaN(1.0)}`);\n\
                            const value: f64 = parseFloat(\"1\");\n\
                            print(value.toPrecision(3));",
                why: "A coercing call reads any run-time type, and an omitted radix or \
                      digit count changes the output silently.",
                collision: "stdlib.md §11",
            },
            Divergence::JsonSubset => DivergenceEntry {
                ts: "JSON.stringify(new Map<i32, i32>());\n\
                     const parsed = JSON.parse(\"{}\");",
                subscript: "class Box { value: i32 = 1; }\n\
                            print(JSON.stringify(new Box()));\n\
                            const r: JsonResult<Box> = JSON.parse<Box>('{\"value\":1}');",
                why: "A container, a function, and a Date have no static field shape, and a \
                      parse needs a declared target type.",
                collision: "stdlib.md §13",
            },
            Divergence::AggregateLayoutLimit => DivergenceEntry {
                ts: "const data: FixedArray<u8, 2147483648> = [];",
                subscript: "const data: FixedArray<u8, 4> = [0, 0, 0, 0];",
                why: "A field offset is a signed 32-bit displacement, so one aggregate and \
                      the whole stack frame each have a byte limit.",
                collision: "collisions.md Q29",
            },
            Divergence::RegExpSubset => DivergenceEntry {
                ts: "const match = /x/.exec(\"x\");\n\
                     const index: i32 = /x/g.lastIndex;",
                subscript: "const found: boolean = /x/.test(\"x\");\n\
                            print(`${\"x\".replace(/x/, \"y\")}`);",
                why: "An exec result is an array with fields, and `lastIndex` is mutable \
                      global state; the language has neither type.",
                collision: "stdlib.md §15",
            },
            Divergence::ReplaceAllGlobalFlag => DivergenceEntry {
                ts: "print(\"aaa\".replaceAll(/a/, \"Z\"));",
                subscript: "print(\"aaa\".replaceAll(/a/g, \"Z\"));",
                why: "The lib traps a non-global literal at run time; this compiler reads \
                      the flag, so it reports it at check time.",
                collision: "stdlib.md §15",
            },
            Divergence::WorkerEntryShape => DivergenceEntry {
                ts: "class Message { value: i32 = 0; }\n\
                     async function entry(inbox: Inbox<Message>, outbox: Outbox<Message>): \
                     Promise<void> {}\n\
                     Worker.spawn(entry);",
                subscript: "class Message { value: i32 = 0; }\n\
                            function entry(inbox: Inbox<Message>, outbox: Outbox<Message>): \
                            void {}\n\
                            function run(): void { \
                            const w: Worker<Message, Message> = Worker.spawn(entry); }",
                why: "A worker starts on another thread with its own Context, so its entry \
                      is a named, non-capturing, synchronous module function.",
                collision: "compiler.md §40",
            },
            Divergence::WorkerContextAffinity => DivergenceEntry {
                ts: "class TextMessage { text: string = \"\"; }\n\
                     const w: Worker<TextMessage, TextMessage> = Worker.spawn(echo);",
                subscript: "class CountMessage { count: i32 = 0; }\n\
                            function run(): void { \
                            const w: Worker<CountMessage, CountMessage> = Worker.spawn(echo); }",
                why: "A message copies between two Contexts and a handle belongs to one, so \
                      a string field and a module global stay out.",
                collision: "compiler.md §40",
            },
            Divergence::SwitchOverAlias => DivergenceEntry {
                ts: "switch (phase) { case \"queued\": break; }",
                subscript: "switch (phase) { case \"queued\": break; default: break; }",
                why: "A closed literal set dispatches on an integer, so every member has one \
                      arm, and no member has two.",
                collision: "compiler.md §41",
            },
            Divergence::UnreachableInValuePosition => DivergenceEntry {
                ts: "const value: i32 = unreachable();",
                subscript: "unreachable();",
                why: "`unreachable()` diverges and gives no value, so it is a statement and \
                      never an operand.",
                collision: "compiler.md §42",
            },
            Divergence::DescriptorConstruction => DivergenceEntry {
                ts: "@Descriptor class D { value?: i32 = 1; }\n\
                     const d: D = new D();",
                subscript: "@Descriptor class D { value?: i32 = 1; }\n\
                            const d: D = { value: 1 };",
                why: "A descriptor class has no constructor and no heap identity, so a \
                      literal in its position constructs it.",
                collision: "compiler.md §25",
            },
            Divergence::ByteAccessTarget => DivergenceEntry {
                ts: "class Node { value: i32 = 0; }\n\
                     Context.bytesOf<Node>(node);",
                subscript: "@CStruct class Point { x: i32 = 0; }\n\
                            Context.bytesOf<Point>(point);",
                why: "Storage bytes read only where the layout is C-identical, so a \
                      reference class and a handle field have none.",
                collision: "stdlib.md §18",
            },
            Divergence::EntryParameterType => DivergenceEntry {
                ts: "type Level = \"low\" | \"high\";\n\
                     export function configure(level: Level): void {}",
                subscript: "type Level = CEnum<{ \"low\": 0; \"high\": 1 }>;\n\
                            export function configure(level: Level): void {}",
                why: "A host-callable entry takes a C type, and a plain literal alias has \
                      no wire representation.",
                collision: "compiler.md §61",
            },
            Divergence::EmbeddedHeaderCopy => DivergenceEntry {
                ts: "const copied: SubChainHeader = extension.header;\n\
                     print(`${copied.sType}`);",
                subscript: "print(`${extension.header.sType}`);",
                why: "A copy carries the extension's tag with no extension behind it, so \
                      the host reads past the header.",
                collision: "compiler.md §33.5",
            },
            Divergence::GenericMethodTypeArguments => DivergenceEntry {
                ts: "class Box { identity<T>(value: T): T { return value; } }\n\
                     const box: Box = new Box();\n\
                     print(`${box.identity(1)}`);",
                subscript: "class Box { identity<T>(value: T): T { return value; } }\n\
                            const box: Box = new Box();\n\
                            print(`${box.identity<i32>(1)}`);",
                why: "Each type-argument list names one instance ahead of time, so the \
                      compiler infers no type argument from an argument.",
                collision: "compiler.md §64",
            },
            Divergence::GenericMethodOnGenericClass => DivergenceEntry {
                ts: "class Holder<T> { value: T;\n\
                       constructor(value: T) { this.value = value; }\n\
                       pick<U>(other: U): U { return other; } }",
                subscript: "class Holder<T> { value: T;\n\
                              constructor(value: T) { this.value = value; } }\n\
                            function pick<U>(other: U): U { return other; }",
                why: "The checker holds one substitution, so a class parameter and a \
                      method parameter cannot bind at the same time.",
                collision: "compiler.md §64",
            },
            Divergence::AsyncGenericMethod => DivergenceEntry {
                ts: "class Box { async load<T>(value: T): Promise<T> {\n\
                       await Context.suspend(); return value; } }",
                subscript: "class Box { async load(value: i32): Promise<i32> {\n\
                              await Context.suspend(); return value; } }",
                why: "The await grammar gains no form for a type-argument list, so an \
                      async method declares no type parameter.",
                collision: "compiler.md §64",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Divergence, DivergenceEntry};
    use std::collections::BTreeSet;

    /// The collision record, read at compile time (§79 rule 5).
    const COLLISIONS: &str = include_str!("../../specs/blocks/collisions.md");

    /// This source file, read at compile time. The variant names come
    /// from the enum text, so the count of `ALL` compares against a fact
    /// that the table itself did not produce (CLAUDE.md principle 9).
    const SOURCE: &str = include_str!("divergence.rs");

    /// Every `### C<n>` heading id in the collision record.
    fn recorded_headings() -> BTreeSet<String> {
        COLLISIONS
            .lines()
            .filter_map(|line| line.strip_prefix("### "))
            .filter_map(|rest| rest.split('.').next())
            .filter(|id| {
                id.starts_with('C') && id.len() > 1 && id[1..].chars().all(|c| c.is_ascii_digit())
            })
            .map(str::to_owned)
            .collect()
    }

    /// The variant names declared in the `Divergence` enum body.
    fn declared_variants() -> BTreeSet<String> {
        let start = SOURCE
            .find("pub enum Divergence {")
            .expect("the enum declaration");
        let body = &SOURCE[start..];
        let end = body.find("\n}\n").expect("the end of the enum body");
        body[..end]
            .lines()
            .map(str::trim)
            .filter(|line| line.ends_with(','))
            .map(|line| line.trim_end_matches(',').to_owned())
            .filter(|name| {
                name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                    && name.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .collect()
    }

    /// A collision id that is not a `C<n>` heading names the section that
    /// decided the rule. These are the accepted spellings.
    fn is_section_id(id: &str) -> bool {
        for file in ["compiler.md §", "stdlib.md §", "collisions.md §"] {
            if let Some(rest) = id.strip_prefix(file) {
                return !rest.is_empty()
                    && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
                    && rest.starts_with(|c: char| c.is_ascii_digit());
            }
        }
        if let Some(rest) = id.strip_prefix("collisions.md Q") {
            return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
        }
        false
    }

    #[test]
    fn all_lists_every_variant_one_time() {
        let declared = declared_variants();
        assert!(
            !declared.is_empty(),
            "the enum body gave no variant names; the reader is wrong"
        );

        let listed: BTreeSet<String> = Divergence::ALL.iter().map(|d| format!("{d:?}")).collect();

        let missing: Vec<&String> = declared.difference(&listed).collect();
        assert!(missing.is_empty(), "variants absent from ALL: {missing:?}");

        let unknown: Vec<&String> = listed.difference(&declared).collect();
        assert!(unknown.is_empty(), "ALL names no such variant: {unknown:?}");

        assert_eq!(
            Divergence::ALL.len(),
            listed.len(),
            "ALL lists a variant more than one time"
        );
    }

    #[test]
    fn collision_ids_and_headings_are_total() {
        let headings = recorded_headings();
        let mut bad: Vec<String> = Vec::new();
        for divergence in Divergence::ALL {
            let id = divergence.entry().collision;
            let known = if id.starts_with('C') && id[1..].chars().all(|c| c.is_ascii_digit()) {
                headings.contains(id)
            } else {
                is_section_id(id)
            };
            if !known {
                bad.push(format!("{divergence:?} cites `{id}`"));
            }
        }
        assert!(bad.is_empty(), "unrecorded collision ids: {bad:#?}");

        let cited: BTreeSet<&str> = Divergence::ALL
            .iter()
            .map(|d| d.entry().collision)
            .collect();
        let missing: Vec<String> = recorded_headings()
            .into_iter()
            .filter(|id| !cited.contains(id.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "collisions.md headings with no variant: {missing:?}"
        );
    }

    #[test]
    fn fragments_differ_and_reasons_are_short() {
        let mut same: Vec<String> = Vec::new();
        for divergence in Divergence::ALL {
            let DivergenceEntry { ts, subscript, .. } = divergence.entry();
            if ts == subscript {
                same.push(format!("{divergence:?}"));
            }
        }
        assert!(
            same.is_empty(),
            "the TypeScript and subscript fragments are equal: {same:?}"
        );

        let mut long: Vec<String> = Vec::new();
        for divergence in Divergence::ALL {
            let why = divergence.entry().why;
            let words = why.split_whitespace().count();
            if words > 25 {
                long.push(format!("{divergence:?}: {words} words"));
            }
        }
        assert!(
            long.is_empty(),
            "a reason is longer than 25 words: {long:?}"
        );
    }

    #[test]
    fn every_fragment_has_content() {
        for divergence in Divergence::ALL {
            let entry = divergence.entry();
            assert!(!entry.ts.is_empty(), "{divergence:?} has no `ts` fragment");
            assert!(
                !entry.subscript.is_empty(),
                "{divergence:?} has no `subscript` fragment"
            );
            assert!(!entry.why.is_empty(), "{divergence:?} has no reason");
            assert!(
                !entry.collision.is_empty(),
                "{divergence:?} has no collision id"
            );
        }
    }
}
