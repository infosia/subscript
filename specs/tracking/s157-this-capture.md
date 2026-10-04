# Section 157: receiver capture

Status: complete. Rules 1–5 and acceptance 1–5 pass.

## Measurements at the contract pin

The clean starting HEAD was `e5706619`.
`cargo build --offline --locked -p subscript-cli` passed before any file changed.

The new entry is `corpus/accept/a330-this-capture.ts`.
It covers callbacks, nested arrows, an accessor, a constructor, an async method, and an instance generator method.
Node `v24.18.0`, with TypeScript `5.9.2`, produced the new `.expected` file through the existing corpus runner.
Stock `tsc` accepted the entry with the repository's strict compiler options and ambient prelude.

The pin's `subscript check` exited 1 with these diagnostics:

| Code | Position | Message |
|---|---|---|
| S100 | 46:3 | generator methods are not in the decided surface |
| S100 | 13:29 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 18:36 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 20:7 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 21:18 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 24:7 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 25:18 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 28:7 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 29:19 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 29:38 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 32:34 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 32:58 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 38:29 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 42:34 | a lambda cannot capture `this`; capture a const local instead |
| S100 | 42:58 | a lambda cannot capture `this`; capture a const local instead |
| S018 | 58:31 | `Counter` has no method `steps` |

## Round 1 contract conflict

Section 157 acceptance 1 requires a lambda in a generator method across a `yield`.
Rule 4 rejects lexical `this` in a static method, so a static generator method cannot supply this case.
C24 row 11 rejects an instance generator method because no lowering is decided.
Section 154.3 item 5 lists instance generator methods as an open decision.
`compiler/src/check/class_shape.rs` rejects the declaration at `GeneratorMethodDeclaration` before the checker checks its body.

`CLAUDE.md` forbids the coding agent from a scope change or a contract edit.
The handoff permits only this tracking note under `specs/`.
Round 1 required an owner decision on the instance generator acceptance case.

Round 1 left the new accept entry Red. The amended contract permits the implementation below.
Round 1 changed no existing golden and created no commit.
Round 1 did not run the final checks or Phase Review.

## Amended contract pin

The amended HEAD is `d6ba67f2e6b4e9a32ddd50084db9b820fa8bb48d`.
The CLI build passed before production changes.
The edited entry excludes the instance generator method.
Node v24.18.0 and TypeScript 5.9.2 produced its new golden through the corpus runner.
The checker exited 1 with S100 at 13:29, 18:36, 20:7, 21:18, 24:7, 25:18,
28:7, 29:19, 29:38, 32:34, 32:58, 38:29, 42:34, and 42:58.
Each message was "a lambda cannot capture `this`; capture a const local instead".
The owner removed the contract conflict. Implementation proceeds under the amended contract.

## Implementation

The root function scope contains an immutable local named `this` with the receiver type.
An arrow reads this local through `lookup_local`, which supplies the existing const capture slots and nested capture propagation.
The arrow body uses `ExprKind::Local` with the receiver's declared type.
The existing LIR receiver binding supplies the captured value.
No second environment or escape path exists.
Section 118 infers escape facts from the same `Capture` records.

The capture form needs no new field.
Its existing name carries the lexical-receiver fact, because a source local cannot use the reserved name `this`.
The `Capture::name` documentation states this fact.
The constructor prefix consumes it at calls, including callback arguments and parameter defaults.
The prefix reuses `ConstructorThisBeforeFieldValues` and its member-use message.
A declaration alone does not execute the lambda body.
Local replacements, branch joins, loop back edges, and exception edges preserve the receiver-use fact.

`ThisInMethodArrow` leaves the site table.
`ThisInValueTypeArrow` rejects a receiver copy and cites C24 row 15 and C2.
`ThisInStaticMethodArrow` rejects a lexical receiver in a static method.
Static member access retains `ThisStaticMethodMember`.
Instance and static field initializers retain their section 147 sites.
Function expressions retain their own site; their reason states that they bind their own `this`.
Every site has a section 154 witness.

The corpus adds `a330` and `r353` through `r356`.
Stock TypeScript 5.9.2 accepted all four reject entries with strict options and the ambient prelude.
It also accepted the project, including `a330`.
The two C5 entries expect S009; the ValueType and constructor entries expect S100.
The generated corpus index includes all five entries.
The LIR golden adds the `a330-this-capture` record only.
Removal of that record reproduces the previous file byte for byte.
No existing `.expected` file changes.

## Self-check of the coding agent (round 2)

The fresh review found three realistic MAJOR defects in constructor receiver-flow facts.
Controls now cover clean replacements, throw edges before replacement, and exceptions that an inner handler consumes.
The final review reports no open CRITICAL, MAJOR, or MINOR findings.
`git diff --check` and `tools/hygiene.sh` passed in the final review.
The four new in-process unit tests measured 0.04 seconds after these fixes.
No new test repeats corpus execution on three tiers.

## Final checks

`cargo test --offline --locked -p subscript-compiler` passed after the review fixes.
The first complete codegen suite passed, including the corpus interpreter and both native tiers.
The final `cargo test --offline --locked -p subscript-codegen` rerun passed.
`cargo fmt --check` passed.
`cargo clippy --offline --locked --workspace --all-targets` passed.
Its library warning counts are compiler 5, runtime 18, and codegen 13.
The gate baselines are 7, 18, and 13, respectively; no changed file adds a warning.
`cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passed.
Changed Rust files stay at or below 2,000 lines.
The compiler suite passed 993 tests; the codegen suite passed 726 tests.
Each suite retains one existing ignored test.
`tools/gate.sh` did not run.
No commit exists for this work.

## Round 3: parameter defaults and external lambda bodies

The restored initializer comparison test failed before the fix.
The inferred method default passed the checker and returned a HIR lambda with a `this` capture.

`FnCtx::parameter_default` identifies a default scope.
Parameter decisions, method bindings, constructor bindings, and lambda bindings set this fact during their default checks.
Each path restores the enclosing fact before it checks the body.
Section 156 deferred work clones `FnCtx`, so deferred lambdas and arguments retain this fact.
`ThisInParameterDefaultArrow` rejects receiver captures with S009 and a C5 divergence, as earlier-parameter captures do.
Its reason states that a default executes outside the method frame that holds the receiver.
No capture form changes or second capture path exist.
The initializer comparison table again includes inferred and annotated method defaults.
The constructor control now uses a clean default; moving its call cannot permit a forbidden default capture.
The context control checks that a clean default still permits receiver capture in the method body.
Reject entries `r357` and `r358` cover inferred and annotated defaults.

### Probe inventory

Each source below includes `export function main():void {}` unless the row states a print.
`subscript check` ran on each source.
TypeScript 5.9.2 accepted all class probes in one strict project with the ambient prelude.
The standalone global probe was outside that TypeScript measurement.

| External body or stored work | Probe | Result and receiver source |
|---|---|---|
| Method parameter default | `class C { d:i32=7; m(k=()=>this.d):i32 { return k(); } }` | S009, `ThisInParameterDefaultArrow`. |
| Annotated method default, deferred lambda | `class C { d:i32=7; m(k:()=>i32=():i32=>this.d):i32 { return k(); } }` | Same S009 site and message. |
| Constructor parameter default | `class C { d:i32=7; constructor(k:()=>i32=():i32=>this.d) { k(); } }` | Same S009 site. |
| Lambda parameter default | `class C { d:i32=7; m():i32 { const f=(k:()=>i32=():i32=>this.d):i32=>k(); return f(():i32=>1); } }` | Same S009 site. |
| Instance field, inferred or annotated lambda | `class C { d:i32=7; k=()=>this.d; }`, and `k:()=>i32=():i32=>this.d` | S100, `FieldInitializerThisUse`, section 147 rule 2. |
| Static field, inferred or annotated lambda | `class C { static d:i32=7; static k=()=>this.d; }`, and `static k:()=>i32=():i32=>this.d` | S100, `ThisStaticField`; the class has no instance receiver. |
| Deferred default arguments | `function take(f:()=>i32):i32 { return f(); } class C { d:i32=7; m(k=take(():i32=>this.d)):i32 { return k; } }` | S009, `ThisInParameterDefaultArrow`; the deferred argument retains the default fact. |
| Deferred instance-field arguments | The same `take`, with `class C { d:i32=7; k=take(():i32=>this.d); }` | S100, `FieldInitializerThisUse`; deferred work retains the field context. |
| Deferred static-field arguments | The same `take`, with `class C { static d:i32=7; static k=take(():i32=>this.d); }` | S100, `ThisStaticField`; deferred work retains the missing-receiver site. |
| Module initializer | `const k=()=>this;` | S100, `ThisOutsideMethod`; no receiver exists. |
| Body after a clean default | `class C { d:i32=7; m(k:()=>i32=():i32=>1):i32 { const f=():i32=>this.d; return f()+k(); } }` | JIT prints `8`; C emission passes. The method receiver supplies the capture. |
| Nested lambda body | `class C { d:i32=7; m():i32 { const f=():i32=>{const g=():i32=>this.d;return g();};return f(); } }` | JIT prints `7`; C emission passes. The outer capture supplies the inner capture. |

`codegen/src/lir/defaults.rs` lowers parameter defaults as synthesized helpers at the call site.
Its free-local scan includes lambda captures; a caller outside the method cannot supply the receiver local.
Direct receiver reads use its explicit receiver operand and remain accepted.
`codegen/src/lir/lambda.rs` reads every capture from the defining frame and passes each through the closure environment.
`codegen/src/lir/builder.rs` binds receiver parameters and capture parameters through the same binding table.
Field and static initializers never reach receiver-capture lowering because their checker sites reject the probes above.
Section 156 has two deferred-work forms: `Lambda` and `Arguments`.
They defer checker work, then restore checked HIR to the initializer; they do not introduce another runtime receiver frame.
The probe inventory covers both forms in defaults and fields.

### Round 3 review and checks

A self-check of the coding agent found that the new lambda-default control failed.
The local function-value type loses its parameter-default fact, so the call with no argument rejects the clean control.
The control now supplies a clean argument and still tests the default declaration.
The focused four receiver-capture tests pass in 0.04 seconds.
The site-map test passes after the S009 assertion uses `as_str()`.
The control passes after that change.
The strict TypeScript check accepts both new reject entries; their measured headers state `tsc: accepts`.

The final compiler suite passes 993 tests, with one existing ignored test.
The complete codegen suite passes 726 tests, with one existing ignored test.
Its corpus tests check interpreter, dev JIT, ship C, and the goldens.
`cargo fmt --check` passes.
`cargo clippy --offline --locked --workspace --all-targets` passes.
The library warning counts remain compiler 5, runtime 18, and codegen 13, within the gate baselines.
The API-reference generator passes and adds the two reject entries to the corpus index.
`git diff --check` and `tools/hygiene.sh` pass after the review corrections.
Every changed Rust file contains at most 2,000 lines.
No existing `.expected` file changes, and the existing LIR golden change remains the `a330` record only.
The initializer comparison file matches HEAD after restoration of the parameter-default pair and removal of the blank line.
`tools/gate.sh` did not run. No commit exists for this work.

## Phase Review

One pass, on the round 3 tree: no CRITICAL, no MAJOR, three MINOR.
About 80 probes ran on the dev JIT and ship C against `node`; every
accepted program agreed, every escape form gave S009, and every body
lowered outside the method frame was rejected before lowering. The
MINOR items: the static-method message (recorded in §157.3), stale
reasons in §108 and §147 rule 1a (corrected in the contract commit), and
this note's labels (corrected).

Final gate: `gate full 6c9e0582 dirty:30 debug 2316/0/3 release 2313/0/3 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0`.
The moved golden is the `a330` LIR snapshot record only.

Status: COMPLETE.
