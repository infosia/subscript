# §115 Recoverable exceptions, and `JSON.parse` returns `T` — evidence

Contract: `specs/blocks/compiler/s115-recoverable-exceptions-and-a-direct-json-parse.md`.
Date: 2026-09-26. Host: arm64 macOS. Pin: `cb1d469` (contract commit
follows it).

## Owner decisions (2026-09-26)

- The reversal of C6 is accepted.
- A dispose hook that raises while an exception propagates traps.
- An exception that leaves an async body, a generator body, or a Worker
  entry traps; a `try` block that holds a suspension is rejected.

## `tsc` measurements at the pin

`tsc` 5.9.2, the repository `tsconfig.json`, the prelude copied to a
temporary directory:

| Source | Result |
|---|---|
| `catch (e) { const n: i32 = e; }` | TS2322 (`e` is `unknown`) |
| `catch (e: Error) { }` | TS1196 |
| `e instanceof SyntaxError`, `e.message`, `e.name`, `throw e` | clean |
| `catch { }`, `try { } finally { }` | clean |
| `throw 42`, `class MyErr extends Error {}` | clean |
| prelude `parse<T>(text: string): T`; `const c: Config = JSON.parse(text)` | clean, `T = Config` |
| same prelude; `const u = JSON.parse(text); const n: i32 = u;` | TS2322 (`unknown`) |

## Success-path baseline at the pin (§115.12 criterion 4)

`cross-language --only <id>`, release build at `dd2e744`, two runs.
Median of 11 timed runs each; milliseconds.

| Workload | ship run 1 | ship run 2 | jit run 1 | jit run 2 |
|---|---|---|---|---|
| fib-recursive | 3.685 | 3.715 | 7.922 | 7.952 |
| callbacks | 37.485 | 37.524 | 460.480 | 459.353 |
| tree | 132.283 | 131.778 | 420.931 | 412.537 |

The two runs agree within 1% on the ship tier, so the 5% kill threshold
is above the noise.

JSON loop: 200,000 `JSON.parse<Config>` calls on a four-field document
(a string, an `i32`, an `f64`, an `i32[]`), with `JsonResult` and
`Context.free`, ship tier, whole process under `/usr/bin/time -p`,
11 runs: median 0.32 s.
