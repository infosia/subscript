//! Site examples for compiler.md §311.
use super::*;

pub(super) fn example(site: RejectionSite) -> Option<&'static DivergenceEntry> {
    match site {
        RejectionSite::Api(ApiRejectionId::StringToLocaleLowerCase, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "\"A\".toLocaleLowerCase();",
                subscript: "\"A\".toLowerCase();",
                ..Divergence::LocaleSensitiveString.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::RegexStringMatchAll, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "\"x\".matchAll(/x/g);",
                subscript:
                    "no equivalent: The runtime has no array of match objects with dynamic groups.",
                ..Divergence::RegExpSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayFind, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].find((x: i32): boolean => x > 0);",
                subscript: "const xs: i32[] = [1]; const i = xs.findIndex((x: i32): boolean => x > 0); if (i >= 0) { const value = xs[i]; }",
                ..Divergence::ArrayMethodDefaults.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayFindLast, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].findLast((x: i32): boolean => x > 0);",
                subscript: "const xs: i32[] = [1]; const i = xs.findLastIndex((x: i32): boolean => x > 0); if (i >= 0) { const value = xs[i]; }",
                ..Divergence::ArrayMethodDefaults.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayFlatMap, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].flatMap((x: i32): i32 => x);",
                subscript: "[1].map((x: i32): i32 => x);",
                ..Divergence::MethodTypeDomain.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayKeys, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const view = [1].keys();",
                subscript: "const xs: i32[] = [1]; for (let i: i32 = 0; i < xs.length; i++) { print(`${i}`); }",
                ..Divergence::IteratorTemporary.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayValues, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const view = [1].values();",
                subscript: "for (const value of [1]) { print(`${value}`); }",
                ..Divergence::IteratorTemporary.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::MapValues, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const view = new Map<i32, i32>().values();",
                subscript:
                    "for (const value of new Map<i32, i32>().values()) { print(`${value}`); }",
                ..Divergence::IteratorTemporary.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::SetKeys, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const view = new Set<i32>().keys();",
                subscript: "for (const value of new Set<i32>()) { print(`${value}`); }",
                ..Divergence::IteratorTemporary.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::SetValues, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "const view = new Set<i32>().values();",
                subscript: "for (const value of new Set<i32>()) { print(`${value}`); }",
                ..Divergence::IteratorTemporary.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayEntries, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].entries();",
                subscript: "const xs: i32[] = [1]; for (let i: i32 = 0; i < xs.length; i++) { print(`${i}:${xs[i]}`); }",
                ..Divergence::NoTupleType.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::MapEntries, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Map<i32, i32>().entries();",
                subscript: "new Map<i32, i32>().forEach((value: i32, key: i32): void => { print(`${key}:${value}`); });",
                ..Divergence::NoTupleType.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::SetEntries, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Set<i32>().entries();",
                subscript: "for (const value of new Set<i32>()) { print(`${value}:${value}`); }",
                ..Divergence::NoTupleType.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetMonth, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getMonth();",
                subscript: "new Date(0).getUTCMonth();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetDate, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getDate();",
                subscript: "new Date(0).getUTCDate();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetDay, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getDay();",
                subscript: "new Date(0).getUTCDay();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetHours, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getHours();",
                subscript: "new Date(0).getUTCHours();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetMinutes, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getMinutes();",
                subscript: "new Date(0).getUTCMinutes();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetSeconds, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getSeconds();",
                subscript: "new Date(0).getUTCSeconds();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetMilliseconds, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getMilliseconds();",
                subscript: "new Date(0).getUTCMilliseconds();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateLocalGetTimezoneOffset, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).getTimezoneOffset();",
                subscript: "no equivalent: The runtime stores UTC timestamps and has no local time zone offset.",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateStringToString, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).toString();",
                subscript: "new Date(0).toISOString();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateStringToDateString, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).toDateString();",
                subscript: "new Date(0).toISOString();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateStringToTimeString, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).toTimeString();",
                subscript: "new Date(0).toISOString();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateStringToLocaleString, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).toLocaleString();",
                subscript: "new Date(0).toISOString();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateStringToLocaleDateString, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).toLocaleDateString();",
                subscript: "new Date(0).toISOString();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::DateStringToLocaleTimeString, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).toLocaleTimeString();",
                subscript: "new Date(0).toISOString();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::JsonStringifySetK, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.stringify(new Set<i32>());",
                subscript: "const values: i32[] = []; for (const value of new Set<i32>()) { values.push(value); } JSON.stringify(values);",
                ..Divergence::JsonSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::JsonStringifyObject, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C {} JSON.stringify(new C() as object);",
                subscript: "class C {} JSON.stringify(new C());",
                ..Divergence::JsonSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::JsonStringifyFunction, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.stringify((): void => {});",
                subscript: "no equivalent: A function has no JSON value representation.",
                ..Divergence::JsonSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::JsonStringifyF16, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.stringify(1.0 as f16);",
                subscript: "JSON.stringify((1.0 as f16) as f32);",
                ..Divergence::JsonSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::JsonParseDateText, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "JSON.parse<Date>(\"0\");",
                subscript: "new Date(JSON.parse<i64>(\"0\"));",
                ..Divergence::JsonSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormParseIntValue, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "parseInt(\"1\");",
                subscript: "parseInt(\"1\", 10);",
                ..Divergence::NumberCoercionAndArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormNewNumberValue, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Number(\"1\");",
                subscript: "parseFloat(\"1\");",
                ..Divergence::NumberCoercionAndArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormToString, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "(1.0).toString();",
                subscript: "`${1.0}`;",
                ..Divergence::NumberCoercionAndArguments.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormDateParse, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Date.parse(\"1970-01-01\");",
                subscript: "Date.UTC(1970, 0, 1);",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormNewDateYearMonth, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(2026, 0);",
                subscript: "new Date(Date.UTC(2026, 0, 1));",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormDirectComparison, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0) < new Date(1);",
                subscript: "new Date(0).getTime() < new Date(1).getTime();",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormSet, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Date(0).setTime(1);",
                subscript: "new Date(1);",
                ..Divergence::DateSubset.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormReduceCallback, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].reduce((a: i32, b: i32): i32 => a + b);",
                subscript: "[1].reduce((a: i32, b: i32): i32 => a + b, 0);",
                ..Divergence::ArrayMethodDefaults.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormReduceRightCallback, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].reduceRight((a: i32, b: i32): i32 => a + b);",
                subscript: "[1].reduceRight((a: i32, b: i32): i32 => a + b, 0);",
                ..Divergence::ArrayMethodDefaults.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormCallbackValueIndexArray, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "[1].forEach((x: i32, i: i32, all: i32[]): void => {});",
                subscript: "[1].forEach((x: i32, i: i32): void => {});",
                why: "Array callbacks receive a value and an index, with no third array parameter. Omit that parameter or capture the array in a const.",
                ..Divergence::EscapingCapture.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayNullableFind, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C {} const xs: (C | null)[] = [null]; xs.find((x: C | null): boolean => x === null);",
                subscript: "class C {} const xs: (C | null)[] = [null]; xs.findIndex((x: C | null): boolean => x === null);",
                ..Divergence::NullableElementSearch.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::ArrayNullableFindLast, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "class C {} const xs: (C | null)[] = [null]; xs.findLast((x: C | null): boolean => x === null);",
                subscript: "class C {} const xs: (C | null)[] = [null]; xs.findLastIndex((x: C | null): boolean => x === null);",
                ..Divergence::NullableElementSearch.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormNewSetMap, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "new Set(new Map<i32, i32>());",
                subscript: "class Pair { key: i32; value: i32; constructor(key: i32, value: i32) { this.key = key; this.value = value; } } const pairs = new Set<Pair>(); new Map<i32, i32>().forEach((value: i32, key: i32): void => { pairs.add(new Pair(key, value)); });",
                ..Divergence::NoTupleType.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormNewSetGeneratorT, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function* g(): Generator<i32> { yield 1; } new Set(g());",
                subscript: "function* g(): Generator<i32> { yield 1; } const values = new Set<i32>(); for (const value of g()) { values.add(value); }",
                ..Divergence::GeneratorSingleUse.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormArrayFromMap, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Array.from(new Map<i32, i32>());",
                subscript: "class Pair { key: i32; value: i32; constructor(key: i32, value: i32) { this.key = key; this.value = value; } } const pairs: Pair[] = []; new Map<i32, i32>().forEach((value: i32, key: i32): void => { pairs.push(new Pair(key, value)); });",
                ..Divergence::BareMapToArray.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormArrayFromGeneratorT, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "function* g(): Generator<i32> { yield 1; } Array.from(g());",
                subscript: "function* g(): Generator<i32> { yield 1; } const values: i32[] = []; for (const value of g()) { values.push(value); }",
                ..Divergence::GeneratorSingleUse.entry()
            };
            Some(&ENTRY)
        }
        RejectionSite::Api(ApiRejectionId::FormHypotWithMoreThanTwoArguments, _) => {
            const ENTRY: DivergenceEntry = DivergenceEntry {
                ts: "Math.hypot(1.0, 2.0, 3.0);",
                subscript: "no equivalent: Three or more hypot arguments need the N-argument operation; a fold of two-argument hypot rounds differently.",
                why: "Math.hypot accepts exactly two arguments. The N-argument form has no implementation with the same rounding.",
                ..Divergence::MathSubset.entry()
            };
            Some(&ENTRY)
        }
        _ => None,
    }
}
