# A JSON mismatch names its path — `compiler.md` §194

Status: landed.

## Measurement at the pin `3231e6d7`

The probe is one `Person` class (`name: string`, `age: u8`,
`friend: Person | null`) and four documents. The probe ran on the dev
tier with `target/debug/subscript run`, built from the pin.

| Case | Document | Output |
|---|---|---|
| nested range | `{"name":"a","age":1,"friend":{"name":"b","age":300,"friend":null}}` | `JSON.parse: document does not match Person` |
| nested kind | `{"name":"a","age":1,"friend":5}` | `JSON.parse: document does not match Person` |
| nested missing | `{"name":"a","age":1,"friend":{"name":"b","friend":null}}` | `JSON.parse: document does not match Person` |
| root kind | `{"name":7,"age":1,"friend":null}` | `JSON.parse: document does not match Person` |

The four failures give one message. The committed golden of
`corpus/accept/a255-json-parse-direct` shows the same fact for
`missing field` and `wrong kind`.

## Prior art

scriptc reports a checked-cast mismatch with a path and the two kinds:
`expected number at $.port, got string` *(docs:
<https://github.com/vercel-labs/scriptc>, the "Runtime type checks"
limitation page)*. No scriptc binary was run for this note.

## Landing

Red at the pin: a binary built from a clean checkout of `3231e6d7`
gives 10 different lines on `a255` and 7 on `a372`. After the landing,
both entries agree on the dev JIT, the ship C, the interpreter, and the
golden.

Success-path cost, ship tier, MSVC: a loop of 200,000
`JSON.parse<Config>` calls (`name: string`, `count: i32`), 3 warm-up
runs, then 11 runs of each binary in turn.

| Binary | Median |
|---|---|
| pin | 109.411 ms |
| landing | 111.165 ms |

The change is +1.60%. The stop limit of §194.5 is 5%.

Cost of the new unit tests, as the process median: the record classes
and segments 6.163 ms, the nullable relabel 5.821 ms, the `JsonFn`
table 5.311 ms. The compiler test that walks the validator HIR takes
0.01 s in the debug test profile.

The Phase Review found a runtime test that copied the validator checks
and could not fail. The compiler HIR walk replaced it. A local mutation
moved one `ParseRecord` onto the success path, and the walk failed.

## Phase Review at `77b3043b`

A fresh review of `d9d07f74..77b3043b`: 0 CRITICAL, 0 MAJOR, 3 MINOR.
All three are realistic.

1. Rule 5 gave no order between `a non-integer number` and `a number
   out of range`. `JSON.parse<i32>('12345678901234567890.5')` gives
   `a non-integer number` on the dev tier. Closed: §194.1 rule 5 now
   states this order. No code or golden moves.
2. In a generic function, `<T>` is the source spelling and `<U>` is
   the instantiated type: `document does not match T at $.y: expected
   i32, found string`. This follows rule 1. Open as an observation; no
   change.
3. This note did not give the cost of
   `begin_clears_the_previous_mismatch`. Closed: 6.863 ms, process
   median of 11 runs, debug, arm64 macOS.

Gates on arm64 macOS at `77b3043b`:

```text
gate full 77b3043bfb32b7521212cd3e27841014415bcd72 clean debug 2866/0/5 release 2863/0/5 skips 2/0 clippy 5/18/11 goldens-moved 0 exit 0
```

`tools/hygiene.sh` exits 0.
