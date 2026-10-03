# S153: rejection TypeScript classes

Measured at `5daba081` with TypeScript 5.9.2 and rustc 1.95.0. This is a Step 0 measurement of §79 rule 6a.
No production, test, or corpus file changed. No commit or gate run occurred.

Inventory: 83 `rejected_api()` rows, 82 production S014 constructor sites, and one mirror parameter site.
`diag.rs` contains two S014 matches that construct no diagnostic; the table includes both as adapters.
The S014 assertion in `check/mod.rs:1305` is a test and is excluded.

Every witness path below is relative to `target/s153/`. Every program exports `main(): void`.
The source paths for site IDs start at `compiler/src/check/`; `diag.rs` starts at `compiler/src/`.
Each row ID gives its ambient table, receiver, and surface. The suffix files supply the other TypeScript class.
`mirror-main*.ts` uses the corresponding `mirror-pattern*.d.ts` with `--mirror`; its tsc result includes that mirror.

Built with `cargo build --offline --locked -p subscript-cli`. Every file ran through `target/debug/subscript check`.
Source guards, diagnostic text, and source positions identify the target branch. No production instrumentation was used.
The table selects the target diagnostic, even when another diagnostic appears first. Messages show their first 60 characters.
“Block” describes that diagnostic only. Full CLI output is in `<witness>.checker.txt`; the manifest records every command input.

One final tsc project includes all 315 witness programs, both mirrors, and `prelude/lang.d.ts`.
It uses the exact `tsconfig()` options from `compiler/tests/tsc_corpus.rs`: strict, noEmit, ES2022, ESNext, Bundler,
ES2022 plus ESNext.Disposable libraries, empty types, and forceConsistentCasingInFileNames.
Command: `node_modules/.bin/tsc --project target/s153/tsconfig.json --pretty false`.
Per-file TS codes come from `tsc-output.txt`; `tsc-results.json` records the run cost.

“yes” means tsc accepts and no block renders. “wrong” follows the requested criterion: tsc rejects and a block renders.
A “wrong” observation does not alone violate rule 6: that rule permits a shared site to carry its accepting-class explanation.
“Group” names an existing variant only when its reason covers the accepting witness; otherwise it says `new`.
A missing block on a first-class namespace or method needs `new` unless an existing reason states that restriction.

R1: The three REGEX_REJECTIONS rows have no checker lookup. Direct sites implement their surfaces.
R2: No checker lookup reads the JSON parse-without-target row. `json.rs:196` emits its diagnostic directly.
R3: `json_serializable` admits only serializer helper types. `collect_json_types` includes every root and child.
R4: The serializable, Error, and Date guards admit only parser helper types. Graph closure makes their lookups total.
R3 and R4 error arms require an invalid internal graph; their recursive-class probes both check successfully.
For R1/R2, † marks a measured surface diagnostic; it does not prove that the metadata row was reached.
The table marks those rows unreachable. “—” means no target-class judgement applies.

Reachable target observations: 305; tsc-accepted 144; block present 116; variant needed 86; wrong 58.
Distinct targets: variant needed 86 (35 rows, 50 S014 sites, one mirror site); wrong 58.
Unreachable targets: four metadata rows and two helper error sites; two adapters construct no diagnostic. All 80 remaining S014 sites were reached.
Primary reachable witnesses: 160; tsc-accepted 122; block present 58; variant needed 67; wrong 3.
Final tsc wall cost: 0.269608 seconds for one process over 318 source files.
Checker wall cost: 0.050292 seconds for all 315 programs in one process via `check_program`; input reads occur before the timer.
`batch.rs` links the built compiler library; this timer includes parsing and checking and excludes CLI rendering and process startup.
For comparison, the final per-file CLI sweep cost 1.274083 seconds.

The JSON f16 row renders a block despite `corpus: None`: `json.rs` emits `JsonSubset` directly. Filename selection is not the only path.

| Row/site ID | Witness | Checker code | Message, first 60 characters | Block | tsc | Variant needed | Group / reachability |
|---|---|---|---|---|---|---|---|
| STRING_REJECTIONS/string / localeCompare | a001.ts | S014 | `localeCompare` is rejected: Locale-dependent collation is u | yes | accepts | no | — |
| STRING_REJECTIONS/string / localeCompare | a001-b.ts | S014 | `localeCompare` is rejected: Locale-dependent collation is u | yes | TS2769 | wrong | — |
| STRING_REJECTIONS/string / toLocaleUpperCase | a002.ts | S014 | `toLocaleUpperCase` is rejected: Locale-sensitive case conve | yes | accepts | no | — |
| STRING_REJECTIONS/string / toLocaleUpperCase | a002-b.ts | S014 | `toLocaleUpperCase` is rejected: Locale-sensitive case conve | yes | TS2769 | wrong | — |
| STRING_REJECTIONS/string / toLocaleLowerCase | a003.ts | S014 | `toLocaleLowerCase` is rejected: Locale-sensitive case conve | no | accepts | yes | LocaleSensitiveString |
| STRING_REJECTIONS/string / toLocaleLowerCase | a003-b.ts | S014 | `toLocaleLowerCase` is rejected: Locale-sensitive case conve | no | TS2769 | no | — |
| STRING_REJECTIONS/string / normalize | a004.ts | S014 | `normalize` is rejected: Unicode normalization tables are un | no | accepts | yes | new |
| STRING_REJECTIONS/string / normalize | a004-b.ts | S014 | `normalize` is rejected: Unicode normalization tables are un | no | TS2769 | no | — |
| REGEX_STRING_REJECTIONS/string / match | a005.ts | S014 | `match` is rejected: `RegExpMatchArray.index` is optional un | no | accepts | yes | new |
| REGEX_STRING_REJECTIONS/string / match | a005-b.ts | S014 | `match` is rejected: `RegExpMatchArray.index` is optional un | no | TS2322 | no | — |
| REGEX_STRING_REJECTIONS/string / matchAll | a006.ts | S014 | `matchAll` is rejected: It needs a Q30 fusion decision and e | yes | accepts | no | — |
| REGEX_STRING_REJECTIONS/string / matchAll | a006-b.ts | S014 | `matchAll` is rejected: It needs a Q30 fusion decision and e | yes | TS2345 | wrong | — |
| REGEX_REJECTIONS/RegExp / exec | a007.ts | S014† | `RegExp.exec` is rejected: its result needs an array with ex | yes | accepts | — | R1 |
| REGEX_REJECTIONS/RegExp / exec | a007-b.ts | S014† | `RegExp.exec` is rejected: its result needs an array with ex | yes | TS2345 | — | R1 |
| REGEX_REJECTIONS/RegExp / lastIndex | a008.ts | S014† | `RegExp.lastIndex` is rejected: mutable global-match state w | yes | accepts | — | R1 |
| REGEX_REJECTIONS/RegExp / lastIndex | a008-b.ts | S014† | `RegExp.lastIndex` is rejected: mutable global-match state w | yes | TS2322 | — | R1 |
| REGEX_REJECTIONS/RegExpMatchArray / groups | a009.ts | S014† | `RegExpMatchArray` is rejected: `groups` requires an object  | yes | accepts | — | R1 |
| REGEX_REJECTIONS/RegExpMatchArray / groups | a009-b.ts | S014† | `RegExpMatchArray` is rejected: `groups` requires an object  | yes | TS2322 | — | R1 |
| ARRAY_REJECTIONS/T[] / find | a010.ts | S014 | `find` is rejected: A scalar element type has no miss value; | yes | accepts | no | — |
| ARRAY_REJECTIONS/T[] / find | a010-b.ts | S014 | `find` is rejected: A scalar element type has no miss value; | yes | TS2769 | wrong | — |
| ARRAY_REJECTIONS/T[] / findLast | a011.ts | S014 | `findLast` is rejected: A scalar element type has no miss va | no | accepts | yes | ArrayMethodDefaults |
| ARRAY_REJECTIONS/T[] / findLast | a011-b.ts | S014 | `findLast` is rejected: A scalar element type has no miss va | no | TS2345 | no | — |
| ARRAY_REJECTIONS/T[] / flat | a012.ts | S014 | `flat` is rejected: Runtime flattening depth cannot determin | no | accepts | yes | new |
| ARRAY_REJECTIONS/T[] / flat | a012-b.ts | S014 | `flat` is rejected: Runtime flattening depth cannot determin | no | TS2345 | no | — |
| ARRAY_REJECTIONS/T[] / flatMap | a013.ts | S014 | `flatMap` is rejected: The callback must return an array. (Q | no | accepts | yes | new |
| ARRAY_REJECTIONS/T[] / flatMap | a013-c.ts | S014 | `flatMap` is rejected: The callback must return an array. (Q | no | TS2322 | no | — |
| ARRAY_REJECTIONS/T[] / entries | a014.ts | S014 | `entries` is rejected: `entries()` yields a pair, but the la | no | accepts | yes | NoTupleType |
| ARRAY_REJECTIONS/T[] / entries | a014-b.ts | S014 | `entries` is rejected: `entries()` yields a pair, but the la | no | TS2554 | no | — |
| ARRAY_REJECTIONS/T[] / keys | a015.ts | S014 | `keys` is rejected: `keys()` is accepted only as the direct  | no | accepts | yes | IteratorTemporary |
| ARRAY_REJECTIONS/T[] / keys | a015-b.ts | S014 | `keys` is rejected: `keys()` is accepted only as the direct  | no | TS2554 | no | — |
| ARRAY_REJECTIONS/T[] / values | a016.ts | S014 | `values` is rejected: `values()` is accepted only as the dir | no | accepts | yes | IteratorTemporary |
| ARRAY_REJECTIONS/T[] / values | a016-b.ts | S014 | `values` is rejected: `values()` is accepted only as the dir | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getFullYear | a017.ts | S014 | `getFullYear` is rejected: Local-time accessors are unavaila | yes | accepts | no | — |
| DATE_LOCAL_REJECTIONS/Date / getFullYear | a017-b.ts | S014 | `getFullYear` is rejected: Local-time accessors are unavaila | yes | TS2554 | wrong | — |
| DATE_LOCAL_REJECTIONS/Date / getMonth | a018.ts | S014 | `getMonth` is rejected: Local-time accessors are unavailable | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getMonth | a018-b.ts | S014 | `getMonth` is rejected: Local-time accessors are unavailable | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getDate | a019.ts | S014 | `getDate` is rejected: Local-time accessors are unavailable; | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getDate | a019-b.ts | S014 | `getDate` is rejected: Local-time accessors are unavailable; | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getDay | a020.ts | S014 | `getDay` is rejected: Local-time accessors are unavailable;  | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getDay | a020-b.ts | S014 | `getDay` is rejected: Local-time accessors are unavailable;  | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getHours | a021.ts | S014 | `getHours` is rejected: Local-time accessors are unavailable | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getHours | a021-b.ts | S014 | `getHours` is rejected: Local-time accessors are unavailable | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getMinutes | a022.ts | S014 | `getMinutes` is rejected: Local-time accessors are unavailab | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getMinutes | a022-b.ts | S014 | `getMinutes` is rejected: Local-time accessors are unavailab | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getSeconds | a023.ts | S014 | `getSeconds` is rejected: Local-time accessors are unavailab | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getSeconds | a023-b.ts | S014 | `getSeconds` is rejected: Local-time accessors are unavailab | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getMilliseconds | a024.ts | S014 | `getMilliseconds` is rejected: Local-time accessors are unav | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getMilliseconds | a024-b.ts | S014 | `getMilliseconds` is rejected: Local-time accessors are unav | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getTimezoneOffset | a025.ts | S014 | `getTimezoneOffset` is rejected: The runtime has no timezone | no | accepts | yes | DateSubset |
| DATE_LOCAL_REJECTIONS/Date / getTimezoneOffset | a025-b.ts | S014 | `getTimezoneOffset` is rejected: The runtime has no timezone | no | TS2554 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getYear | a026.ts | S014 | `getYear` is rejected: Local-time accessors are unavailable; | no | TS2339 | no | — |
| DATE_LOCAL_REJECTIONS/Date / getYear | a026-b.ts | S014 | `getYear` is rejected: Local-time accessors are unavailable; | no | TS2339 | no | — |
| DATE_STRING_REJECTIONS/Date / toString | a027.ts | S014 | `toString` is rejected: Local-time formatting is unavailable | no | accepts | yes | DateSubset |
| DATE_STRING_REJECTIONS/Date / toString | a027-b.ts | S014 | `toString` is rejected: Local-time formatting is unavailable | no | TS2554 | no | — |
| DATE_STRING_REJECTIONS/Date / toDateString | a028.ts | S014 | `toDateString` is rejected: Local-time formatting is unavail | no | accepts | yes | DateSubset |
| DATE_STRING_REJECTIONS/Date / toDateString | a028-b.ts | S014 | `toDateString` is rejected: Local-time formatting is unavail | no | TS2554 | no | — |
| DATE_STRING_REJECTIONS/Date / toTimeString | a029.ts | S014 | `toTimeString` is rejected: Local-time formatting is unavail | no | accepts | yes | DateSubset |
| DATE_STRING_REJECTIONS/Date / toTimeString | a029-b.ts | S014 | `toTimeString` is rejected: Local-time formatting is unavail | no | TS2554 | no | — |
| DATE_STRING_REJECTIONS/Date / toLocaleString | a030.ts | S014 | `toLocaleString` is rejected: Locale and timezone formatting | no | accepts | yes | DateSubset |
| DATE_STRING_REJECTIONS/Date / toLocaleString | a030-b.ts | S014 | `toLocaleString` is rejected: Locale and timezone formatting | no | TS2769 | no | — |
| DATE_STRING_REJECTIONS/Date / toLocaleDateString | a031.ts | S014 | `toLocaleDateString` is rejected: Locale and timezone format | no | accepts | yes | DateSubset |
| DATE_STRING_REJECTIONS/Date / toLocaleDateString | a031-b.ts | S014 | `toLocaleDateString` is rejected: Locale and timezone format | no | TS2769 | no | — |
| DATE_STRING_REJECTIONS/Date / toLocaleTimeString | a032.ts | S014 | `toLocaleTimeString` is rejected: Locale and timezone format | no | accepts | yes | DateSubset |
| DATE_STRING_REJECTIONS/Date / toLocaleTimeString | a032-b.ts | S014 | `toLocaleTimeString` is rejected: Locale and timezone format | no | TS2769 | no | — |
| MAP_REJECTIONS/Map<K, V> / keys | a033.ts | S014 | `keys` is rejected: `keys()` is accepted only as the direct  | yes | accepts | no | — |
| MAP_REJECTIONS/Map<K, V> / keys | a033-b.ts | S014 | `keys` is rejected: `keys()` is accepted only as the direct  | yes | TS2554 | wrong | — |
| MAP_REJECTIONS/Map<K, V> / values | a034.ts | S014 | `values` is rejected: `values()` is accepted only as the dir | no | accepts | yes | IteratorTemporary |
| MAP_REJECTIONS/Map<K, V> / values | a034-b.ts | S014 | `values` is rejected: `values()` is accepted only as the dir | no | TS2554 | no | — |
| MAP_REJECTIONS/Map<K, V> / entries | a035.ts | S014 | `entries` is rejected: `entries()` yields a pair, but the la | yes | accepts | no | — |
| MAP_REJECTIONS/Map<K, V> / entries | a035-b.ts | S014 | `entries` is rejected: `entries()` yields a pair, but the la | yes | TS2554 | wrong | — |
| SET_REJECTIONS/Set<K> / keys | a036.ts | S014 | `keys` is rejected: `keys()` is accepted only as the direct  | no | accepts | yes | IteratorTemporary |
| SET_REJECTIONS/Set<K> / keys | a036-b.ts | S014 | `keys` is rejected: `keys()` is accepted only as the direct  | no | TS2554 | no | — |
| SET_REJECTIONS/Set<K> / values | a037.ts | S014 | `values` is rejected: `values()` is accepted only as the dir | no | accepts | yes | IteratorTemporary |
| SET_REJECTIONS/Set<K> / values | a037-b.ts | S014 | `values` is rejected: `values()` is accepted only as the dir | no | TS2554 | no | — |
| SET_REJECTIONS/Set<K> / entries | a038.ts | S014 | `entries` is rejected: `entries()` yields a pair, but the la | no | accepts | yes | NoTupleType |
| SET_REJECTIONS/Set<K> / entries | a038-b.ts | S014 | `entries` is rejected: `entries()` yields a pair, but the la | no | TS2554 | no | — |
| JSON_REJECTIONS/JSON / stringify(Map<K, V>) | a039.ts | S014 | `JSON.stringify` is rejected: Map is rejected rather than si | yes | accepts | no | — |
| JSON_REJECTIONS/JSON / stringify(Map<K, V>) | a039-b.ts | S014 | `JSON.stringify` is rejected: Map is rejected rather than si | yes | TS2322 | wrong | — |
| JSON_REJECTIONS/JSON / stringify(Set<K>) | a040.ts | S014 | `JSON.stringify` is rejected: Set is rejected rather than si | yes | accepts | no | — |
| JSON_REJECTIONS/JSON / stringify(Set<K>) | a040-b.ts | S014 | `JSON.stringify` is rejected: Set is rejected rather than si | yes | TS2322 | wrong | — |
| JSON_REJECTIONS/JSON / stringify(object) | a041.ts | S014 | `JSON.stringify` is rejected: The boundary-opaque object typ | yes | accepts | no | — |
| JSON_REJECTIONS/JSON / stringify(object) | a041-b.ts | S014 | `JSON.stringify` is rejected: The boundary-opaque object typ | yes | TS2322 | wrong | — |
| JSON_REJECTIONS/JSON / stringify(function) | a042.ts | S014 | `JSON.stringify` is rejected: Function values are not JSON d | yes | accepts | no | — |
| JSON_REJECTIONS/JSON / stringify(function) | a042-b.ts | S014 | `JSON.stringify` is rejected: Function values are not JSON d | yes | TS2322 | wrong | — |
| JSON_REJECTIONS/JSON / stringify(f16) | a043.ts | S014 | `JSON.stringify` is rejected: f16 is a storage-only type wit | yes | accepts | no | — |
| JSON_REJECTIONS/JSON / stringify(f16) | a043-b.ts | S014 | `JSON.stringify` is rejected: f16 is a storage-only type wit | yes | TS2322 | wrong | — |
| JSON_REJECTIONS/JSON / parse(text) without target type | a044.ts | S014† | `JSON.parse` requires a target type; use `JSON.parse<T>(text | yes | accepts | — | R2 |
| JSON_REJECTIONS/JSON / parse(text) without target type | a044-b.ts | S014† | `JSON.parse` requires a target type; use `JSON.parse<T>(text | yes | TS2769 | — | R2 |
| JSON_REJECTIONS/JSON / parse<Date>(text) | a045.ts | S014 | `JSON.parse<Date>` is rejected: An untagged ISO string canno | yes | accepts | no | — |
| JSON_REJECTIONS/JSON / parse<Date>(text) | a045-b.ts | S014 | `JSON.parse<Date>` is rejected: An untagged ISO string canno | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/global / isNaN(value) | a046.ts | S014 | `isNaN(value)` is rejected: The global form coerces its argu | yes | accepts | no | — |
| FORM_REJECTIONS/global / isNaN(value) | a046-b.ts | S014 | `isNaN(value)` is rejected: The global form coerces its argu | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/global / isFinite(value) | a047.ts | S014 | `isFinite(value)` is rejected: The global form coerces its a | no | accepts | yes | NumberCoercionAndArguments |
| FORM_REJECTIONS/global / isFinite(value) | a047-b.ts | S014 | `isFinite(value)` is rejected: The global form coerces its a | no | TS2345 | no | — |
| FORM_REJECTIONS/global / parseInt(value) | a048.ts | S014 | `parseInt(value)` is rejected: The radix is a required `i32` | yes | accepts | no | — |
| FORM_REJECTIONS/global / parseInt(value) | a048-b.ts | S014 | `parseInt(value)` is rejected: The radix is a required `i32` | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/Number / Number(value) | a049.ts | S014 | `Number(value)` is rejected: Numeric coercion is not part of | yes | accepts | no | — |
| FORM_REJECTIONS/Number / Number(value) | a049-b.ts | S014 | `Number(value)` is rejected: Numeric coercion is not part of | yes | TS2554 | wrong | — |
| FORM_REJECTIONS/Number / new Number(value) | a050.ts | S014 | `new Number(value)` is rejected: Boxed numbers and numeric c | no | accepts | yes | NumberCoercionAndArguments |
| FORM_REJECTIONS/Number / new Number(value) | a050-b.ts | S014 | `new Number(value)` is rejected: Boxed numbers and numeric c | no | TS2554 | no | — |
| FORM_REJECTIONS/f32 / f64 / toLocaleString | a051.ts | S014 | `toLocaleString` is rejected: Locale-sensitive number format | no | accepts | yes | new |
| FORM_REJECTIONS/f32 / f64 / toLocaleString | a051-b.ts | S014 | `toLocaleString` is rejected: Locale-sensitive number format | no | TS2769 | no | — |
| FORM_REJECTIONS/f32 / f64 / toString() | a052.ts | S014 | `toString()` is rejected: An explicit radix is required; use | yes | accepts | no | — |
| FORM_REJECTIONS/f32 / f64 / toString() | a052-b.ts | S014 | `toString()` is rejected: An explicit radix is required; use | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/f32 / f64 / toPrecision() | a053.ts | S014 | `toPrecision()` is rejected: An explicit digit count is requ | yes | accepts | no | — |
| FORM_REJECTIONS/f32 / f64 / toPrecision() | a053-b.ts | S014 | `toPrecision()` is rejected: An explicit digit count is requ | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/sized integers / toFixed/toString/toExponential/toPrecision | a054.ts | S014 | `toFixed` is rejected: Number formatting methods are accepte | no | accepts | yes | new |
| FORM_REJECTIONS/sized integers / toFixed/toString/toExponential/toPrecision | a054-b.ts | S014 | `toFixed` is rejected: Number formatting methods are accepte | no | TS2345 | no | — |
| FORM_REJECTIONS/Math / max/min/hypot with more than two arguments | a055.ts | S014 | `max` is rejected: Variadic parameters are outside the langu | yes | accepts | no | — |
| FORM_REJECTIONS/Math / max/min/hypot with more than two arguments | a055-b.ts | S014 | `max` is rejected: Variadic parameters are outside the langu | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/Math / Math used as a value | a056.ts | S014 | `Math used as a value` is rejected: Math is a compiler-owned | yes | accepts | no | — |
| FORM_REJECTIONS/Math / Math used as a value | a056-b.ts | S014 | `Math used as a value` is rejected: Math is a compiler-owned | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/Date / Date.parse | a057.ts | S014 | `Date.parse` is rejected: Parsing depends on timezone rules  | no | accepts | yes | DateSubset |
| FORM_REJECTIONS/Date / Date.parse | a057-b.ts | S014 | `Date.parse` is rejected: Parsing depends on timezone rules  | no | TS2345 | no | — |
| FORM_REJECTIONS/Date / new Date() | a058.ts | S014 | `new Date()` is rejected: The zero-argument constructor read | yes | accepts | no | — |
| FORM_REJECTIONS/Date / new Date() | a058-b.ts | S014 | `new Date()` is rejected: The zero-argument constructor read | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/Date / new Date(year, month, ...) | a059.ts | S014 | `new Date(year, month, ...)` is rejected: The multi-argument | yes | accepts | no | — |
| FORM_REJECTIONS/Date / new Date(year, month, ...) | a059-b.ts | S014 | `new Date(year, month, ...)` is rejected: The multi-argument | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/Date / template interpolation | a060.ts | S014 | `Date template interpolation` is rejected: Date has no impli | yes | accepts | no | — |
| FORM_REJECTIONS/Date / template interpolation | a060-b.ts | S014 | `Date template interpolation` is rejected: Date has no impli | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/Date / direct comparison | a061.ts | S014 | `Date direct comparison `===`` is rejected: Date values do n | yes | accepts | no | — |
| FORM_REJECTIONS/Date / direct comparison | a061-b.ts | S014 | `Date direct comparison `===`` is rejected: Date values do n | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/Date / set* | a062.ts | S014 | `setTime` is rejected: Date is an immutable value; use `cons | yes | accepts | no | — |
| FORM_REJECTIONS/Date / set* | a062-b.ts | S014 | `setTime` is rejected: Date is an immutable value; use `cons | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/T[] / sort() | a063.ts | S014 | `sort()` is rejected: The no-argument overload coerces eleme | yes | accepts | no | — |
| FORM_REJECTIONS/T[] / sort() | a063-b.ts | S014 | `sort()` is rejected: The no-argument overload coerces eleme | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/T[] / reduce(callback) | a064.ts | S014 | `reduce(callback)` is rejected: An explicit initial accumula | yes | accepts | no | — |
| FORM_REJECTIONS/T[] / reduce(callback) | a064-b.ts | S014 | `reduce(callback)` is rejected: An explicit initial accumula | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/T[] / reduceRight(callback) | a065.ts | S014 | `reduceRight(callback)` is rejected: An explicit initial acc | no | accepts | yes | ArrayMethodDefaults |
| FORM_REJECTIONS/T[] / reduceRight(callback) | a065-b.ts | S014 | `reduceRight(callback)` is rejected: An explicit initial acc | no | TS2345 | no | — |
| FORM_REJECTIONS/T[] / callback(value, index, array) | a066.ts | S014 | `map callback with the container parameter` is rejected: Pas | yes | accepts | no | — |
| FORM_REJECTIONS/T[] / callback(value, index, array) | a066-b.ts | S014 | `map callback with the container parameter` is rejected: Pas | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/T[] / splice(start, deleteCount, ...items) | a067.ts | S014 | `splice with inserted elements` is rejected: Variadic parame | yes | accepts | no | — |
| FORM_REJECTIONS/T[] / splice(start, deleteCount, ...items) | a067-b.ts | S014 | `splice with inserted elements` is rejected: Variadic parame | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/T[] / unshift(value, ...values) | a068.ts | S014 | `unshift with multiple elements` is rejected: Variadic param | yes | accepts | no | — |
| FORM_REJECTIONS/T[] / unshift(value, ...values) | a068-b.ts | S014 | `unshift with multiple elements` is rejected: Variadic param | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/FixedArray<T, N> / non-callback T[] methods | a069.ts | S014 | `slice` is rejected: Q27 accepts the closure-taking callback | no | TS2339 | no | — |
| FORM_REJECTIONS/Map<K, scalar V> / get(key) | a070.ts | S014 | `get(key)` is rejected: A scalar value type has no null miss | yes | accepts | no | — |
| FORM_REJECTIONS/Map<K, scalar V> / get(key) | a070-b.ts | S014 | `get(key)` is rejected: A scalar value type has no null miss | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/Map<K, V with no shared nullable-pointer form> / get(key) | a071.ts | S014 | `get(key)` is rejected: The value type has no `\| null` form  | yes | accepts | no | — |
| FORM_REJECTIONS/Map<K, V with no shared nullable-pointer form> / get(key) | a071-b.ts | S014 | `get(key)` is rejected: The value type has no `\| null` form  | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/Map / new Map(iterable) | a072.ts | S014 | `new Map(iterable)` is rejected: `new Map([[k, v]])` require | yes | accepts | no | — |
| FORM_REJECTIONS/Map / new Map(iterable) | a072-b.ts | S014 | `new Map(iterable)` is rejected: `new Map([[k, v]])` require | yes | TS2769 | wrong | — |
| FORM_REJECTIONS/Set / new Set(Map) | a073.ts | S014 | `new Set(map)` is rejected: A Map yields a pair, so invarian | no | TS2769 | no | — |
| FORM_REJECTIONS/Set / new Set(Map) | a073-b.ts | S014 | `new Set(map)` is rejected: A Map yields a pair, so invarian | no | accepts | yes | new |
| FORM_REJECTIONS/Set / new Set(Generator<T>) | a074.ts | S014 | `new Set(generator)` is rejected: A generator is single-use, | yes | accepts | no | — |
| FORM_REJECTIONS/Set / new Set(Generator<T>) | a074-b.ts | S014 | `new Set(generator)` is rejected: A generator is single-use, | yes | TS2769 | wrong | — |
| FORM_REJECTIONS/Array / Array used as a value | a075.ts | S014 | `Array used as a value` is rejected: Array is a compiler-own | no | accepts | yes | new |
| FORM_REJECTIONS/Array / Array used as a value | a075-b.ts | S014 | `Array used as a value` is rejected: Array is a compiler-own | no | TS2322 | no | — |
| FORM_REJECTIONS/Array / Array.from(source, mapFn) | a076.ts | S014 | `Array.from(source, mapFn)` is rejected: The mapper overload | yes | accepts | no | — |
| FORM_REJECTIONS/Array / Array.from(source, mapFn) | a076-b.ts | S014 | `Array.from(source, mapFn)` is rejected: The mapper overload | yes | TS2769 | wrong | — |
| FORM_REJECTIONS/Array / Array.from(Map) | a077.ts | S014 | `Array.from(map)` is rejected: TypeScript reads a Map elemen | yes | accepts | no | — |
| FORM_REJECTIONS/Array / Array.from(Map) | a077-b.ts | S014 | `Array.from(map)` is rejected: TypeScript reads a Map elemen | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/Array / Array.from(Generator<T>) | a078.ts | S014 | `Array.from(generator)` is rejected: A generator is single-u | yes | accepts | no | — |
| FORM_REJECTIONS/Array / Array.from(Generator<T>) | a078-b.ts | S014 | `Array.from(generator)` is rejected: A generator is single-u | yes | TS2322 | wrong | — |
| FORM_REJECTIONS/Array / isArray(value) | a079.ts | S014 | `Array.isArray` is rejected: A declared type answers this st | yes | accepts | no | — |
| FORM_REJECTIONS/Array / isArray(value) | a079-b.ts | S014 | `Array.isArray` is rejected: A declared type answers this st | yes | TS2554 | wrong | — |
| FORM_REJECTIONS/Array / of(value, …) | a080.ts | S014 | `Array.of` is rejected: Variable arity needs the variadic-pa | yes | accepts | no | — |
| FORM_REJECTIONS/Array / of(value, …) | a080-b.ts | S014 | `Array.of` is rejected: Variable arity needs the variadic-pa | yes | TS2345 | wrong | — |
| FORM_REJECTIONS/Array / new Array(length) | a081.ts | S014 | `new Array(length)` is rejected: The language has no array h | yes | accepts | no | — |
| FORM_REJECTIONS/Array / new Array(length) | a081-b.ts | S014 | `new Array(length)` is rejected: The language has no array h | yes | TS2769 | wrong | — |
| FORM_REJECTIONS/Object / groupBy | a082.ts | S014 | `Object.groupBy` is rejected: It returns a null-prototype ob | no | TS2550 | no | — |
| FORM_REJECTIONS/Set<K> / algebra(non-Set) | a083.ts | S014 | `Set.union non-Set argument` is rejected: The language has n | no | TS2345 | no | — |
| expr/aggregate.rs:290 check_array_spread_lit | s001.ts | S014 | array-literal spread produces a fresh T[]; it cannot constru | no | accepts | yes | new |
| expr/aggregate.rs:290 check_array_spread_lit | s001-b.ts | S014 | array-literal spread produces a fresh T[]; it cannot constru | no | TS2322 | no | — |
| expr/aggregate.rs:322 check_array_spread_lit | s002.ts | S014 | a bare `Map` is not an array-literal spread operand: this la | yes | accepts | no | — |
| expr/aggregate.rs:322 check_array_spread_lit | s002-b.ts | S014 | a bare `Map` is not an array-literal spread operand: this la | yes | TS2322 | wrong | — |
| expr/aggregate.rs:334 check_array_spread_lit | s003.ts | S014 | Generator<T> is single-use; array-literal spread would consu | yes | accepts | no | — |
| expr/aggregate.rs:334 check_array_spread_lit | s003-b.ts | S014 | Generator<T> is single-use; array-literal spread would consu | yes | TS2322 | wrong | — |
| expr/aggregate.rs:348 check_array_spread_lit | s004.ts | S014 | array-literal spread accepts T[], FixedArray<T, N>, Set, or  | no | TS2488 | no | — |
| expr/array_of_and_map_copy.rs:75 check_map_copy | s005.ts | S014 | type is not a permitted Map/Set key kind (Q24) | no | TS2769 | no | — |
| expr/array_of_and_map_copy.rs:75 check_map_copy | s005-c.ts | S014 | type is not a permitted Map/Set key kind (Q24) | no | accepts | yes | MapKeyKind |
| expr/call.rs:365 check_context_bytes_call | s006.ts | S014 | `Context.bytesOf<T>` takes exactly one type argument | no | accepts | yes | new |
| expr/call.rs:365 check_context_bytes_call | s006-c.ts | S014 | `Context.bytesOf<T>` takes exactly one type argument | no | TS2554 | no | — |
| expr/call.rs:373 check_context_bytes_call | s007.ts | S014 | `Context.bytesOf<T>` takes exactly one type argument | no | TS2558 | no | — |
| expr/call.rs:446 check_context_bytes_call | s008.ts | S014 | `Context.bytesOf` expects exactly 1 argument(s), got 0 | no | TS2554 | no | — |
| expr/call.rs:461 check_context_bytes_call | s009.ts | S014 | spread arguments require variadic parameters, which the lang | no | TS2556 | no | — |
| expr/call.rs:461 check_context_bytes_call | s009-b.ts | S014 | spread arguments require variadic parameters, which the lang | no | accepts | yes | VariadicArguments |
| expr/call.rs:1201 check_args_with_arguments | s010.ts | S014 | spread arguments require variadic parameters, which the lang | no | TS2556 | no | — |
| expr/call.rs:1201 check_args_with_arguments | s010-b.ts | S014 | spread arguments require variadic parameters, which the lang | no | accepts | yes | VariadicArguments |
| expr/call.rs:1243 check_set_source | s011.ts | S014 | spread arguments require variadic parameters, which the lang | yes | TS2769 | wrong | — |
| expr/call.rs:1243 check_set_source | s011-b.ts | S014 | spread arguments require variadic parameters, which the lang | yes | accepts | no | — |
| expr/call.rs:1273 check_set_source | s012.ts | S014 | `new Set(source)` accepts T[], FixedArray<T, N>, Set<T>, or  | no | TS2769 | no | — |
| expr/call.rs:1273 check_set_source | s012-c.ts | S014 | `new Set(source)` accepts T[], FixedArray<T, N>, Set<T>, or  | no | accepts | yes | new |
| expr/call.rs:1426 check_new | s013.ts | S014 | `i32[]` is not a permitted Map/Set key kind (Q24) | no | accepts | yes | MapKeyKind |
| expr/call.rs:1426 check_new | s013-c.ts | S014 | `i32[]` is not a permitted Map/Set key kind (Q24) | no | TS2322 | no | — |
| expr/literal.rs:64 check_lit | s014.ts | S014 | `RegExp.lastIndex` is not in the language: sticky matching r | yes | accepts | no | — |
| expr/literal.rs:64 check_lit | s014-b.ts | S014 | `RegExp.lastIndex` is not in the language: sticky matching r | yes | TS2322 | wrong | — |
| expr/literal.rs:467 check_ident | s015.ts | S014 | `Context` is an ambient namespace, not a value; use `Context | no | accepts | yes | new |
| expr/literal.rs:467 check_ident | s015-b.ts | S014 | `Context` is an ambient namespace, not a value; use `Context | no | TS2322 | no | — |
| expr/literal.rs:497 check_ident | s016.ts | S014 | `Number` is an ambient namespace, not a value or coercion; u | no | accepts | yes | new |
| expr/literal.rs:497 check_ident | s016-b.ts | S014 | `Number` is an ambient namespace, not a value or coercion; u | no | TS2322 | no | — |
| expr/literal.rs:505 check_ident | s017.ts | S014 | `JSON` is an ambient namespace, not a value; use `JSON.strin | no | accepts | yes | new |
| expr/literal.rs:505 check_ident | s017-b.ts | S014 | `JSON` is an ambient namespace, not a value; use `JSON.strin | no | TS2322 | no | — |
| expr/literal.rs:515 check_ident | s018.ts | S014 | `Date` is not a value; only `new Date(ms)`, `Date.UTC(…)`, a | no | accepts | yes | new |
| expr/literal.rs:515 check_ident | s018-b.ts | S014 | `Date` is not a value; only `new Date(ms)`, `Date.UTC(…)`, a | no | TS2322 | no | — |
| expr/literal.rs:523 check_ident | s019.ts | S014 | `Map` is a generic reference class, not a value; construct i | no | accepts | yes | new |
| expr/literal.rs:523 check_ident | s019-b.ts | S014 | `Map` is a generic reference class, not a value; construct i | no | TS2322 | no | — |
| expr/literal.rs:540 check_ident | s020.ts | S014 | `parseInt` may only be called, not read as a value (Q25) | no | accepts | yes | new |
| expr/literal.rs:540 check_ident | s020-b.ts | S014 | `parseInt` may only be called, not read as a value (Q25) | no | TS2322 | no | — |
| expr/literal.rs:547 check_ident | s021.ts | S014 | the coercing global `isFinite` is rejected; use `Number.isFi | no | accepts | yes | new |
| expr/literal.rs:547 check_ident | s021-b.ts | S014 | the coercing global `isFinite` is rejected; use `Number.isFi | no | TS2322 | no | — |
| expr/member.rs:545 member_on | s022.ts | S014 | `RegExp.lastIndex` is rejected: mutable global-match state w | yes | accepts | no | — |
| expr/member.rs:545 member_on | s022-b.ts | S014 | `RegExp.lastIndex` is rejected: mutable global-match state w | yes | TS2322 | wrong | — |
| expr/member.rs:600 member_on | s023.ts | S014 | `Date` is an immutable value; `getTime` cannot be assigned ( | no | accepts | yes | DateSubset |
| expr/member.rs:600 member_on | s023-b.ts | S014 | `Date` is an immutable value; `getTime` cannot be assigned ( | no | TS2322 | no | — |
| expr/member.rs:611 member_on | s024.ts | S014 | `getTime` may only be called, not read as a value (Q20) | no | accepts | yes | new |
| expr/member.rs:611 member_on | s024-b.ts | S014 | `getTime` may only be called, not read as a value (Q20) | no | TS2322 | no | — |
| expr/member.rs:627 member_on | s025.ts | S014 | numeric method `toFixed` may only appear in an accepted call | no | accepts | yes | new |
| expr/member.rs:627 member_on | s025-b.ts | S014 | numeric method `toFixed` may only appear in an accepted call | no | TS2322 | no | — |
| expr/method.rs:97 check_number_method | s026.ts | S014 | `toFixed` takes zero or one i32 digit count, got 2 argument( | no | TS2554 | no | — |
| expr/method.rs:302 check_array_method | s027.ts | S014 | `includes` is defined per element kind (scalars, strings, `D | no | accepts | yes | new |
| expr/method.rs:302 check_array_method | s027-b.ts | S014 | `includes` is defined per element kind (scalars, strings, `D | no | TS2345 | no | — |
| expr/method.rs:365 check_array_method | s028.ts | S014 | `join` formats elements by the Q14 interpolation rules; `i32 | no | accepts | yes | new |
| expr/method.rs:365 check_array_method | s028-b.ts | S014 | `join` formats elements by the Q14 interpolation rules; `i32 | no | TS2345 | no | — |
| expr/method.rs:597 check_array_method | s029.ts | S014 | spread arguments require variadic parameters, which the lang | no | TS2556 | no | — |
| expr/method.rs:597 check_array_method | s029-b.ts | S014 | spread arguments require variadic parameters, which the lang | no | accepts | yes | VariadicArguments |
| expr/method.rs:639 check_array_method | s030.ts | S014 | the `reduce` accumulator crosses the runtime↔script boundary | no | accepts | yes | new |
| expr/method.rs:639 check_array_method | s030-b.ts | S014 | the `reduce` accumulator crosses the runtime↔script boundary | no | TS2769 | no | — |
| expr/method.rs:754 check_array_method | s031.ts | S014 | `map` produces a `Box[]`; `Box` is outside the supported ele | no | accepts | yes | new |
| expr/method.rs:754 check_array_method | s031-b.ts | S014 | `map` produces a `Box[]`; `Box` is outside the supported ele | no | TS2322 | no | — |
| expr/method.rs:870 check_map_group_by | s032.ts | S014 | `Map.groupBy` callback returns `i32[]`, which is not a §10.2 | yes | accepts | no | — |
| expr/method.rs:870 check_map_group_by | s032-b.ts | S014 | `Map.groupBy` callback returns `i32[]`, which is not a §10.2 | yes | TS2322 | wrong | — |
| expr/method.rs:910 check_array_static_call | s033.ts | S014 | `Array.nope` is outside the accepted Array namespace (Q22) | no | TS2339 | no | — |
| expr/method.rs:910 check_array_static_call | s033-c.ts | S014 | `Array.toString` is outside the accepted Array namespace (Q2 | no | accepts | yes | new |
| expr/method.rs:948 check_array_from | s034.ts | S014 | `Array.from<T>` takes exactly one type argument | no | TS2554 | no | — |
| expr/method.rs:948 check_array_from | s034-d.ts | S014 | `Array.from<T>` takes exactly one type argument | no | accepts | yes | new |
| expr/method.rs:972 check_array_from | s035.ts | S014 | `Array.from(source)` takes one source argument, got 0 | no | TS2554 | no | — |
| expr/method.rs:985 check_array_from | s036.ts | S014 | spread arguments require variadic parameters, which the lang | yes | TS2556 | wrong | — |
| expr/method.rs:985 check_array_from | s036-b.ts | S014 | spread arguments require variadic parameters, which the lang | yes | accepts | no | — |
| expr/method.rs:1022 check_array_from | s037.ts | S014 | `Array.from(source)` accepts T[], FixedArray<T, N>, Set<T>,  | no | TS2769 | no | — |
| expr/method.rs:1022 check_array_from | s037-b.ts | S014 | `Array.from(source)` accepts T[], FixedArray<T, N>, Set<T>,  | no | accepts | yes | new |
| expr/method.rs:1470 callback_params_for_arity | s038.ts | S014 | `map` callbacks take 1 parameter(s), or 2 with a trailing `i | no | TS2345 | no | — |
| expr/method.rs:1470 callback_params_for_arity | s038-d.ts | S014 | `map` callbacks take 1 parameter(s), or 2 with a trailing `i | no | accepts | yes | new |
| expr/namespace.rs:69 check_namespace_member | s039.ts | S014 | `Context.collect` may only be called, not read as a value (Q | no | accepts | yes | new |
| expr/namespace.rs:69 check_namespace_member | s039-b.ts | S014 | `Context.nope` is outside the accepted Context subset (Q6/Q7 | no | TS2339 | no | — |
| expr/namespace.rs:94 check_namespace_member | s040.ts | S014 | `JSON.stringify` may only be called, not read as a value (Q2 | no | accepts | yes | new |
| expr/namespace.rs:94 check_namespace_member | s040-b.ts | S014 | `JSON.nope` is outside the accepted JSON subset (Q28) | no | TS2339 | no | — |
| expr/namespace.rs:108 check_namespace_member | s041.ts | S014 | `Array.from` may only be called, not read as a value (Q22) | no | accepts | yes | new |
| expr/namespace.rs:108 check_namespace_member | s041-b.ts | S014 | `Array.from` may only be called, not read as a value (Q22) | no | TS2322 | no | — |
| expr/namespace.rs:114 check_namespace_member | s042.ts | S014 | `Array.prototype` is outside the accepted Array namespace (Q | no | accepts | yes | new |
| expr/namespace.rs:114 check_namespace_member | s042-b.ts | S014 | `Array.nope` is outside the accepted Array namespace (Q22) | no | TS2339 | no | — |
| expr/namespace.rs:124 check_namespace_member | s043.ts | S014 | `Map.groupBy` may only be called, not read as a value (Q27) | no | accepts | yes | new |
| expr/namespace.rs:124 check_namespace_member | s043-b.ts | S014 | `Map.groupBy` may only be called, not read as a value (Q27) | no | TS2322 | no | — |
| expr/namespace.rs:131 check_namespace_member | s044.ts | S014 | `Set.prototype` is outside the accepted Map/Set subset; iter | no | accepts | yes | new |
| expr/namespace.rs:131 check_namespace_member | s044-b.ts | S014 | `Set.nope` is outside the accepted Map/Set subset; iterator- | no | TS2339 | no | — |
| expr/namespace.rs:316 check_math_member | s045.ts | S014 | `Math.PI` is read-only (Q19) | no | TS2540 | no | — |
| expr/namespace.rs:316 check_math_member | s045-b.ts | S014 | `Math.sin` is read-only (Q19) | no | accepts | yes | MathSubset |
| expr/namespace.rs:331 check_math_member | s046.ts | S014 | `Math.sin` may only be called, not read as a value | no | accepts | yes | MathSubset |
| expr/namespace.rs:331 check_math_member | s046-b.ts | S014 | `Math.sin` may only be called, not read as a value | no | TS2322 | no | — |
| expr/namespace.rs:338 check_math_member | s047.ts | S014 | `Math.nope` is outside the accepted Math subset (Q19) | no | TS2339 | no | — |
| expr/namespace.rs:338 check_math_member | s047-c.ts | S014 | `Math.toString` is outside the accepted Math subset (Q19) | no | accepts | yes | MathSubset |
| expr/namespace.rs:375 check_math_call | s048.ts | S014 | `Math.sin` takes exactly 1 f64 argument(s), got 0 (Q19: the  | no | TS2554 | no | — |
| expr/namespace.rs:375 check_math_call | s048-b.ts | S014 | `Math.max` takes exactly 2 f64 argument(s), got 1 (Q19: the  | no | accepts | yes | MathSubset |
| expr/namespace.rs:422 check_number_member | s049.ts | S014 | `Number.EPSILON` is read-only (Q25) | no | TS2540 | no | — |
| expr/namespace.rs:422 check_number_member | s049-b.ts | S014 | `Number.isFinite` is read-only (Q25) | no | accepts | yes | new |
| expr/namespace.rs:437 check_number_member | s050.ts | S014 | `Number.isFinite` may only be called, not read as a value (Q | no | accepts | yes | new |
| expr/namespace.rs:437 check_number_member | s050-b.ts | S014 | `Number.isFinite` may only be called, not read as a value (Q | no | TS2322 | no | — |
| expr/namespace.rs:444 check_number_member | s051.ts | S014 | `Number.prototype` is outside the accepted Number subset (Q2 | no | accepts | yes | new |
| expr/namespace.rs:444 check_number_member | s051-b.ts | S014 | `Number.nope` is outside the accepted Number subset (Q25) | no | TS2339 | no | — |
| expr/namespace.rs:462 check_number_predicate_call | s052.ts | S014 | `Number.isFinite` takes exactly 1 f64 argument, got 0 (Q25) | no | TS2554 | no | — |
| expr/namespace.rs:521 check_number_global_call | s053.ts | S014 | `parseFloat` takes exactly 1 argument(s), got 0 (Q25) | no | TS2554 | no | — |
| expr/namespace.rs:869 check_regex_method | s054.ts | S014 | `RegExp.exec` is rejected: its result needs an array with ex | yes | accepts | no | — |
| expr/namespace.rs:869 check_regex_method | s054-b.ts | S014 | `RegExp.exec` is rejected: its result needs an array with ex | yes | TS2345 | wrong | — |
| expr/namespace.rs:956 check_string_pattern_method | s055.ts | S014 | spread arguments require variadic parameters, which the lang | no | TS2556 | no | — |
| expr/namespace.rs:956 check_string_pattern_method | s055-b.ts | S014 | spread arguments require variadic parameters, which the lang | no | accepts | yes | VariadicArguments |
| expr/namespace.rs:1028 check_string_pattern_method | s056.ts | S014 | `string.search` requires a `RegExp`; string-pattern search i | no | accepts | yes | new |
| expr/namespace.rs:1028 check_string_pattern_method | s056-b.ts | S014 | `string.search` requires a `RegExp`; string-pattern search i | no | TS2769 | no | — |
| expr/namespace.rs:1078 check_date_member | s057.ts | S014 | `Date.now` is read-only (Q20) | no | accepts | yes | new |
| expr/namespace.rs:1078 check_date_member | s057-b.ts | S014 | `Date.now` is read-only (Q20) | no | TS2322 | no | — |
| expr/namespace.rs:1097 check_date_member | s058.ts | S014 | `Date.now` may only be called, not read as a value (Q20) | no | accepts | yes | new |
| expr/namespace.rs:1097 check_date_member | s058-c.ts | S014 | `Date.invalid` is outside the accepted Date subset (Q20) | no | TS2339 | no | — |
| expr/namespace.rs:1175 check_date_new | s059.ts | S014 | spread arguments require variadic parameters, which the lang | no | TS2556 | no | — |
| expr/namespace.rs:1175 check_date_new | s059-b.ts | S014 | spread arguments require variadic parameters, which the lang | no | accepts | yes | VariadicArguments |
| expr/namespace.rs:1274 date_subset_rejection | s060.ts | S014 | `nope` is outside the accepted Date subset (Q20) | yes | TS2339 | wrong | — |
| expr/namespace.rs:1274 date_subset_rejection | s060-b.ts | S014 | `hasOwnProperty` is outside the accepted Date subset (Q20) | yes | accepts | no | — |
| expr/operator.rs:45 check_unary | s061.ts | S014 | arithmetic on `f16` is not supported; compute via `as f32` | yes | accepts | no | — |
| expr/operator.rs:45 check_unary | s061-b.ts | S014 | arithmetic on `f16` is not supported; compute via `as f32` | yes | TS2322 | wrong | — |
| json.rs:55 check_json_call | s062.ts | S014 | `JSON.nope` is outside the accepted JSON subset (Q28) | no | TS2339 | no | — |
| json.rs:55 check_json_call | s062-c.ts | S014 | `JSON.toString` is outside the accepted JSON subset (Q28) | no | accepts | yes | new |
| json.rs:63 check_json_call | s063.ts | S014 | `JSON.stringify` expects exactly 1 argument, got 2 | no | accepts | yes | new |
| json.rs:63 check_json_call | s063-b.ts | S014 | `JSON.stringify` expects exactly 1 argument, got 0 | no | TS2554 | no | — |
| json.rs:75 check_json_call | s064.ts | S014 | spread arguments require variadic parameters, which the lang | no | TS2556 | no | — |
| json.rs:75 check_json_call | s064-b.ts | S014 | spread arguments require variadic parameters, which the lang | no | accepts | yes | VariadicArguments |
| json.rs:114 check_json_call | s065.ts | S014 | `JSON.stringify` cannot serialize `RegExp`; P13 accepts size | no | accepts | yes | new |
| json.rs:114 check_json_call | s065-b.ts | S014 | `JSON.stringify` cannot serialize `RegExp`; P13 accepts size | no | TS2322 | no | — |
| json.rs:130 check_json_call | s066.ts | unreachable | — | — | accepts | — | R3 |
| json.rs:157 check_json_parse | s067.ts | S014 | `JSON.parse` expects exactly 1 argument, got 0 | no | TS2554 | no | — |
| json.rs:169 check_json_parse | s068.ts | S014 | spread arguments require variadic parameters, which the lang | no | TS2556 | no | — |
| json.rs:169 check_json_parse | s068-b.ts | S014 | spread arguments require variadic parameters, which the lang | no | accepts | yes | VariadicArguments |
| json.rs:180 check_json_parse | s069.ts | S014 | `JSON.parse<T>` takes exactly one type argument | no | TS2558 | no | — |
| json.rs:196 check_json_parse | s070.ts | S014 | `JSON.parse` requires a target type; use `JSON.parse<T>(text | yes | accepts | no | — |
| json.rs:196 check_json_parse | s070-b.ts | S014 | `JSON.parse` requires a target type; use `JSON.parse<T>(text | yes | TS2769 | wrong | — |
| json.rs:251 check_json_parse | s071.ts | S014 | `JSON.parse` cannot deserialize `Map<i32, i32>`; Q28 accepts | no | accepts | yes | new |
| json.rs:251 check_json_parse | s071-b.ts | S014 | `JSON.parse` cannot deserialize `Map<i32, i32>`; Q28 accepts | no | TS2345 | no | — |
| json.rs:266 check_json_parse | s072.ts | unreachable | — | — | accepts | — | R4 |
| json.rs:293 reject_json_error_type | s073.ts | S014 | `JSON.stringify` cannot accept `Error`: the Error classes ar | yes | accepts | no | — |
| json.rs:293 reject_json_error_type | s073-b.ts | S014 | `JSON.stringify` cannot accept `Error`: the Error classes ar | yes | TS2322 | wrong | — |
| stmt.rs:1094 check_for_of_subject_under_flag | s074.ts | S014 | `entries()` yields a pair, but the language has no tuple typ | yes | accepts | no | — |
| stmt.rs:1094 check_for_of_subject_under_flag | s074-b.ts | S014 | `entries()` yields a pair, but the language has no tuple typ | yes | TS2554 | wrong | — |
| stmt.rs:1132 check_for_of_subject_under_flag | s075.ts | S014 | `keys()` is a subject-only fused view on Map, Set, or T[]; r | no | TS2339 | no | — |
| stmt.rs:1170 for_of_subject_from | s076.ts | S014 | a bare `Map` is not a `for…of` subject: this language binds  | yes | accepts | no | — |
| stmt.rs:1170 for_of_subject_from | s076-b.ts | S014 | a bare `Map` is not a `for…of` subject: this language binds  | yes | TS2322 | wrong | — |
| stmt.rs:1198 for_of_subject_from | s077.ts | S014 | `for…of` cannot make user class `Box` iterable (invariant 5) | no | TS2488 | no | — |
| stmt.rs:1208 for_of_subject_from | s078.ts | S014 | `for…of` accepts only T[], FixedArray<T, N>, Set, string, or | no | TS2488 | no | — |
| tyres.rs:274 resolve_type_ref | s079.ts | S014 | `RegExpMatchArray` is rejected: `groups` requires an object  | yes | accepts | no | — |
| tyres.rs:274 resolve_type_ref | s079-b.ts | S014 | `RegExpMatchArray` is rejected: `groups` requires an object  | yes | TS2322 | wrong | — |
| tyres.rs:466 resolve_type_ref | s080.ts | S014 | `i32[]` is not a Map/Set key kind; Q24 permits sized integer | yes | accepts | no | — |
| tyres.rs:466 resolve_type_ref | s080-b.ts | S014 | `i32[]` is not a Map/Set key kind; Q24 permits sized integer | yes | TS2322 | wrong | — |
| expr/operator.rs:249 check_update | s081.ts | S014 | arithmetic on `f16` is not supported; compute via `as f32` | yes | accepts | no | — |
| expr/operator.rs:249 check_update | s081-c.ts | S014 | arithmetic on `f16` is not supported; compute via `as f32` | yes | TS2322 | wrong | — |
| expr/operator.rs:1081 bin_result | s082.ts | S014 | arithmetic on `f16` is not supported; compute via `as f32` | yes | accepts | no | — |
| expr/operator.rs:1081 bin_result | s082-b.ts | S014 | arithmetic on `f16` is not supported; compute via `as f32` | yes | TS2322 | wrong | — |
| diag.rs:97 as_str | — | unreachable | — | — | — | — | adapter: no diagnostic constructor |
| diag.rs:130 explanation | — | unreachable | — | — | — | — | adapter: no diagnostic constructor |
| signatures.rs:563 resolve_param_pat | mirror-main.ts | S100 | parameter pattern outside the decided surface | no | accepts | yes | new |
| signatures.rs:563 resolve_param_pat | mirror-main-b.ts | S100 | parameter pattern outside the decided surface | no | TS2488 | no | — |

## Implementation

Red test at contract pin `647f2769`: all 86 measured targets fail in one report.
The test reuses all 315 measured programs and both mirrors. One tsc process costs 0.272 seconds.
The checker costs 0.049 seconds; the complete test costs 0.367 seconds.
No production change precedes this test. The shared tsc helper retains the corpus gate's options and binary lookup.

The implementation uses 79 rows, 82 S014 sites, and the mirror parameter site.
The four unread metadata rows are deleted. Their direct-site witnesses remain.
The witness table retains both TypeScript classes and all six old reject programs.
It has 313 programs: 315 minus eight deleted-row observations, plus six old corpus programs.
One tsc process also checks both mirrors, all 16 new TypeScript fragments, and the three retained rewritten reject entries.
The round-3 focused test costs 0.438 seconds: tsc 0.334 seconds, checker and render 0.071 seconds.
All 16 new subscript fragments pass the checker. The wrong-message control fires.
The source guard admits S014 construction only in the common checker module.
The witness index uses exhaustive matches for both closed enums. It compares independent expected variants.

Retained rewrites: `r27-string-match.ts`, `r78-call-spread-variadic.ts`, and `r201-new-class-spread-variadic.ts` state `tsc: accepts`.
Retired entries: `r76-return-keys-view.ts`, `r77-pass-keys-view.ts`, and `r198-set-source-map.ts`.
Their old witness programs match `0c75792b` byte-for-byte, except trailing whitespace. The accepted-class witnesses remain.
The Set row has no corpus reference. The corpus index no longer lists the three retired entries.
The corpus gates read the first diagnostic again. No expected diagnostic hides an earlier rejection.
First diagnostics: `r27` S014 at 9:25; `r78` S014 at 13:8; `r201` S014 at 17:34.
The `r27` message names the optional-index gap. Both spread messages state: `spread arguments require variadic parameters, which the language does not have`.

Five sites remove tsc outcome statements. The old and new texts follow.

- `string.match` row reason:
  Old: `RegExpMatchArray.index` is optional under stock `tsc --strict`, so the result cannot satisfy the language's `i32` index contract.
  New: The match result requires an optional numeric index, but the language requires a definite `i32` index.
- `new Set(Map)` row reason:
  Old: A Map yields a pair, so invariant 5 excludes it: stock `tsc` answers TS2769 for a Map source.
  New: A Map yields a key-value pair, but the language has no tuple type to represent that pair (invariant 5).
- `Array.from(Map)` row reason:
  Old: TypeScript reads a Map element as a `[K, V]` pair and this language reads `K`, so an accepted program fails the `tsc` gate (compiler.md §104.1).
  New: Map traversal binds `K`; a `[K, V]` pair has no tuple representation in the language (compiler.md §104.1).
- Array spread message:
  Old: a bare `Map` is not an array-literal spread operand: this language binds `K` and TypeScript binds a `[K, V]` pair, so an accepted program fails the `tsc` gate; push `map.keys()` or `map.values()` into the array with a `for…of` loop
  New: a bare `Map` is not an array-literal spread operand: Map traversal binds `K`; a `[K, V]` pair has no tuple representation in the language; push `map.keys()` or `map.values()` into the array with a `for…of` loop
- For-of message:
  Old: a bare `Map` is not a `for…of` subject: this language binds `K` and TypeScript binds a `[K, V]` pair, so an accepted program fails the `tsc` gate; iterate `map.keys()` or `map.values()`
  New: a bare `Map` is not a `for…of` subject: Map traversal binds `K`; a `[K, V]` pair has no tuple representation in the language; iterate `map.keys()` or `map.values()`

The message scan covers all checker files and ambient row summaries. The remaining outcome statements occur at sites without variants.
The witness messages and reason tests use the new text. Variant fragments and `why` records keep their accepted-class explanations.

New variants and reasons:

| Variant | why | collision |
|---|---|---|
| `CompilerOwnedValue` | Compiler-owned namespaces and methods lower to direct operations; the language has no value or writable storage for them. | stdlib.md §9.0 |
| `NamespaceObjectMember` | Compiler namespaces expose only declared intrinsics; JavaScript prototype members and inherited Object methods have no namespace representation. | stdlib.md §9.0 |
| `UnicodeNormalization` | Unicode normalization needs tables that the runtime does not provide. | stdlib.md §8 |
| `MatchOptionalIndex` | TypeScript makes the match index optional; the language requires a definite i32 index and has no optional numeric field. | stdlib.md §15.3 |
| `ArrayFlattenDepth` | A runtime flattening depth cannot determine one static result element type. | stdlib.md §9 |
| `MethodTypeDomain` | Each method has a fixed receiver, element, result, and accumulator domain; TypeScript generic method domains include more kinds. | stdlib.md §9 |
| `ArrayJoinDomain` | Array join uses the interpolation rules; nested arrays and other non-interpolatable elements have no implicit string form. | stdlib.md §9 |
| `FixedArraySpread` | Array spread creates a dynamic array; its runtime length cannot construct a FixedArray with a static length. | stdlib.md §14.4 |
| `ExplicitIntrinsicTypeArguments` | The intrinsic requires one explicit type argument for its storage or element type; inferred and mapper overloads do not supply that shape. | stdlib.md §18.1 |
| `SourceConstructionDomain` | Source construction accepts arrays, FixedArray, Set, and string; null and JavaScript array-like objects are outside this domain. | stdlib.md §14 |
| `CallbackParameterShape` | A container callback declares its element parameters and an optional index; omitted element parameters do not match the runtime callback ABI. | stdlib.md §12 |
| `JsonCallArguments` | JSON intrinsics take one argument; replacer, spacing, and reviver overloads are outside the declared interface. | stdlib.md §13 |
| `JsonTypeDomain` | JSON helpers require a supported static data shape; RegExp and container parse targets have no helper representation. | stdlib.md §13 |
| `StringSearchPattern` | String search requires a compiled RegExp; implicit conversion from a string pattern is outside the regular-expression interface. | stdlib.md §15.3 |
| `MirrorParameterPattern` | A mirror function uses named C ABI parameters; parameter destructuring requires a script body that a mirror does not provide. | compiler.md §107 |
| `LocaleNumberFormatting` | Locale-sensitive number formatting needs host locale data; the runtime provides only explicit locale-independent formats. | stdlib.md §11 |


Validation:

- The focused total test passes with 313 witnesses. Its single tsc process also accepts the three retained rewritten entries.
- Each of the six round-2 rewrites passed an individual tsc run before the three retirements.
- `cargo test --offline --locked -p subscript-compiler` passes: 914 passed, zero failed, one ignored.
- The final corpus run passes all 46 tests. The divergence-header gate reports no additional entry.
- The generic matrix uses the callback variant's `stdlib.md §12` record.
- `cargo fmt --check` passes.
- Clippy exits zero. The compiler library has two warnings; `tools/gate.sh` permits seven.
- The test targets have 13 distinct warnings. All 15 distinct warnings precede this change.
- The change added one identical-branch warning in `generic_tsc_matrix/api.rs`. The combined condition removes it.
- The API-reference generator runs with `--offline --locked`. `generated-docs/api-reference.md` and `generated-docs/corpus-index.md` change.
- No accept golden changes. No commit or `tools/gate.sh` run occurs.

Changed files:

- `compiler/src/ambient.rs`
- `compiler/src/ambient/rejection_id.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_mirrors.txt`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/support/tsc.rs`
- `compiler/tests/tsc_corpus.rs`
- Deleted: `corpus/reject/r198-set-source-map.ts`
- `corpus/reject/r201-new-class-spread-variadic.ts`
- `corpus/reject/r27-string-match.ts`
- Deleted: `corpus/reject/r76-return-keys-view.ts`
- Deleted: `corpus/reject/r77-pass-keys-view.ts`
- `corpus/reject/r78-call-spread-variadic.ts`
- `generated-docs/api-reference.md`
- `generated-docs/corpus-index.md`
- `specs/tracking/s153-rejection-tsc-class.md`

## Landing gate

```text
gate full 0c75792b9903a681766aa3f278e31c4792983b03 dirty:36 debug 2234/0/3 release 2231/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Round 4 implementation fixes

The total witness test passes with 319 witnesses: tsc 0.277 s, checker 0.057 s, total 0.385 s.
This run uses the working tree. It records no Phase Review result.

Five targets gain variants from accepted witnesses:

- `JsonParseCount`: `JSON.parse` with a reviver; `JsonCallArguments`.
- `ForOfUserClass`: a class with `Symbol.iterator`; `UserIterationProtocol`.
- `ArraySpreadSource`: spread of that iterable class; `UserIterationProtocol`.
- `NumberGlobalCount`: `Number.parseInt("1")`; `NumberCoercionAndArguments`.
- `FormAlgebraNonSet`: a class that extends `Set<i32>`; `SetAlgebraDomain`.

The corpus divergence gate reports r53 and r72. Both entries retire under acceptance 3.
Their old programs remain as rejected-class witnesses: `a083-old.ts` and `s077-corpus-old.ts`.
Both replacement probes pass tsc with the pinned configuration.

- r53's Set subclass first reports S100: "class inheritance is not in the decided surface".
- r72's iterable class first reports S100: "computed method names are not decided".

A structural Set implementation needs undeclared interfaces or computed members; a user iterator needs a computed method or inheritance.
Those declarations precede the target diagnostic. Neither accepted replacement preserves the purpose with the target diagnostic first.
The retired corpus rows and r53's ambient corpus link are deleted.

The witness index now requires a reason field for each no-variant entry.
Every no-variant target has this required reason:

- `DateLocalGetYear` (`a026.ts`): The ES2022 Date interface has no getYear member (TS2339).
- `FormNonCallbackTMethods` (`a069.ts`): The prelude FixedArray interface has no non-callback array methods (TS2339).
- `FormGroupBy` (`a082.ts`): The ES2022 ObjectConstructor interface has no groupBy member (TS2550).
- `ContextBytesTypeCount` (`s007.ts`): The prelude Context.bytesOf signature permits exactly one type argument (TS2558).
- `ContextBytesArgumentCount` (`s008.ts`): The prelude Context.bytesOf signature requires exactly one value argument (TS2554).
- `NumberMethodArgumentCount` (`s026.ts`): The ES2022 numeric formatting signatures permit at most one argument (TS2554).
- `ArrayFromArgumentCount` (`s035.ts`): The mapper guard takes two or three arguments; all other non-single counts violate Array.from overloads (TS2554).
- `NumberPredicateCount` (`s052.ts`): The ES2022 Number predicate signatures require exactly one argument (TS2554).
- `JsonStringifyHelper` (`s066.ts`): The json_serializable guard and collect_json_types graph closure admit only serializer helper types.
- `JsonParseTypeCount` (`s069.ts`): The prelude JSON.parse overload permits zero or one type argument (TS2558).
- `JsonParseHelper` (`s072.ts`): The serializable, Error, and Date guards plus graph closure admit only parser helper types.
- `ForOfKeys` (`s075.ts`): The fused receiver guard leaves only FixedArray here; its prelude interface has no keys or values member (TS2339).
- `ForOfSubject` (`s078.ts`): The resolved-type guard accepts containers and routes classes elsewhere; remaining scalar or nullable subjects lack a non-null iteration protocol (TS2488/TS18047).

The S014 text scan finds one outcome claim in a production message and its witness:

- Old: "`for…of` cannot make user class `Box` iterable (invariant 5): that requires `Symbol.iterator`, and `Symbol` is a permanent non-goal; stock `tsc` rejects this subject too".
- New: "`for…of` cannot make user class `Box` iterable (invariant 5): that requires `Symbol.iterator`, and `Symbol` is a permanent non-goal".

No API row summary contains a tsc outcome claim.
General error, resolution_error, and error_diverging paths assert that the code is not S014.
The source guard remains; it reports explicit S014 references outside the named constructor and checks the exhaustive site list.
The duplicate tsc corpus block for r27, r78, and r201 is deleted.
The no-variant probes use a Date subclass, readonly arrays, a mapper overload, omitted arguments, and extra generic arguments.
The JSON probes use recursive nullable class fields. Both pass the checker and never reach the helper failure guards.
The iterable probe uses an interface and `Iterable<i32>`; tsc accepts it, but the checker reports S100 and S016 before traversal.
The accepted numeric-format and mapper probes reach other rows that already carry variants.
The invalid count probes report TS2554 or TS2558. Object.groupBy still reports TS2550 under the pinned ES2022 library.
No probe reaches another no-variant target with tsc acceptance.

Firing controls construct a wrong variant, an absent block, both false TypeScript classes, and both empty no-variant reasons.
The TypeScript controls join the existing single tsc process; they add no duplicate corpus check.
The reason field is required at compile time. An empty string remains possible and both entry kinds reject it at runtime.

Validation:

- `cargo test --offline --locked -p subscript-compiler`: 917 passed, zero failed, one ignored.
- The corpus suite passes all 46 tests; the divergence-header gate reports no remaining entry.
- `cargo fmt --check` and `tools/hygiene.sh` pass.
- All changed files have fewer than 2,000 lines.
- Clippy exits zero; the compiler library has two warnings against the gate baseline of seven.
- No new warning remains against the recorded round-2 clippy log.
- The API-reference generator changes `api-reference.md` and `corpus-index.md`; other generated references remain byte-identical.
- No accept golden changes. No commit or `tools/gate.sh` run occurs.

Changed files for this implementation round:

- `compiler/src/ambient.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/stmt.rs`
- `compiler/src/divergence.rs`
- `compiler/tests/corpus_reject.rs`
- Deleted: `corpus/reject/r53-set-algebra-nonset.ts`
- Deleted: `corpus/reject/r72-for-of-user-class.ts`
- `generated-docs/api-reference.md`
- `generated-docs/corpus-index.md`
- `specs/tracking/s153-rejection-tsc-class.md`

Fix-round gate:

```text
gate full 376fcdf7ac150fe4407be12a3704e93e8fe3bdc2 dirty:14 debug 2237/0/3 release 2234/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
