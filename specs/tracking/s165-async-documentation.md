# §165 Async documentation — evidence

Contract: `specs/blocks/compiler/s165-the-async-documentation-states-the-contract.md`.
Date: 2026-10-06. Host: arm64 macOS. Pin: `c9aae042`.

## Documentation

| File and site | Replaced text | Contracted behaviour |
|---|---|---|
| `docs/tutorial-typescript.md`, async part | “A failed `await` returns a value; it does not throw.” | An await raises the exception of a failed handle (§116.1 rule 2). |
| `docs/tutorial-typescript.md`, exception part | “`finally` is rejected, and so is a `try` block that holds `await` or `yield`.” | A try block can hold await or yield (§116.1 rule 7). Finally stays rejected. |
| `docs/tutorial-c-cpp.md`, part 5 | “every pending async entry once, in start order” | A checkpoint appends existing parked frames after ready jobs and drains the ready queue (§94.1 rules 8 and 9). |
| `docs/tutorial-c-cpp.md`, part 5 | “promises are not storable values, so there is nothing to collect” | Locals, arrays, and fields can hold handles. Copies retain frames; releases free frames when the count reaches zero (§70.3). |
| `docs/tutorial-c-cpp.md`, host notes | “every pending entry once (start order, deterministic)” | Jobs added during the drain run in that checkpoint. Newly parked frames wait for the next checkpoint (§94.1 rules 8 and 9). |
| `docs/tutorial-rust.md`, async host note | “polls each root pending at entry once” | The step drains ready jobs and returns the count of ready jobs plus parked frames (§94.1–94.2). |

The TypeScript tutorial includes two new programs with adjacent output fences.
One catches an exception at await. The other awaits one failed handle twice and prints the message at each raise.
The tutorial cites `corpus/trap/t66-unobserved-async-exception.ts` for the trap on the release of an unobserved failed handle.
Q34 states §116.1 rules 1–4 in one paragraph.
The documentation test expects 20 TypeScript fences, instead of 18.

## CLI measurements

The CLI binary comes from `cargo build --offline --locked -p subscript-cli --bin subscript` at the pin and current tree.
Each program uses a separate copy under `$TMPDIR` before `subscript run` executes it.

| Program | Exit | Measured output |
|---|---|---|
| New caught-await fence | 0 | `caught boom` |
| New repeated-await fence | 0 | `after call`, `first boom`, `second boom` |
| Tutorial ready-chain fence | 0 | `A:start`, `A:leaf`, `B:start`, `B:leaf`, `main:mid`, `A:end`, `B:end`, `a=2 b=2` |
| `a259-try-holds-yield.ts` | 0 | The output equals its `.expected` file. Both generator catch blocks run after a resume. |
| `a161-counted-handle-stores.ts` | 0 | `3`, `5`, `7`, `1`, `2`, `24`; the output equals its `.expected` file. |
| `t66-unobserved-async-exception.ts` | 1 | `fails:start held`, `main:created`; then trap `[uncaught-exception]`: `Error: never observed held` at `fails`, line 9, column 3. |

The two new fences and the ready-chain fence match their text output byte for byte.
The t66 program does not print `unreached`.
The host checkpoint and frame lifetime descriptions cite §94.1–94.2 and §70.3.

## Search

This search includes every table text and permits line breaks between words:

```sh
rg -U -n \
  -e 'A failed\s+`await`\s+returns a value;\s+it does not throw\.' \
  -e '`finally` is rejected, and so is a\s+`try` block that\s+holds\s+`await` or `yield`' \
  -e 'every pending async entry once,\s+in start order' \
  -e 'promises are not storable values,\s+so\s+there is nothing to collect' \
  -e 'every pending entry once\s+\(start order, deterministic\)' \
  -e 'polls each root pending at entry once' \
  README.md llms.txt docs generated-docs runtime/include
```

Result: no matches; exit 1.

## Acceptance measurements

| Command or comparison | Result |
|---|---|
| `cargo test --offline --locked -p subscript-codegen --test docs -- --nocapture` | 8 passed; 0 failed. The TypeScript tutorial has 18 programs and 18 compared outputs. Both new programs execute. |
| `cargo test --offline --locked -p subscript-compiler language_reference` | 2 language-reference tests passed; 0 failed. |
| `cargo fmt --check` | Exit 0. |
| `cargo run --offline -p subscript-compiler --bin generate-api-reference` | Exit 0. All generated files match their SHA-256 values from before this regeneration. |
| Generated files against HEAD | Only `generated-docs/language-reference.md` differs. |
| `.expected` files against HEAD | No changes. |

Compiler, runtime, and corpus behaviour stays unchanged.

## Phase Review

1. §165.3 item 1: “suspended async roots” and “explicit polling control” → `codegen/src/reload.rs`, `ReloadSession::async_pending`: ready jobs plus parked registrations (§94.2).
2. §165.3 item 2: “Context's deterministic pending queue” → `runtime/src/context.rs`, `Context::async_kick`: parked list, child waiter list, or ready queue (§94.1 rules 3, 4, 6, 7).
3. §165.3 item 3: no text about an export without a holder → `docs/tutorial-typescript.md`, async part, and `compiler/src/language_reference.rs`, Q34: the exception traps (§116.1 rule 5).
4. §165.3 item 4: “A handle lives in a local or an array” and “Handles may be stored in locals and arrays” → the tutorial async part and Q34: locals, arrays, fields, globals (§70.3 rule 2a).
5. §165.3 item 5: “kinds 1 through 27 at this commit” → `docs/tutorial-c-cpp.md`, Step 6: a link to the `TrapKind` list in `runtime/src/trap.rs` (§165.3 item 5).

## Round 3 measurements

The CLI binary comes from `cargo build --offline --locked -p subscript-cli --bin subscript` in the current tree.
Each program uses a separate copy under `$TMPDIR` before `subscript run` executes it.

| Program | Exit | Measured result |
|---|---|---|
| Host-callable async export that throws before an await | 1 | `[uncaught-exception]: Error: export boom`, line 1, column 47. |
| Host-callable async export that throws after `await Context.suspend()` | 1 | `[uncaught-exception]: Error: export boom`, line 1, column 72. |
| `a161-counted-handle-stores.ts` | 0 | Output equals its `.expected` file; global, field, array, and local holders preserve the handle. |
| `a188-async-nested-settled-chains.ts` | 0 | Output equals its `.expected` file; the host checkpoint runs the continuations. |
| `a93-async-chain.ts` | 0 | Output equals its `.expected` file; explicit suspensions preserve the call chain. |

The export probes use `export async function main(): Promise<void>` with `throw new Error("export boom");` in its body.
The second probe inserts `await Context.suspend();` before the throw.
§94.2 defines the pending count; §94.1 rules 3, 4, 6, and 7 define the suspension destinations.
The CLI does not expose those host counters or queue lists to a script.
`runtime/src/trap.rs` supplies the trap list; no new claim fixes its count.
No new TypeScript fence changes the count of 20 in `codegen/tests/docs.rs`.

| Command or comparison | Result |
|---|---|
| `cargo test --offline --locked -p subscript-codegen --test docs` | 8 passed; 0 failed. |
| `cargo fmt --check` | Exit 0. |
| `cargo test --offline --locked -p subscript-compiler language_reference` | 2 language-reference tests passed; 0 failed. |
| `cargo build --offline --locked --workspace --all-targets` | Exit 0. |
| `cargo run --offline -p subscript-compiler --bin generate-api-reference` | Exit 0; a second regeneration preserves all generated file SHA-256 values. |
| `sh tools/hygiene.sh` | Exit 0. |
| `git diff --check` | Exit 0. |
| Generated files against HEAD | Only `generated-docs/language-reference.md` differs. |
| `.expected` files against HEAD | No changes. |

The stale-text search uses the listed replacement sites and permits a line break in the trap-count text:

```sh
rg -U -n \
  -e 'suspended async roots' \
  -e 'explicit polling control' \
  -e "Context's deterministic pending queue" \
  -e 'A handle lives in a local or an array' \
  -e 'Handles may be stored in locals and arrays' \
  -e 'kinds 1 through 27 at\s+this commit' \
  codegen/src/reload.rs runtime/src/context.rs \
  docs/tutorial-typescript.md docs/tutorial-c-cpp.md \
  compiler/src/language_reference.rs generated-docs/language-reference.md
```

Result: no matches; exit 1.
The full gate at `27383528` reports debug 2371/0/3, release 2368/0/3, goldens-moved 0, exit 0.
