# §317 conditional discharge

## Status

The checker implements the amended rules of §317.
Each value keeps a kind for each origin: made in an operand, or selected from a binding.
A selected origin also keeps whether every operand carries it.
A read through a local alias or a pattern keeps this fact.
A compiler temporary keeps the kind of its source operand.
The check does not need a path condition.

A parameter default cannot discharge an earlier parameter.
Other origins made in a default keep the discharge of §257.
The parameter keeps its own obligation.

Five new S013 sites cover a default that reads a parameter, a conditional transfer through a local or parameter, and a conditional return through a local or parameter.
Each site is `Diverges` and has an example of its own form.
The conditional parameter-return alternative takes one handle and returns it, so it also fits synchronous bodies.
The other conditional-transfer alternatives create or take one handle and store it without converting it to a value.
Their explanations apply to await, argument, store, Promise.all, and then transfers.
The sites keep the `DroppedAsyncHandle` record id.
The total checks of §154, §307, §309, §311, and §312 pass.

## Red measurements

Contract pin: `83812ae9`, which contains the amended rule 1.
The pin CLI was built from a source archive outside the repository, with a separate target directory.
No working-tree file was replaced.
TypeScript version: 5.9.2, with the project prelude and strict project options.

| Reject entry | Contract program | Pin CLI `check` | Measured `tsc` | Changed CLI `check` |
|---|---|---|---|---|
| `r533-default-reads-earlier-handle` | `f` | Exit 0; no errors | Exit 0; no errors | Exit 1; S013 at the earlier parameter |
| `r534-conditional-handle-binding` | `g` | Exit 0; no errors | Exit 0; no errors | Exit 1; S013 at both calls |
| `r535-conditional-handle-value` | `h` | Exit 0; no errors | Exit 0; no errors | Exit 1; S013 at both parameters |

Each entry copies its contract program.
The helper `v(n)` returns `n` from an async function.
The pin accepts each entry that must receive S013.
The separate TypeScript check of these three files passes.

## Accepted forms and test cost

`a465-conditional-handle-discharge` runs both conditions.
It awaits both existing handles, awaits a handle made in each selected operand, and awaits a conditional that selects the same origin twice.
It also covers a local alias, a pattern, and a call with a made default.
The call with a default runs with both an omitted and an explicit second argument.
The development tier, ship tier, interpreter, and node give the 16 lines of its output golden.

The two new checker tests run 50 and 16 checks, with no lowering or child process.
With one test thread, they cost 76 ms and 13 ms.
They cover aliases, patterns, nested conditionals, made call and await results, and each discharge form.
They also cover defaults of a function, method, static method, constructor, lambda, and generator.
The existing runtime-case test of §188 rule 5 passes.
`apparent_type_shapes` passes.

## Corpus comparison

The CLI ran `check` on all 1,061 entries in `corpus/accept`, `corpus/reject`, `corpus/warn`, and `corpus/trap` at the pin and after the change.
The count includes `a465` and the three new reject entries.
Each directory entry used `main.ts`.
Each check used the entry's selected interop mirrors and enabled modules.
The comparison included exit status, standard output, and diagnostics.

Before the corpus repair, the pin accepted 573 entries and the changed checker accepted 569.
The two runs cost 8.742 s and 8.833 s.
These four entries changed from accepted to rejected:

| Entry | Reason |
|---|---|
| `a423-async-return-adopts-a-handle` | `chooseHeld` returns one of two supplied handles. No other path discharges the other handle. Both parameters receive S013. |
| `r533-default-reads-earlier-handle` | The default reads `p`, but a call can supply `q`. The default cannot discharge `p`. |
| `r534-conditional-handle-binding` | `r` selects either `p` or `q`. Awaiting `r` cannot discharge both existing call origins. |
| `r535-conditional-handle-value` | The conditional selects one of two supplied handles. Its await cannot discharge both parameter origins. |

The repair of `a423` awaits both parameters before it returns the selected handle.
Its output golden does not change.
The existing conditional-return checker test now awaits both parameters too.
Its conditional return still tests the result context.
The final comparison checks all 1,061 entries again after this repair.
The pin accepts 573 entries; the changed checker accepts 570.
The runs cost 8.790 s and 8.873 s.
Only `r533`, `r534`, and `r535` differ in exit status or output.
The repaired `a423` is accepted by both CLIs.
All other final corpus results are byte-identical.

Only two LIR text blocks change: the repaired `a423` function and the new `a465` entry.
No existing output golden changes.
The document generator updates the corpus index and the language reference.

## Required verification results

1. `node_modules/.bin/tsc -p tsconfig.json`: PASS, exit 0, 0.683 s; no errors.
2. `cargo test -p subscript-compiler --tests`: PASS, exit 0, 117.959 s; 1,675 tests pass; one existing wall-time measurement is ignored.
3. `cargo test -p subscript-compiler --lib`: PASS, exit 0, 3.197 s; 685 tests pass.
4. `cargo test -p subscript-codegen --tests`: PASS, exit 0, 323.535 s; 1,045 tests pass; one existing release scaling measurement is ignored.
5. `cargo test -p subscript-cli --tests`: PASS, exit 0, 43.406 s; 139 tests pass.
6. `cargo clippy --offline --locked --workspace --all-targets`: PASS, exit 0, 12.391 s; no new warnings; the pin and changed tree have the same warning messages; library baselines remain compiler 4, runtime 10, codegen 13.
7. `cargo fmt --check`: PASS, exit 0, 2.161 s; no formatting differences.

The Clippy baseline was measured with the same command in the separate pin build.
No warning message was added or removed.
The codegen corpus tests verify development, ship, and interpreter output against the goldens.
The separate node run of `a465` also matches its golden byte for byte.
The generated LIR blocks were compared before replacement; no other block moved.
The source review found no new library panic and no changed Rust file above 2,000 lines.

`tools/hygiene.sh`: PASS, exit 0.
`git diff --check`: PASS.

## Work state

No commit or staging operation occurred.
No standalone repository gate was started.
The required CLI test suite exercised the gate script with isolated command fixtures.
No separate Phase Review ran in this session.
All shell jobs started for this task have finished.
No background job remains.

## §317-3 example review repair

`AsyncConditionalReturnParameter` now explains: "Returning a conditional value transfers only the selected handle. Take one handle and return it, or transfer both supplied handles separately."
Its alternative is `function f(p: Promise<i32>): Promise<i32> { return p; }`.
This takes one handle, as allowed by the review, and requires no await in a synchronous function, arrow, or method.

`AsyncConditionalBinding` now explains: "A conditional value transfers only the selected handle. Create one handle and transfer it, or transfer both existing handles separately."
Its alternative is:
```ts
async function v(n: i32): Promise<i32> { return n; }
let stored: Promise<i32>[] = []; function g(): void { const p = v(1); stored = [p]; }
```

`AsyncConditionalParameter` now explains: "A conditional value transfers only the selected handle. Take one handle and transfer it, or transfer both supplied handles separately."
Its alternative is:
```ts
let stored: Promise<i32>[] = []; function h(p: Promise<i32>): void { stored = [p]; }
```

The one-handle repair applies to every discharge form without converting handles into values.
The new regression test checks rejected conditional transfers and accepted one-handle alternatives for await, arguments, Promise.all, then, local stores, and field stores, with both binding and parameter origins.
It also checks synchronous function, arrow, and method returns and async function returns.
The examples remain on the existing sites, with unchanged record ids.

Review repair verification (all exit 0):

- `cargo test -p subscript-compiler --tests`: PASS; 1,675 passed, one existing ignored measurement.
- `cargo test -p subscript-compiler --test conditional_async_discharge`: PASS; all three tests, including the subsequently added regression matrix.
- `cargo test -p subscript-compiler --lib`: PASS; 685 passed, including the total checks of §154, §307, §309, §311, and §312 and their TypeScript fragment/alternative validation.
- `node_modules/.bin/tsc -p tsconfig.json`: PASS.
- `cargo clippy --offline --locked --workspace --all-targets`: PASS; library warning baseline remains compiler 4, runtime 10, codegen 13. Repeated after the regression test correction; PASS.
- `cargo fmt --check`: PASS.
- `git diff --check`: PASS.

Only the example source, the conditional-discharge test, and this tracking note were edited for this review repair. No staging, commit, or standalone gate invocation occurred.
