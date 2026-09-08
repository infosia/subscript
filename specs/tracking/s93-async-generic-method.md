# §93 — an async method declares type parameters

Contract: `specs/blocks/compiler.md` §93. Pin for every measurement
below: `61c64c36ec1b0bd678a9e6a579140be77c2f120d`. Host: aarch64
macOS; rustc 1.95.0; Apple clang 21.0.0 (clang-2100.1.1.101); tsc
5.9.2; node v24.18.0.

## Why the section exists

§82.4 accepts `recv.m<A>(...)`. §64 accepts `await f<A>(...)` and
`await recv.m(...)`. Only the combination of the two accepted
features is rejected, and `tsc` accepts it. §82.4 rule 5 gave the
reason as "the await grammar gains no form", which states a fact
about the checker, not about the language. The fix is mechanical: the
sync call path already instantiates a generic method at the call, and
the await path lacks that one step.

## Measured at the pin

Each program below is in the §93 measurement list. The command is
`subscript run <file>`; every one exits 1.

1. An async method with type parameters on a non-generic reference
   class: S100 "async generic methods are not in the decided surface"
   at the declaration, with a divergence block.
2. The rejection is at collection. A call without type arguments, a
   floating call, and a read of the method as a value each report
   that same declaration diagnostic, at the declaration line.
3. On a generic class: S100 "generic classes cannot declare generic
   methods".
4. `static async m<T>()`: S100 "async static methods are not in the
   decided surface".
5. On a `@CStruct` value class: S100 "async generic methods are not
   in the decided surface". The value-class rejection sits after the
   generic-method branch in `collect_class`, so the branch masks it.
6. Bodiless, in a `declare class` written in a `.ts` source: the same
   masked result, where §82.4 rule 1a states "function bodies are
   required".

Items 5 and 6 are message defects. Each program is rejected today,
and each one names the wrong rule.

## Red, at the pin

The three corpus sources of §93.3 were run at the pin from a scratch
directory. Every one exits 1 with the item-1 diagnostic at the method
declaration:

| Entry | Line reported | Line the section requires |
|---|---|---|
| `a187-async-generic-method` | 20, the declaration | none; the entry must run |
| `r186-async-generic-method-without-type-args` | 8, the declaration | the call |
| `r187-async-generic-method-on-value-class` | 12, the declaration | the declaration, other rule |

`tsc -p` over the three sources with the repository prelude and the
repository `tsconfig.json` options exits 0. The `tsc: accepts` header
of each entry is measured, not assumed.

## Correction found while writing the section

The `compiler.md` §0 section index stopped at §89. §90, §91, and §92
were absent. The rows are added with §93's. No check reads the index,
so nothing reported the gap.
