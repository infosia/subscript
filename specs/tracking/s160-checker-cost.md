# §160 checker cost

## Method

The contract pin is `1af40676`. The comparison pin is `40b0cbff`.
The compiler uses Rust 1.95.0 on an arm64 macOS host.
All measured CLI builds use the release profile and the locked offline dependencies.
Each table entry gives the fastest of three `subscript check` runs.
Builds do not overlap the final measurement runs.

Generate the benchmark source files with this command:

```sh
python3 compiler/tests/fixtures/s160_gen.py "$TMPDIR"
```

The six original files match the contract generator.
Each original size has an annotated form and an inferred form.
The generator also produces annotated-field and long-function local-assignment forms.
The comparison pin rejects the inferred form, so it has no inferred timing.

## Costs

Temporary timing counters measure the named operations in release builds.
The counters are removed from the final sources.
Inclusive counters overlap; their times must not be added.
The clone counters exclude destruction of the copied tables.
Percentages use the elapsed time of the same counter run.
Counter run D overlaps the verification tests. The final timing runs do not overlap tests or builds.

| Counter run, `n = 3,000` | Annotated elapsed | Inferred elapsed |
|---|---:|---:|
| A: decisions and rejection facts | 36.160 s | 40.567 s |
| B: initializer effects and table copies | 36.705 s | 43.350 s |
| D: class labels and diagnostic positions | 90.538 s | 77.410 s |

| Cost | Run | Annotated cost / share | Inferred cost / share | Contrived form only? |
|---|---|---:|---:|---|
| Whole initializer effect pass, inclusive | B | 35.520 s / 96.77% | 41.745 s / 96.30% | No; normal initialization routes |
| Whole default table copy per scanner, 42,004 calls | B | 16.282 s / 44.36% | 20.486 s / 47.26% | No; ordinary default parameters |
| Whole default table copy per lambda, 6,000 calls | B | 2.128 s / 5.80% | 2.274 s / 5.25% | No; ordinary callbacks |
| Unimplemented overload scan, 6,002 calls | A | 0.120 s / 0.33% | 0.118 s / 0.29% | No; an honest overload error |
| Class member label scan, 18,000 calls | D | 0.403 s / 0.45% | 0.300 s / 0.39% | No; same-named classes from separate modules |
| Diagnostic declaration positions, one call | D | 0.001363 s / 0.0015% | 0.000541 s / 0.0007% | No; diagnostic order |
| All declaration type decisions, inclusive | A | 0.018 s / 0.05% | 0.082 s / 0.20% | No; inferred signatures |
| Local assignment facts | A | 0.025 s / 0.07% | 0.017 s / 0.04% | No; ordinary assignments |
| Whole `FnCtx` copy | A | 0 calls / 0% | 0 calls / 0% | No; lambda defaults and deferred units |
| Type-name class scans | A | 0 calls / 0% | 0 calls / 0% | No; type diagnostics |
| Bodyless implementation and abstract-method facts | A | 0 calls / 0% | 0 calls / 0% | No; bodyless declarations |
| Source function shadow facts | A | 0 calls / 0% | 0 calls / 0% | No; decorated classes |
| Parameter default lowering lookups | CLI path | Not entered / 0% | Not entered / 0% | No; `check` does not lower code |

Counter run A does not execute the retained `FnCtx` copies.
Code inspection finds those copies in lambda parameter decisions and deferred expression creation.
Shared snapshots remove the payload copies at snapshot creation.
Their unit test verifies independent writes to the source and the snapshot.
Field initializers also use independent snapshots of one unchanged empty context.

After the table-copy fix, a second measurement isolates the remaining binding scan.
This scan reads every module binding for each initializer, even when the initializer reads no bindings.

| Borrowed-table counter run C, `n = 3,000` | Annotated | Inferred |
|---|---:|---:|
| Process elapsed | 2.004349 s | 1.981067 s |
| All-binding scan, 6,000 calls | 0.943338 s / 47.06% | 0.936201 s / 47.26% |
| Initializer order, inclusive | 0.966077 s / 48.20% | 0.960688 s / 48.49% |

The binding scan serves honest forward-read errors, not only contrived forms.

## Changes

- The initializer effect scan borrows one default table. Lambda scans borrow the same table.
- The initializer read check examines accessed bindings and selects the first violation in declaration order.
- Class member labels use one shared index of ambiguous class names.
- The overload check runs only after a duplicate name appears.
- Diagnostic declaration positions are collected only for a rejected program.
- Lexical facts, captures, flow facts, and synthetic prefixes use shared snapshots with copy on write.
- Lambda parameter decisions skip parameters whose types are already decided.
- Annotated instance fields use the body checker directly. Only unannotated fields store initializer decision signatures.
- Annotated field reads use the type in HIR. Field decisions borrow decided states before they copy an initializer.
- Local assignment analysis indexes root statements by local name and retains statements that can remove a path.
- Default-route labels qualify duplicate declarations and retain the default parameter name.
- Shared snapshots carry a type description. Mutable iteration and test comparisons use the underlying collection.

Corpus diagnostics, outputs, and golden files do not change.
No rule change is required.

## Timings

All entries give seconds, best of three. The inferred comparison-pin form is rejected.

The baseline columns use `1af40676` from a separate run on 2026-10-05.
The contract table uses `5e708e40` from an earlier run on that date.
The two tables use different revisions and measurement runs.
Their annotated 200/1,000/3,000 times are 0.20961/6.74064/62.96217 s and 0.22/3.84/35.46 s, respectively.
Their inferred times are 0.21246/6.71962/50.65592 s and 0.22/3.84/63.48 s, respectively.
The earlier comparison-pin run gave 0.04974/0.35004/1.94768 s, against 0.07/0.34/2.25 s in the contract.
The final comparison-pin and corrected-build columns below use fresh runs on 2026-10-05.

| `n` | `40b0cbff` annotated | `1af40676` annotated | `1af40676` inferred | Final annotated | Final inferred |
|---:|---:|---:|---:|---:|---:|
| 200 | 0.04910 | 0.20961 | 0.21246 | 0.04984 | 0.05215 |
| 1,000 | 0.32542 | 6.74064 | 6.71962 | 0.24394 | 0.25952 |
| 3,000 | 1.75704 | 62.96217 | 50.65592 | 0.91465 | 0.98540 |

- Annotated `n = 3,000`: 0.521 times the comparison pin. The limit is 1.2.
- Inferred `n = 3,000`: 1.077 times the final annotated form. The limit is 1.5.
- Annotated scale from 1,000 to 3,000: 3.750, against 5.399 at the comparison pin.
- Inferred scale from 1,000 to 3,000: 3.797, against 5.399 at the comparison pin.

Acceptance 2 passes all three conditions.
The final annotated program is faster than the comparison pin at the two larger sizes.
The 200-block difference is 0.74 ms; the measured first-run startup spread exceeds that difference.

## Annotated field cost

Each form has 6,000 classes with four annotated instance fields.
The read form reads earlier fields. The constant form uses integer literals.
The before column contains the existing §160 fixes before the field and local-flow corrections.
All entries use release builds on 2026-10-05 and give seconds, best of three.

| Form | `40b0cbff` | Before | Final | Final / pin |
|---|---:|---:|---:|---:|
| reads | 0.18495 | 0.25616 | 0.21451 | 1.160 |
| constants | 0.12451 | 0.18860 | 0.14618 | 1.174 |

Annotated fields do not enter the §156 decision path.
They keep their declared type in HIR and receive independent empty-context snapshots.
The field checker checks each initializer once per checker pass.

## Local assignment cost

The original §158 pass scans the whole owning function once for each uninitialized local.
The 2,000-local forms have 6,000 root statements, so the pass visits 12,000,000 root statements.
Initialized locals do not enter this pass.
The CLI checks provisional and final bodies, so it executes this scan twice.

The statement index makes local assignment analysis linear for independent `if` and `switch` forms.
The index selects declarations, reads, writes, and shadow declarations by local name.
It retains returns, throws, unreachable calls, `while`/`for` loops, and exits that leave the root statement for every local.
The original state and exit analysis checks each selected statement.
A skipped statement preserves the selected local's state and returns the same next path.

One large nested root statement or many shared exits still require repeated per-local traversal.
The index retains that cost to preserve the original path diagnostics.

Total CLI times still include the ordinary body-check costs that the initialized controls share.
All entries use release builds on 2026-10-05 and give seconds, best of three.

| Form | Locals | Before, uninitialized | Final, uninitialized | Final, initialized |
|---|---:|---:|---:|---:|
| if | 1,000 | 0.75208 | 0.09885 | 0.09077 |
| if | 2,000 | 3.01602 | 0.32662 | 0.30532 |
| if | 4,000 | 13.02945 | 1.11547 | 1.09079 |
| switch | 1,000 | 1.02283 | 0.09509 | 0.09485 |
| switch | 2,000 | 4.04921 | 0.30889 | 0.30854 |
| switch | 4,000 | 17.12807 | 1.10665 | 1.10614 |

The before initialized controls at 2,000 locals take 0.30535 s for `if` and 0.31361 s for `switch`.

Temporary release counters isolate local assignment analysis, including the statement index after the fix.
The table selects the fastest process run and reports the counter from that same run.
The counters sum the provisional and final checks. These runs do not overlap tests or builds.

| Form | Locals | Before process | Before flow / share | Final process | Final flow / share |
|---|---:|---:|---:|---:|---:|
| if | 1,000 | — | — | 0.090729 s | 0.002410 s / 2.66% |
| if | 2,000 | 3.050651 s | 2.745538 s / 90.00% | 0.297544 s | 0.005478 s / 1.84% |
| if | 4,000 | — | — | 1.088668 s | 0.011161 s / 1.03% |
| switch | 1,000 | — | — | 0.096370 s | 0.002779 s / 2.88% |
| switch | 2,000 | 4.087485 s | 3.773449 s / 92.32% | 0.309560 s | 0.005915 s / 1.91% |
| switch | 4,000 | — | — | 1.108831 s | 0.012304 s / 1.11% |

## Validation

- `cargo test --offline --locked -p subscript-compiler`: 1,004 passed; one existing test ignored.
- `cargo test --offline --locked -p subscript-codegen`: 726 passed; one existing test ignored.
- All 738 accept/reject/trap/warn CLI corpus results match the initial fix: exit status, standard output, and diagnostic output.
- `cargo fmt --check`: pass.
- `cargo clippy --offline --locked --workspace --all-targets`: pass, with no new warning.
- `tools/hygiene.sh`: pass.
- All changed Rust files stay below 2,000 lines.
- No measurement counters remain in the repository.
- `tools/gate.sh` is not run.

The duplicate-binding regression fails before the correction and passes after it.
The binding index retains the first occurrence of each symbol, as the original ordered scan does.


## Phase Review

One pass on the round 1 tree: no CRITICAL, one MAJOR (rule 1 for
annotated field initializers, 1.5 times the pin), five MINOR. The
review compared 3,275 programs (every corpus entry in five annotation
and order variants) on the tree and on HEAD: 0 differences. The copy-on-
write snapshots mutate only through `Rc::make_mut`. Round 2 brought the
field shapes to 1.16–1.17 times the pin (recorded in §160.3), made the
§158 local analysis linear for independent statements, qualified the
default-route labels, and cleaned this note and `snapshot.rs`.

Final gate: `gate full 1af40676 dirty:17 debug 2327/0/3 release 2324/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`
(the contract was amended to `2d2fbed8` after the gate; the amendment
adds §160.3 text only).

Status: COMPLETE.
