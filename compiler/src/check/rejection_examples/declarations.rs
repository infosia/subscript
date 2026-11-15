//! Site examples for compiler.md §311.
use super::*;

pub(super) fn example(site: RejectionSite) -> Option<&'static DivergenceEntry> {
    match site {
        RejectionSite::ErasedNominalTypeArguments => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class Box<T> { x: T; constructor(x: T) { this.x = x; } } function f(x: Box<i32>): void { const y: Box<f64> = x; }",
                subscript: "class Box<T> { x: T; constructor(x: T) { this.x = x; } } function f(x: Box<i32>): void { const y = new Box<f64>(x.x as f64); }",
                ..Divergence::ErasedAssignableTypeMismatch.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ThisInStaticMethodArrow => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { static f(): void { const read = (): void => { const self = this; }; } }",
                subscript: "no equivalent: A static method has no runtime constructor object to capture.",
                ..Divergence::ThisStaticMethodMember.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ClassAlignmentBelowNatural => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType({align: 2}) class C { x: f32 = 0.0; }",
                subscript: "@ValueType({align: 4}) class C { x: f32 = 0.0; }",
                ..Divergence::ValueClassLayout.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ClassFieldLayoutLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class C { x: FixedArray<u8, 2147483640>; y: u64; constructor(x: FixedArray<u8, 2147483640>) { this.x = x; this.y = 0; } }",
                subscript: "no equivalent: The accumulated field storage exceeds the signed displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::SuspendFrameMemberLayoutLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function* g(x: FixedArray<i32, 536870911>): Generator<FixedArray<i32, 536870911>> { yield x; }",
                subscript: "no equivalent: The generator frame cannot hold the parameter and its yielded aggregate within the signed displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::GeneratorFrameFinalAlignmentLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function* g(x: FixedArray<u8, 2147483631>): Generator<void> { yield; }",
                subscript: "no equivalent: ABI alignment makes the generator frame exceed the signed displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ClosureEnvironmentLayoutLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(a: FixedArray<u8, 1600000000>, b: FixedArray<u8, 1600000000>): void { const left = a; const right = b; const read = (): i32 => left.length + right.length; }",
                subscript: "no equivalent: Both aggregate captures exceed the closure environment displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::StoredAggregateLayoutLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class C { x: i32 = 1; } function f(x: FixedArray<C, 2147483647>): void {}",
                subscript: "no equivalent: The fixed aggregate storage exceeds the signed displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::SwitchCaseWriteOutsideDeclaration => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(n: i32): void { switch (n) { case 0: let x: i32 = 1; break; case 1: x = 2; break; } }",
                subscript: "function f(n: i32): void { let x: i32 = 0; switch (n) { case 0: x = 1; break; case 1: x = 2; break; } }",
                ..Divergence::DeclarationScope.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ConstructorFieldReadAfterNestedAssignment => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { x: i32; y: i32; constructor(b: boolean) { if (b) { this.x = 1; } else { this.x = 2; } this.y = this.x; this.x = 3; } }",
                subscript: "class C { x: i32; y: i32; constructor(b: boolean) { this.x = b ? 1 : 2; this.y = this.x; this.x = 3; } }",
                ..Divergence::ConstructorFieldReadWithAssignmentFact.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::LambdaUsingDeclaration => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { [Symbol.dispose](): void {} } const f = (): void => { using c = new C(); };",
                subscript: "class C { dispose(): void {} } const f = (): void => { const c = new C(); c.dispose(); };",
                ..Divergence::UsingDeclaration.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::LiteralAliasDuplicateCase => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type A = \"a\" | \"b\"; function f(x: A): void { switch (x) { case \"a\": break; case \"a\": break; case \"b\": break; } }",
                subscript: "type A = \"a\" | \"b\"; function f(x: A): void { switch (x) { case \"a\": break; case \"b\": break; } }",
                ..Divergence::SwitchOverAlias.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DescriptorDisposeMethod => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor class C { [Symbol.dispose](): void {} }",
                subscript: "@Descriptor class C {} function dispose(c: C): void {}",
                ..Divergence::UsingDeclaration.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ValueClassDisposeMethod => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class C { [Symbol.dispose](): void {} }",
                subscript: "@ValueType class C { dispose(): void {} }",
                ..Divergence::UsingDeclaration.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ValueClassWriteAccessor => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class C { get x(): i32 { return 1; } set x(value: i32) {} }",
                subscript:
                    "@ValueType class C { get x(): i32 { return 1; } setX(value: i32): void {} }",
                ..Divergence::NamedAccessor.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AsyncGeneratorMethod => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { async *g(): AsyncGenerator<i32> { yield 1; } }",
                subscript:
                    "no equivalent: The runtime has no async iterator or async generator frame.",
                ..Divergence::AsyncFunctionShape.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ValueClassAsyncMethod => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class C { async f(): Promise<void> {} }",
                subscript: "class C { async f(): Promise<void> {} }",
                ..Divergence::AsyncFunctionShape.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::TypeOnlyExportDeclaration => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type A = \"a\" | \"b\"; export type {A};",
                subscript:
                    "// file: main.ts\nexport {};\n// file: lib.ts\nexport type A = \"a\" | \"b\";",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ExportSpecifierKind => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport * as ns from \"./other\";\n// file: other.ts\nexport const x: i32 = 1;",
                subscript: "// file: main.ts\nexport {};\n// file: lib.ts\nexport {x} from \"./other\";\n// file: other.ts\nexport const x: i32 = 1;",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::TypeOnlyOrDefaultExportSpecifier => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const x: i32 = 1; export {x as default};",
                subscript:
                    "// file: main.ts\nexport {};\n// file: lib.ts\nexport const x: i32 = 1;",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::TypeOnlyImportReexport => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport type {C} from \"./other\"; export {C};\n// file: other.ts\nexport class C {}",
                subscript: "// file: main.ts\nexport {};\n// file: lib.ts\nexport {C} from \"./other\";\n// file: other.ts\nexport class C {}",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ReexportSpecifierKind => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport * as ns from \"./other\"; export {ns};\n// file: other.ts\nexport const x: i32 = 1;",
                subscript: "// file: main.ts\nexport {};\n// file: lib.ts\nexport {x} from \"./other\";\n// file: other.ts\nexport const x: i32 = 1;",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::TypeOnlyNamespaceImport => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport type * as ns from \"./other\";\n// file: other.ts\nexport class C {}",
                subscript: "// file: main.ts\nimport {C} from \"./other\";\n// file: other.ts\nexport class C {}",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::CatchBindingUnnarrowedUse => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "try {} catch (error) { `${error}`; }",
                subscript:
                    "try {} catch (error) { if (error instanceof Error) { print(error.message); } }",
                ..Divergence::Exceptions.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::CatchBindingPattern => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "try {} catch ({message}: any) {}",
                subscript: "try {} catch (error) { if (error instanceof Error) { const message = error.message; } }",
                ..Divergence::Exceptions.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::CatchBindingAnnotation => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "try {} catch (error: any) {}",
                subscript: "try {} catch (error) {}",
                ..Divergence::Exceptions.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::InstanceofLeftNotErrorFamily => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C {} new C() instanceof Error;",
                subscript: "class C {} const value = new C(); false;",
                why: "A source class instance is not an Error. For this statically known class, the Error test is always false.",
                ..Divergence::InstanceofNonError.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::EnumNonIntegerMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "enum E { A = 1.5 }",
                subscript: "const value: f64 = 1.5;",
                ..Divergence::IntegerLiteralRange.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::WireEnumMemberNonIntegerSyntax => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type E = CEnum<{A: i32}>;",
                subscript: "type E = CEnum<{A: 0}>;",
                ..Divergence::WireEnumValues.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::WireEnumValueRange => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type E = CEnum<{A: 2147483648}>;",
                subscript:
                    "no equivalent: The wire value exceeds the signed i32 enum representation.",
                ..Divergence::WireEnumValues.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DuplicateWireEnumValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type E = CEnum<{A: 1; B: 1}>;",
                subscript: "type E = CEnum<{A: 1}>;",
                ..Divergence::WireEnumValues.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::VoidTypeOutsideResult => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const values: void[] = [];",
                subscript:
                    "no equivalent: The runtime has no storage representation for void elements.",
                ..Divergence::VoidValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::UndefinedKeywordAnnotation => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: undefined): void {}",
                subscript:
                    "no equivalent: The language has no standalone undefined parameter value.",
                ..Divergence::GeneralUnionAndUndefined.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MirrorNullableView => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"x.h\"\ndeclare function f(values: i32[] | null): void;\n// file: main.ts\nexport {};",
                subscript: "no equivalent: The C array view has no nullable representation. Change the C API to carry an explicit presence flag.",
                ..Divergence::NullableNonReference.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NullableNominalAssignmentShared => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C {} class H { value: C | null = null; } function f(h: H, touch: () => void, take: (x: C) => void): void { if (h.value !== null) { touch(); take(h.value); } }",
                subscript: "class C {} class H { value: C | null = null; } function f(h: H, touch: () => void, take: (x: C) => void): void { const value = h.value; if (value !== null) { touch(); take(value); } }",
                ..Divergence::SharedLocationNarrowing.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NullableAssignmentShared => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class H { value: (() => void) | null = null; } function f(h: H, touch: () => void, take: (x: () => void) => void): void { if (h.value !== null) { touch(); take(h.value); } }",
                subscript: "class H { value: (() => void) | null = null; } function f(h: H, touch: () => void, take: (x: () => void) => void): void { const value = h.value; if (value !== null) { touch(); take(value); } }",
                ..Divergence::SharedLocationNarrowing.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ImplicitNumericAssignment => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function take(x: i64): void {} const x: i32 = 1; take(x);",
                subscript: "function take(x: i64): void {} const x: i32 = 1; take(x as i64);",
                ..Divergence::SizedOperandWidths.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::CaptureEffectArgument => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function forward(f: () => i32): () => i32 { return f; } class C { n: i32 = 1; f(): () => i32 { return forward((): i32 => this.n); } }",
                subscript: "function forward(f: () => i32): () => i32 { return f; } class C { n: i32 = 1; f(): () => i32 { const result = this.n; return (): i32 => result; } }",
                ..Divergence::EscapingCapture.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::IndexUpdateExpressionValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { [i: i32]: i32; get(i: i32): i32 { return i; } set(i: i32, value: i32): void {} } function f(): void { const c = new C(); const x = c[0]++; } f();",
                subscript: "class C { [i: i32]: i32; get(i: i32): i32 { return i; } set(i: i32, value: i32): void {} } function f(): void { const c = new C(); const x = c[0]; c[0]++; } f();",
                ..Divergence::ClassIndexSignature.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AccessorUpdateExpressionValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { get x(): i32 { return 1; } set x(value: i32) {} } function f(): void { const c = new C(); const x = c.x++; } f();",
                subscript: "class C { get x(): i32 { return 1; } set x(value: i32) {} } function f(): void { const c = new C(); const x = c.x; c.x++; } f();",
                ..Divergence::NamedAccessor.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::OptionalMemberValueWithoutFallback => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { x: i32 = 1; } function f(c: C | null): void { c?.x; }",
                subscript: "class C { x: i32 = 1; } function f(c: C | null): void { if (c !== null) { const x = c.x; } }",
                ..Divergence::OptionalChainUnbound.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DescriptorAbsentMemberRead => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type A = \"a\" | \"b\"; @Descriptor class D { x?: A; } function f(d: D): void { `${d.x}`; }",
                subscript: "type A = \"a\" | \"b\"; @Descriptor class D { x?: A; } function f(d: D): void { if (d.x !== undefined) { print(`${d.x}`); } }",
                ..Divergence::OptionalDescriptorMember.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::InstancePrototypeMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { prototype: i32 = 1; } new C().prototype;",
                subscript: "class C { value: i32 = 1; } new C().value;",
                ..Divergence::DynamicObjectModel.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::UnknownValueName => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport * as ns from \"./other\"; const held = ns;\n// file: other.ts\nexport const x: i32 = 1;",
                subscript: "// file: main.ts\nimport {x} from \"./other\"; const held = x;\n// file: other.ts\nexport const x: i32 = 1;",
                ..Divergence::NamedModuleSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DynamicEvaluatorValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = eval;",
                subscript: "no equivalent: The runtime cannot evaluate source text.",
                ..Divergence::DynamicObjectModel.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ContextByteTargetLayout => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Context.bytesOf<FixedArray<string, 1>>([\"x\"]);",
                subscript: "no equivalent: A string handle has no portable byte representation.",
                ..Divergence::ByteAccessTarget.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseAllSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function f(): Promise<void> { const jobs: Promise<i32>[] = []; await Promise.all(...[jobs]); }",
                subscript: "async function f(): Promise<void> { const jobs: Promise<i32>[] = []; await Promise.all(jobs); }",
                ..Divergence::ForOfSpreadCall.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::CoroutineStepLayoutLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(g: Generator<FixedArray<i32, 536870911>>): void { g.next(); }",
                subscript: "no equivalent: The yielded aggregate exceeds the step result displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DynamicFunctionConstructed => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Function(\"return 1\");",
                subscript: "function f(): i32 { return 1; }",
                ..Divergence::DynamicObjectModel.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseConstructed => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Promise<i32>((resolve) => resolve(1));",
                subscript: "async function f(): Promise<i32> { return 1; } f();",
                ..Divergence::PromiseObject.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::StaticMethodAssignment => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { static f(): i32 { return 1; } } C.f = (): i32 => 2;",
                subscript: "class C { static f(): i32 { return 1; } } const f = (): i32 => 2;",
                ..Divergence::MethodAssignment.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NamespacePrototypeRead => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C<T> {} C.prototype;",
                subscript: "no equivalent: A source class has no runtime prototype object.",
                ..Divergence::DynamicObjectModel.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::GlobalPrototypeRead => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Object.setPrototypeOf;",
                subscript: "no equivalent: The runtime has no mutable prototype chain.",
                ..Divergence::DynamicObjectModel.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::WorkerEntryNotNamedFunction => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class M { x: i32 = 1; } Worker.spawn((i: Inbox<M>, o: Outbox<M>): void => {});",
                subscript: "class M { x: i32 = 1; } function entry(i: Inbox<M>, o: Outbox<M>): void {} Worker.spawn(entry);",
                ..Divergence::WorkerEntryShape.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DescriptorDefaultThisUse => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor class D { x?: i32 = this.x; }",
                subscript: "@Descriptor class D { x?: i32 = 0; }",
                ..Divergence::ThisInFieldInitializer.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::FieldInitializerThisUse => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { x: C = this; }",
                subscript: "class C { x: C | null = null; constructor() { this.x = this; } }",
                ..Divergence::ThisInFieldInitializer.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ObjectLiteralWithoutDescriptorContext => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const value = {x: 1};",
                subscript: "@Descriptor class D { x?: i32 = 1; } const value: D = {x: 1};",
                ..Divergence::ObjectLiteralConstruction.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::LogicalAssignmentNonBoolean => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "let s: string = \"\"; s ||= \"d\";",
                subscript: "let s: string = \"\"; if (s.length === 0) { s = \"d\"; }",
                ..Divergence::LogicalNonBooleanOperandForm.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::StaticAccessorAssignmentExpressionValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { static get x(): i32 { return 1; } static set x(value: i32) {} } const x = (C.x = 2);",
                subscript: "class C { static get x(): i32 { return 1; } static set x(value: i32) {} } C.x = 2; const x: i32 = 2;",
                ..Divergence::NamedAccessor.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayMapVoidCallback => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].map((x: i32): void => {});",
                subscript: "[1].forEach((x: i32): void => {});",
                ..Divergence::VoidValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::BindingPatternUnsupportedRoot => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(...args: i32[]): void {}",
                subscript: "function f(args: i32[]): void {}",
                why: "A rest parameter needs variadic call and allocation semantics. Pass an explicit array parameter instead.",
                ..Divergence::NestedPattern.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ObjectBindingNestedPattern => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class B { y: i32 = 1; } class C { x: B = new B(); } function f(): void { const {x: {y}} = new C(); }",
                subscript: "class B { y: i32 = 1; } class C { x: B = new B(); } function f(): void { const {x} = new C(); const {y} = x; }",
                ..Divergence::NestedPattern.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ObjectBindingShorthandDefault => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { x: i32 = 1; } function f(): void { const {x = 1} = new C(); }",
                subscript: "class C { x: i32 = 1; } function f(): void { const {x} = new C(); }",
                ..Divergence::PatternDefaultValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ObjectBindingFieldDefault => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { x: i32 = 1; } function f(): void { const {x: y = 1} = new C(); }",
                subscript: "class C { x: i32 = 1; } function f(): void { const {x: y} = new C(); }",
                ..Divergence::PatternDefaultValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ConditionalJoinSizedOperandWidths => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const x: i32 = 1; const y: f64 = 2.0; const value = true ? x : y;",
                subscript: "const x: i32 = 1; const y: f64 = 2.0; true ? (x as f64) : y;",
                ..Divergence::SizedOperandWidths.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ConditionalJoinGeneralUnionAndUndefined => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const value = true ? 1 : \"x\";",
                subscript: "true ? `${1}` : \"x\";",
                ..Divergence::GeneralUnionAndUndefined.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AggregateAnnotationLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(): void { const [x]: FixedArray<u8, 2147483648>[] = []; }",
                subscript:
                    "no equivalent: The annotated aggregate exceeds the signed displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AssignmentLiteralAlias => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type A = \"a\" | \"b\"; type B = \"a\" | \"b\"; function f(x: A): void { const y: B = x; }",
                subscript: "type A = \"a\" | \"b\"; type B = \"a\" | \"b\"; function f(x: A): void { const y: B = x === \"a\" ? \"a\" : \"b\"; }",
                ..Divergence::LiteralUnionAlias.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::UndefinedOptionalMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "type A = \"a\" | \"b\"; @Descriptor class D { x?: A; } const value: D = {x: undefined};",
                subscript: "type A = \"a\" | \"b\"; @Descriptor class D { x?: A; } const value: D = {};",
                ..Divergence::OptionalDescriptorMember.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::LocalAggregateFrameLimit => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(input: FixedArray<u8, 2147483633>): void { const value: FixedArray<u8, 2147483633> = input; }",
                subscript: "no equivalent: ABI alignment makes the local frame exceed the signed displacement limit.",
                ..Divergence::AggregateLayoutLimit.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NullableCallShared => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { f: (() => void) | null = null; } function touch(): void {} function g(c: C): void { if (c.f !== null) { touch(); c.f(); } }",
                subscript: "class C { f: (() => void) | null = null; } function touch(): void {} function g(c: C): void { const f = c.f; if (f !== null) { touch(); f(); } }",
                ..Divergence::SharedLocationNarrowing.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::IndexedCallChecked => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(xs: (((x: i32) => i32) | null)[]): void { if (xs[0] !== null) { xs[0](1); } }",
                subscript: "function f(xs: (((x: i32) => i32) | null)[]): void { const first = xs[0]; if (first !== null) { first(1); } }",
                ..Divergence::IndexedReadNullCheck.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::IndexedAssignmentChecked => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C {} function take(x: C): void {} function f(xs: (C | null)[]): void { if (xs[0] !== null) { take(xs[0]); } }",
                subscript: "class C {} function take(x: C): void {} function f(xs: (C | null)[]): void { const first = xs[0]; if (first !== null) { take(first); } }",
                ..Divergence::IndexedReadNullCheck.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ThisStaticField => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { static n: i32 = 1; static value: i32 = this.n; }",
                subscript: "no equivalent: A source class has no runtime constructor object.",
                ..Divergence::StaticMemberSurface.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::IteratorBindingUnsupportedMembers => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function* g(): Generator<i32> { yield 1; } function f(): void { const {done, value} = g().next(); }",
                subscript: "function* g(): Generator<i32> { yield 1; } function f(): void { const step = g().next(); if (!step.done) { const value = step.value; } }",
                ..Divergence::PatternSourceShape.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::TaskGroupGeneratorBody => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function* values(): Generator<i32> { new TaskGroup(); yield 1; }",
                subscript: "async function values(): Promise<void> { const g = new TaskGroup(); await g.join(); }",
                why: "A dropped generator does not execute a lexical scope exit. Put the group in an async function and await its join.",
                collision: "collisions.md C24",
            };
            Some(&ENTRY)
        }
        RejectionSite::DateMethodValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { const d = new Date(0); const g = d.getTime; }",
                subscript: "export function main(): void { const d = new Date(0); const g = (): i64 => d.getTime(); g(); }",
                why: "A Date method lowers to a direct operation. Use a lambda that calls the method on the Date value.",
                collision: "stdlib.md §3; collisions.md C24 row 12",
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayMapFunctionResult => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function id(x: i32): i32 { return x; } export function main(): void { [1].map((x: i32): ((x: i32) => i32) => id); }",
                subscript: "function id(x: i32): i32 { return x; } export function main(): void { const fs: ((x: i32) => i32)[] = []; for (const x of [1]) { fs.push(id); } }",
                why: "map does not support a function result. Use a typed array and push each function in a for-of loop.",
                collision: "stdlib.md §9",
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayMapAsyncResult => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function f(): Promise<void> { const jobs = [1].map(async (x: i32): Promise<i32> => x); await Promise.all(jobs); }",
                subscript: "async function work(x: i32): Promise<i32> { return x; } async function f(): Promise<void> { const jobs: Promise<i32>[] = []; for (const x of [1]) { jobs.push(work(x)); } await Promise.all(jobs); }",
                why: "map cannot transfer a counted callback result. Use a for-of loop, push each handle, and await the handle array.",
                collision: "compiler.md §171",
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseCombinatorCall => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().then(null, (e: Error): i32 => 0); }",
                subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch((e: Error): i32 => 0); const s: string = await leaf().then((x: i32): string => `${x}`); }",
                why: "then takes a fulfillment callback and an optional rejection callback, and no type arguments. Use catch for a rejection callback alone.",
                collision: "collisions.md C8; compiler.md §186",
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseCatchCall => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch<i32>((e: Error): i32 => 0); }",
                subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().catch((e: Error): i32 => 0); }",
                why: "catch takes one callback with an optional Error parameter, and no type arguments.",
                collision: "collisions.md C8; compiler.md §186",
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseFinallyCall => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().finally(); }",
                subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { const v: i32 = await leaf().finally((): void => {}); }",
                why: "finally takes one callback with no parameter, and no type arguments.",
                collision: "collisions.md C8; compiler.md §180, §186",
            };
            Some(&ENTRY)
        }
        RejectionSite::AsyncArrowReceiverCapture => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C { n: i32 = 1; f(): void { const job = async (): Promise<i32> => this.n; } }",
                subscript: "class C { n: i32 = 1; f(): void { const self = this; const job = async (): Promise<i32> => self.n; } }",
                why: "The receiver must appear as an explicit const capture or a class field.",
                collision: "collisions.md C24",
            };
            Some(&ENTRY)
        }
        RejectionSite::ReactionReceiverCapture => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function leaf(): Promise<i32> { return 1; }\nclass C { n: i32 = 5; async run(): Promise<i32> { return await leaf().then((v: i32): i32 => v + this.n); } }\nexport async function main(): Promise<void> { print(`${await new C().run()}`); }",
                subscript: "async function leaf(): Promise<i32> { return 1; }\nclass C { n: i32 = 5; async run(): Promise<i32> { const n = this.n; return await leaf().then((v: i32): i32 => v + n); } }\nexport async function main(): Promise<void> { print(`${await new C().run()}`); }",
                why: "A reaction callback owns const captures. A direct this capture remains outside the accepted surface.",
                collision: "collisions.md C24; compiler.md §186",
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseThenNullRejection => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { await leaf().then((x: i32): i32 => x, null); }",
                subscript: "async function leaf(): Promise<i32> { return 1; }\nexport async function main(): Promise<void> { await leaf().then((x: i32): i32 => x); }",
                why: "Omit a null rejection callback; use then(f).",
                collision: "collisions.md C8; compiler.md §186 rule 2",
            };
            Some(&ENTRY)
        }
        RejectionSite::PromiseReactionSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function leaf(): Promise<i32> { return 1; } async function f(): Promise<void> { await leaf().then(...[(x: i32): i32 => x]); }",
                subscript: "async function leaf(): Promise<i32> { return 1; } async function f(): Promise<void> { await leaf().then((x: i32): i32 => x); }",
                why: "A reaction takes explicit callbacks; spread arguments require variadic parameters, which the language does not have.",
                collision: "collisions.md C8; compiler.md §186 rule 3",
            };
            Some(&ENTRY)
        }
        RejectionSite::CallbackSetRepeatedValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Set<i32>().forEach((v: i32, again: i32): void => {});",
                subscript: "new Set<i32>().forEach((v: i32): void => {});",
                why: "Set.forEach passes the value twice in JavaScript; this callback place passes it once.",
                collision: "stdlib.md §10.4; Q27",
            };
            Some(&ENTRY)
        }
        RejectionSite::KeyofAnnotation => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class P { x: i32 = 1; } function get(p: P, k: keyof P): i32 { return p[k]; }",
                subscript: "type Key = \"x\" | \"y\"; class P { x: i32 = 1; } function get(p: P, k: Key): i32 { return k === \"x\" ? p.x : 0; }",
                why: "The language has no keyof operator. Name the accepted keys and select their fields explicitly.",
                ..Divergence::OperatorAnnotation.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ObjectReflectionValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class P { x: i32 = 1; } Object.keys(new P());",
                subscript: "class P { x: i32 = 1; } const keys: string[] = [\"x\"];",
                why: "Object reflection has no runtime property table. List the known field names explicitly.",
                ..Divergence::LibGlobalValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ForOfArrayEntries => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const arr: i32[] = [1]; for (const [i, v] of arr.entries()) {}",
                subscript: "const arr: i32[] = [1]; for (let i: i32 = 0; i < arr.length; i++) { const v = arr[i]; }",
                ..Divergence::NoTupleType.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ForOfEntries => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const map = new Map<i32, i32>(); for (const [k, v] of map.entries()) {}",
                subscript:
                    "const map = new Map<i32, i32>(); map.forEach((v: i32, k: i32): void => {});",
                ..Divergence::NoTupleType.entry()
            };
            Some(&ENTRY)
        }
        _ => None,
    }
}
