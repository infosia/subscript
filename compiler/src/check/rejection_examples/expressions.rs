//! Site examples for compiler.md §311.
use super::*;

pub(super) fn example(site: RejectionSite) -> Option<&'static DivergenceEntry> {
    match site {
        RejectionSite::AsyncConditionalReturnBinding => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function v(n: i32): Promise<i32> { return n; }\nasync function f(c: boolean): Promise<i32> { const p = v(1); const q = v(2); return c ? p : q; }",
                subscript: "async function v(n: i32): Promise<i32> { return n; }\nasync function f(c: boolean): Promise<i32> { const p = await v(1); const q = await v(2); return c ? p : q; }",
                why: "Returning a conditional value transfers only the selected handle; each existing handle needs its own await.",
                ..Divergence::DroppedAsyncHandle.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AsyncConditionalReturnParameter => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function v(): Promise<i32> { return 1; }\nasync function many(): Promise<Promise<i32>[]> { return [v()]; }\nasync function f(c: boolean): Promise<Promise<i32>[]> { const take = c; return await many().then((p: Promise<i32>[]): Promise<i32>[] => take ? p : []); }",
                subscript: "async function v(): Promise<i32> { return 1; }\nasync function many(): Promise<Promise<i32>[]> { return [v()]; }\nasync function f(): Promise<Promise<i32>[]> { return await many().then((p: Promise<i32>[]): Promise<i32>[] => p); }",
                why: "Returning a conditional value can drop the callback parameter. Return the whole parameter or await each handle.",
                ..Divergence::DroppedAsyncHandle.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AsyncDefaultReadsParameter => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function v(): Promise<i32> { return 1; }\nasync function f(p: Promise<i32> = v(), q: Promise<i32> = p): Promise<i32> { return await q; }",
                subscript: "async function v(): Promise<i32> { return 1; }\nasync function f(p: Promise<i32> = v(), q: Promise<i32> = p): Promise<i32> { await p; return await q; }",
                why: "A default runs only for an omitted argument, so it cannot discharge an earlier parameter.",
                ..Divergence::DroppedAsyncHandle.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AsyncConditionalBinding => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function v(n: i32): Promise<i32> { return n; }\nlet stored: Promise<i32>[] = []; function g(c: boolean): void { const p = v(1); const q = v(2); stored = c ? [p] : [q]; }",
                subscript: "async function v(n: i32): Promise<i32> { return n; }\nlet stored: Promise<i32>[] = []; function g(): void { const p = v(1); stored = [p]; }",
                why: "A conditional value transfers only the selected handle. Create one handle and transfer it, or transfer both existing handles separately.",
                ..Divergence::DroppedAsyncHandle.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::AsyncConditionalParameter => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function v(): Promise<i32> { return 1; }\nasync function many(): Promise<Promise<i32>[]> { return [v()]; }\nasync function h(c: boolean): Promise<void> { const take = c; const empty: Promise<i32>[] = []; await many().then(async (p: Promise<i32>[]): Promise<void> => { await Promise.all(take ? p : empty); }); }",
                subscript: "async function v(): Promise<i32> { return 1; }\nasync function many(): Promise<Promise<i32>[]> { return [v()]; }\nasync function h(): Promise<void> { await many().then(async (p: Promise<i32>[]): Promise<void> => { await Promise.all(p); }); }",
                why: "A conditional value can drop the callback parameter. Transfer the whole parameter or await each handle.",
                ..Divergence::DroppedAsyncHandle.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArraySpreadSource => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: bag.d.ts\ndeclare class SpreadBag { [Symbol.iterator](): Iterator<i32>; }\n// file: main.ts\nconst copy = [...new SpreadBag()];",
                subscript: "const copy = [...[1]];",
                ..Divergence::UserIterationProtocol.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MapCopyKey => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Map<i32[], i32>([]);",
                subscript: "const source = new Map<i32, i32>(); new Map<i32, i32>(source);",
                ..Divergence::MapKeyKind.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ContextBytesSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class Box { x: i32 = 1; } Context.bytesOf<Box>(new Box(), ...[]);",
                subscript: "@ValueType class Box { x: i32 = 1; } Context.bytesOf<Box>(new Box());",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::CallSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function take(x: i32): void {} take(...[1]);",
                subscript: "function take(x: i32): void {} take(1);",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::SetSourceSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Set<i32>(...[[1]]);",
                subscript: "new Set<i32>([1]);",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::RegexSticky => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const pattern = /x/y;",
                subscript: "class Pattern { lastIndex: i32 = 0; test(text: string): boolean { const hit = text.slice(this.lastIndex).startsWith(\"x\"); this.lastIndex = hit ? this.lastIndex + 1 : 0; return hit; } } const pattern = new Pattern();",
                ..Divergence::RegExpSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ContextValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Context;",
                subscript: "no equivalent: Context has no first-class runtime object.",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NumberValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Number;",
                subscript: "no equivalent: Number has no first-class runtime object.",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = JSON;",
                subscript: "no equivalent: JSON has no first-class runtime object.",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DateValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Date;",
                subscript: "no equivalent: Date has no first-class runtime object.",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MapSetValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Map;",
                subscript: "no equivalent: Map has no first-class runtime object.",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NumberGlobalValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = parseInt;",
                subscript: "const held = (text: string, radix: i32): f64 => parseInt(text, radix);",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DateMemberWrite => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getTime = (): i64 => 1;",
                subscript: "const time = (): i64 => 1; time();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NumberMethodValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = (1.0).toFixed;",
                subscript: "const held = (digits: i32): string => (1.0).toFixed(digits);",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayElementDomain => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function* g(): Generator<i32> { yield 1; } [g()].forEach((value: Generator<i32>): void => { value.next(); });",
                subscript: "function* g(): Generator<i32> { yield 1; } for (const value of [g()]) { value.next(); }",
                why: "A generator is a counted type; callback positions do not carry its owner counts.",
                collision: "compiler.md §176",
            };
            Some(&ENTRY)
        }
        RejectionSite::MapCallbackCountedValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Map<i32, Promise<i32>[]>().forEach((value: Promise<i32>[], key: i32): void => {});",
                subscript: "for (const value of new Map<i32, Promise<i32>[]>().values()) {}",
                ..Divergence::MethodTypeDomain.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayCallbackSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].reduce((a: i32, b: i32): i32 => a + b, ...[0]);",
                subscript: "[1].reduce((a: i32, b: i32): i32 => a + b, 0);",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayAccumulatorDomain => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class Box { x: i32 = 0; } [1].reduce((a: Box, b: i32): Box => a, new Box());",
                subscript: "@ValueType class Box { x: i32 = 0; } const a = new Box(); for (const b of [1]) {}",
                ..Divergence::MethodTypeDomain.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayMapResult => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class Box { x: i32 = 1; } [1].map((x: i32): Box => new Box());",
                subscript: "@ValueType class Box { x: i32 = 1; } const values: Box[] = []; for (const x of [1]) { values.push(new Box()); }",
                ..Divergence::MethodTypeDomain.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MapGroupByKey => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Map.groupBy([1], (x: i32): i32[] => [x]);",
                subscript: "Map.groupBy([1], (x: i32): i32 => x);",
                ..Divergence::MapKeyKind.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayFromTypeCount => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: i32): i32 { return x * 2; } Array.from<i32, i32>([1], f);",
                subscript: "function f(x: i32): i32 { return x * 2; } [1].map(f);",
                ..Divergence::ExplicitIntrinsicTypeArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayFromSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Array.from(...[[1]]);",
                subscript: "Array.from([1]);",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayFromSource => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class Source { readonly [i: u32]: i32; length: i32 = 1; get(i: u32): i32 { return 1; } } Array.from(new Source());",
                subscript: "class Source { length: i32 = 1; get(i: u32): i32 { return 1; } } function copy(source: Source): i32[] { const values: i32[] = []; for (let i: u32 = 0; i < (source.length as u32); i++) { values.push(source.get(i)); } return values; } copy(new Source());",
                ..Divergence::SourceConstructionDomain.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::VoidPlaceDropsAsyncHandle => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "async function work(): Promise<void> {} const callback: () => void = work;",
                subscript: "async function work(): Promise<void> {} const callback: () => Promise<void> = work;",
                ..Divergence::DroppedAsyncHandle.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ContextMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Context.collect;",
                subscript: "const held = (): void => { Context.collect(); };",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = JSON.stringify;",
                subscript: "const held = (value: i32): string => JSON.stringify(value);",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayFromValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Array.from;",
                subscript: "const held = (values: i32[]): i32[] => Array.from(values);",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::ArrayMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Array.prototype;",
                subscript: "no equivalent: Array has no runtime prototype object.",
                ..Divergence::NamespaceObjectMember.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MapSetMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Set.prototype;",
                subscript: "no equivalent: Set has no runtime prototype object.",
                ..Divergence::NamespaceObjectMember.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NumberMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Number.prototype;",
                subscript: "no equivalent: Number has no runtime prototype object.",
                ..Divergence::NamespaceObjectMember.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MapGroupByValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Map.groupBy;",
                subscript: "const held = (xs: i32[]): Map<i32, i32[]> => Map.groupBy(xs, (x: i32): i32 => x);",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MathMemberWrite => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Math.sin = Math.cos;",
                subscript: "const sine = (x: f64): f64 => Math.cos(x);",
                ..Divergence::MathSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MathMethodValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Math.sin;",
                subscript: "const held = (x: f64): f64 => Math.sin(x);",
                ..Divergence::MathSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MathMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Math.toString;",
                subscript: "no equivalent: Math has no runtime object or object formatter.",
                ..Divergence::MathSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::MathCallCount => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Math.hypot(1.0);",
                subscript: "Math.hypot(1.0, 0.0);",
                ..Divergence::MathSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NumberMemberWrite => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Number.isFinite = (x: f64): boolean => true;",
                subscript: "const finite = (x: f64): boolean => true;",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NumberStaticValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Number.isFinite;",
                subscript: "const held = (x: f64): boolean => Number.isFinite(x);",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::NumberGlobalCount => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Number.parseInt(\"1\");",
                subscript: "parseInt(\"1\", 10);",
                ..Divergence::NumberCoercionAndArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::StringPatternSpread | RejectionSite::StringPatternSpreadArity => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "\"x\".split(...[\"x\"]);",
                subscript: "\"x\".split(\"x\");",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DateStaticWrite => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Date.now = (): i64 => 0;",
                subscript: "const now = (): i64 => 0;",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DateStaticValue => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const held = Date.now;",
                subscript: "const held = (): i64 => Date.now();",
                ..Divergence::CompilerOwnedValue.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DateNewSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(...[0]);",
                subscript: "new Date(0);",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::DateMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).hasOwnProperty(\"x\");",
                subscript: "no equivalent: Date has no dynamic property table.",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::HalfFloatUnary => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const x: f16 = 1.0 as f16; -x;",
                subscript: "const x: f16 = 1.0 as f16; -(x as f32);",
                ..Divergence::StorageOnlyHalfFloat.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::HalfFloatUpdate => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "let x: f16 = 1.0 as f16; x++;",
                subscript: "let x: f16 = 1.0 as f16; x = ((x as f32) + 1.0) as f16;",
                ..Divergence::StorageOnlyHalfFloat.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonStaticMember => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.toString();",
                subscript: "no equivalent: JSON has no runtime object or object formatter.",
                ..Divergence::NamespaceObjectMember.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonStringifySpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.stringify(...[1]);",
                subscript: "JSON.stringify(1);",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonParseReviver => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.parse(\"1\", (key: string, value: i32): i32 => value);",
                subscript: "JSON.parse<i32>(\"1\");",
                ..Divergence::JsonCallArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonParseSpread => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.parse(\"1\", ...[]);",
                subscript: "JSON.parse<i32>(\"1\");",
                ..Divergence::VariadicArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonParseDomain => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.parse<Map<i32, i32>>(\"{}\");",
                subscript: "no equivalent: The JSON parser has no Map construction form.",
                ..Divergence::JsonTypeDomain.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::JsonError => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.stringify(new Error(\"x\"));",
                subscript: "JSON.stringify(new Error(\"x\").message);",
                ..Divergence::JsonSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::RegexMatchType => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function f(match: RegExpMatchArray): void {}",
                subscript:
                    "no equivalent: A match array with dynamic named groups has no admitted type.",
                ..Divergence::RegExpSubset.entry()
            };
            Some(&ENTRY)
        }
        _ => None,
    }
}
