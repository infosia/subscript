# §189 — `subscript boundary` measurement

Date: 2026-10-11. Contract pin: `eabe31c3`. Apple arm64, release build
of `subscript-cli` unless a line says debug.

This round is a measurement round (CLAUDE.md, workflow step 0). Every
production change is reverted. The note records what the prototype
measured.

## Problem

A foreign call can build a scratch copy of a struct, write members back
after the call, and bind a callback. The call site does not show which
of these it does. The pass decision of §187 (`boundary/src/pass.rs`)
decides it. No command prints that decision. Core principle 16 rejects
the hidden costs; this command shows the declared ones.

## 1. Where the facts live

The pass of a parameter is a function of the callee declaration and the
class table only. No argument fact enters the decision:
`parameter_pass`, `value_parameter_pass`, `member_pass`, `element_pass`,
and `struct_pass` read the parameter type, its `const` fact, and the
class view. Measured: over the 65 accept entries that use an interop
mirror, 330 call sites call 199 (entry, callee) pairs. No callee has two
sites with different lines.

| Fact | Checked HIR (`hir::Module`) | LIR (`lir::Module`) |
|---|---|---|
| callee of a call | `Callee::Foreign(name)` in an `Expr` tree | `CallTargetKind::Foreign(id)` in a flat instruction list; `HostCompletionTarget::Foreign(id)`; `SuspendKind::AsyncCall` |
| call position | `Expr` position | `Instruction::pos` |
| parameter type and `const` fact | `ForeignFn` parameters, `ForeignTypeProvenance::ConstPointer` | `ForeignParameter::ty`, `foreign_provenance` |
| member shapes and `const` facts | `BoundaryClasses for hir::Module` | `BoundaryClasses for lir::Module` (the view both tiers read) |
| embedded header | computed by `is_embedded_header` | `Class::is_embedded_header`, copied from HIR |
| member names | `ClassDef` fields | `Class::fields[i].source_name`; field `i` of `StructView::fields` is field `i` of the class |
| one entry per source site | yes (not measured for generics) | no: a generic function gives one call per instantiation (measured: `twice<i32>` and `twice<f64>` give 2 calls at one position) |
| list of every foreign call without a tree walk | no | yes |

Every foreign call is a direct call. A foreign function as a value is
rejected (`S100`, "foreign function `dot` may only be called", C24), so
no foreign call is `CallTargetKind::Indirect`. The LIR walk is total
over instructions and suspension terminators.

The prototype reads LIR, after `subscript_codegen::lir::lower_module`,
through `subscript_compiler::boundary_pass::Classes(&lir)`. This is the
view that `codegen/src/cemit/marshal.rs` and
`codegen/src/lower/func/boundary.rs` read. Each pass in the output is
the return value of the same `pass.rs` function that code generation
calls, with the same arguments.

### What the forms lack (core principle 8)

1. **The reason for a scratch struct.** `pass.rs` returns
   `StructPass::Scratch` but not the members that cause it. The
   prototype adds `scratch_members(view, s) -> Vec<usize>` to
   `pass.rs`. It filters the members with the private `member_copies`,
   the predicate that `largest_copying_set` uses. A command that
   derives the reason outside `pass.rs` is a second derivation.
2. **A fixed array of structs.** `field_shape` maps
   `FixedArray<APt, 2>` to `FieldShape::Embedded(APt)`. Code generation
   stops on that member inside a scratch struct (187.3 item 6). The
   command reads the shape and prints `s.pts: embedded bytes` for a
   call that `subscript emit` rejects with "boundary field `AShape.pts`
   is a fixed array of structs in a struct that the call rebuilds". The
   form does not carry the fact, so the command cannot state it. See
   section 5.
3. **The scratch scope.** `boundary_type_builds_scratch`
   (`codegen/src/lir_types.rs`) decides when a call allocates scratch
   memory. It composes `struct_pass` and `builds_scratch`, and it is
   private to `subscript-codegen`. The prototype does not print it. A
   command that prints it needs the function in the boundary crate or
   in `subscript_compiler::boundary_pass`.
4. **The walk.** Each tier walks the members of a scratch struct and
   calls `member_pass`, `embedded_pass`, and `element_pass` at each
   member. The prototype walks the members again
   (`members` in the prototype). The pass at each node is shared; the
   walk is a third copy. If a tier changes the walk, the command does
   not change with it.
5. **The result.** A result lowers by its read facts (§187 rule 12,
   `read.rs`). The prototype prints `read bytes` or `read by members`
   from `Class::copies_boundary_bytes`, which is the write-direction
   fact. That is a second derivation and is not correct for a contract.

## 2. Prototype and output

Changes (all reverted):

- `boundary/src/pass.rs`: `scratch_members` (26 lines), exported from
  `boundary/src/lib.rs`.
- `cli/Cargo.toml`: `subscript-boundary` moves from dev-dependencies to
  dependencies.
- `cli/src/lib.rs`: subcommand `boundary`, with the arguments of
  `check` (`parse_source_arguments`): load, check, lower, report.
- `cli/src/boundary.rs`: the report (329 lines).

The programs were bound with `subscript bind <header> -o <mirror>`. The
output is the command's stdout, unchanged.

### Math header

```c
typedef struct Vec3 { float x; float y; float z; } Vec3;
Vec3 vadd(Vec3 a, Vec3 b);
float dot(Vec3 a, Vec3 b);
void cross(const Vec3 *a, const Vec3 *b, Vec3 *out);
```

```ts
export function main(): void {
  const a: Vec3 = new Vec3(1.0, 0.0, 0.0);
  const b: Vec3 = new Vec3(0.0, 1.0, 0.0);
  const out: Vec3 = new Vec3(0.0, 0.0, 0.0);
  cross(a, b, out);
  const s: Vec3 = vadd(a, b);
  print(`${dot(s, out)}`);
}
```

```text
math.ts:5:3 cross a: Vec3 | null const pointer, script-memory
math.ts:5:3 cross b: Vec3 | null const pointer, script-memory
math.ts:5:3 cross out: Vec3 | null pointer, script-memory
math.ts:6:19 vadd a: Vec3 by value, bytes
math.ts:6:19 vadd b: Vec3 by value, bytes
math.ts:6:19 vadd result: Vec3 by value, read bytes
math.ts:7:12 dot a: Vec3 by value, bytes
math.ts:7:12 dot b: Vec3 by value, bytes
math.ts:7:12 dot result: f32 value
```

### Descriptor with a string view and a pair

```c
typedef struct PView { const char *data; size_t len; } PView;
typedef struct PMesh { PView name; float bounds[4]; size_t idsCount; const uint32_t *ids; } PMesh;
typedef struct PIds { int32_t tag; size_t idsCount; const uint32_t *ids; } PIds;
int32_t pDraw(const PMesh *m);
int32_t pSum(PIds m);
```

```ts
export function main(): void {
  const ids: u32[] = [1, 2, 3];
  const m: PMesh = new PMesh("cube", [0.0, 0.0, 1.0, 1.0], ids);
  const n: i32 = pDraw(m);
  const p: PIds = new PIds(7, ids);
  print(`${n} ${pSum(p)}`);
}
```

```text
mesh.ts:4:18 pDraw m: PMesh | null const pointer, scratch read-only [const; PMesh.name string view, PMesh.ids pair]
mesh.ts:4:18 pDraw m.name: string view of script bytes
mesh.ts:4:18 pDraw m.ids[]: script-memory
mesh.ts:4:18 pDraw result: i32 value
mesh.ts:6:17 pSum m: PIds by value, scratch [PIds.ids pair]
mesh.ts:6:17 pSum m.ids[]: script-memory
mesh.ts:6:17 pSum result: i32 value
```

The binder rejects `int32_t pDrawV(PMesh m)` (a by-value struct with a
string field): "only a direct pointer parameter has a string-field
lowering". `pSum` is the by-value form without the string view.

### Fill with a written-back pointer target

```c
typedef struct FView { const char *data; size_t len; } FView;
typedef struct FQ { int32_t y; } FQ;
typedef struct FInner { int32_t x; FQ *q; } FInner;
typedef struct FOuter { int32_t k; FInner *inner; } FOuter;
typedef struct FCounter { int32_t n; } FCounter;
typedef struct FCmd { FView name; FCounter *counter; } FCmd;
void fOuterFill(FOuter *out);
void fCmdRun(FCmd *cmd);
```

```ts
export function main(): void {
  const q: FQ = new FQ(1);
  const inner: FInner = new FInner(2, q);
  const outer: FOuter = new FOuter(3, inner);
  fOuterFill(outer);
  const counter: FCounter = new FCounter(0);
  const cmd: FCmd = new FCmd("go", counter);
  fCmdRun(cmd);
  print(`${outer.k} ${counter.n} ${cmd.name}`);
}
```

```text
fill.ts:5:3 fOuterFill out: FOuter | null pointer, scratch written-back [FOuter.inner non-const pointer] writes-back k
fill.ts:5:3 fOuterFill out.inner: scratch written-back [FInner.q non-const pointer] writes-back x
fill.ts:5:3 fOuterFill out.inner.q: script-memory
fill.ts:8:3 fCmdRun cmd: FCmd | null pointer, scratch written-back [FCmd.name string view, FCmd.counter non-const pointer] writes-back name
fill.ts:8:3 fCmdRun cmd.name: string view of script bytes
fill.ts:8:3 fCmdRun cmd.counter: script-memory
```

`writes-back` names the members that `copy_back` does not skip. The
call writes one of them only when C changed it (§187 rule 7).

### Header link, `const` cycle, callback field

```c
typedef struct LView { const char *data; size_t len; } LView;
typedef void (*LCallback)(LView message, void *userdata1, void *userdata2);
typedef struct LBase { int32_t kind; LView name; } LBase;
typedef struct LExt { LBase base; int32_t extra; } LExt;
typedef struct LLinked { int32_t k; const LBase *link; } LLinked;
typedef struct LNode { int32_t v; const struct LNode *next; } LNode;
typedef struct LJob { LCallback callback; void *userdata; void *userparam; int32_t prio; } LJob;
int32_t lUse(const LLinked *l);
int32_t lExt(LExt *e);
void lBump(LNode *n);
void lSubmit(LJob *job);
```

```text
link.ts:6:12 lUse l: LLinked | null const pointer, scratch read-only [const; LLinked.link link to scratch header]
link.ts:6:12 lUse l.link: scratch read-only [const; LBase.name string view]
link.ts:6:12 lUse l.link.name: string view of script bytes
link.ts:6:36 lExt e: LExt | null pointer, scratch written-back [LExt.base embedded scratch struct] writes-back base extra
link.ts:6:36 lExt e.base: embedded scratch
link.ts:6:36 lExt e.base.name: string view of script bytes
link.ts:9:3 lBump n: LNode | null pointer, script-memory
link.ts:11:3 lSubmit job: LJob | null pointer, scratch written-back [LJob.callback callback] writes-back prio
link.ts:11:3 lSubmit job.callback: callback registration
```

(Result lines and the second `lUse` site are left out here.)

### Proposed text format

One line per call site and per crossing position:

```text
<file>:<line>:<col> <callee> <path>: <what> [<cause>, ...] writes-back <member> ...
```

- `<path>` is the parameter name, then `.member` for a member and `[]`
  for pair elements, or `result`.
- `<what>` is one closed word set: `value`, `bytes`, `scratch`,
  `script-memory`, `scratch read-only`, `scratch written-back`,
  `string view of script bytes`, `callback registration`,
  `completion endpoint`, and the result forms.
- `[...]` lists the members from `scratch_members`, each with its shape
  word.
- Lines sort by file, line, column, then callee id. One source site
  that LIR repeats (generic instantiations) prints once.

The words are a closed set and every line starts with its position, so
a golden test can compare the text byte for byte.

### `--json`

Not built. The tree has no JSON serializer (`serde` is a pinned
dependency of `subscript-compiler` only, with no `serde_json`). Cost: a
record type (site, callee, path, what, causes, written-back members)
that both the text and the JSON render from, and either a hand-written
writer with string escapes (about 50 lines) or a new dependency. The
record type is needed for the text form too, if `check --warn` reuses
the facts (section 3).

## 3. CLI surface options

| Option | Cost | Note |
|---|---|---|
| Arguments of `check`: `<file.ts>`, `--mirror`, `--enable-module` | reuse `parse_source_arguments` (0 new lines) | the prototype |
| Arguments of `build`: `--source`, `--host`, `-o` | `--host` and `-o` have no use: the command compiles no C | not proposed |
| Exit 0 when the program checks; 1 with the `check` diagnostics; 2 for a usage error | the existing `Failure` codes | the prototype |
| Exit 1 when a costly site exists (`--deny-...`) | a flag and a predicate | duplicates `check --deny-warnings` if a warning exists |
| Every call site (per-site lines) | 1,036 lines for the 65 entries | the prototype |
| Each callee once, with its site count and positions | 529 lines for the same entries | the pass is a callee fact (section 1), so this loses no information |

### A costly mark that `check --warn` can reuse

`check_warnings` reads the checked HIR (`compiler/src/warn.rs`, with its
own statement walk). `subscript_compiler::boundary_pass` already gives
the HIR class table as a `StructView` (`Classes(&hir_module)`). So a
warning can call the same `pass.rs` functions on HIR, with no lowering.

A shared mark needs one function that both callers use, for example
`boundary_pass::callee_crossings(classes, foreign_fn) -> Vec<Crossing>`
in `subscript-compiler`, generic over `BoundaryClasses`. The command
calls it with the LIR table; the warning calls it with the HIR table.
The mark is then a property of a `Crossing`: `ScratchWrittenBack`,
`Scratch` (a by-value scratch build), or a callback registration.
Because the pass is a callee fact, a warning at the declaration (once
per callee) states the same fact as a warning at each site.

The same function closes item 4 of section 1 only if the tiers walk
its result. If they do not, the function is the third walk.

## 4. Cost

Release build. The prototype prints its phase times to stderr
(`SUBSCRIPT_BOUNDARY_TIMING`). Wall time is the median of 20 process
runs.

| Program | Foreign sites | Check | Lower | Report | Wall, `boundary` | Wall, `check` |
|---|---|---|---|---|---|---|
| `a149-suspension-state` (largest accept entry, 681 lines) | 7 | 6.8 ms | 9.3 ms | 0.02 ms | 19.6 ms | 10.4 ms |
| `a104-interop-recursive-render-pipeline` (most report lines) | 17 | 3.9 ms | 0.8 ms | 0.3 ms | 7.1 ms | 6.2 ms |
| `examples/host/game.ts` with `engine.generated.d.ts` | 12 | 1.1 ms | 0.4 ms | 0.02 ms | 4.6 ms | 4.2 ms |

The report is under 0.3 ms in each case. Lowering is the main added
cost (9.3 ms on `a149`). A HIR-based command does not lower; the HIR
walk was not measured.

Output of `game.ts` (path prefix removed):

```text
game.ts:31:3 engineWorldSetName engineName: string view of script bytes
game.ts:32:3 engineWorldSetTransform engineTransform: EngineTransform by value, bytes
game.ts:37:43 engineWorldApplyFlags engineBatch: EngineEntityBatch by value, scratch [EngineEntityBatch.engineEntityIds pair]
game.ts:37:43 engineWorldApplyFlags engineBatch.engineEntityIds[]: script-memory
game.ts:79:27 engineWorldReadEntities engineStates: EngineEntityState[] pair, elements script-memory
```

(Scalar and handle lines are left out here; the full output has 23
lines.) In `game.ts` no call writes back and no call builds a scratch
target. The one scratch build is the by-value `EngineEntityBatch`.

## 5. What the command cannot state truthfully

| Fact | Why | Prototype |
|---|---|---|
| A null pointer argument builds no scratch copy and writes nothing back | run-time value | prints the non-null pass |
| The size of a scratch array of pair elements | run-time length | prints the pass, not the size |
| Which members a write-back writes | the call compares each member with its snapshot (§187 rule 7) | prints the members that can be written back |
| A string-view member that C changed allocates a new script string at write-back (`CopyBack::StringView`) | run-time compare | not marked |
| A fixed array of structs in a scratch struct | `FieldShape` does not carry it (section 1, item 2); code generation stops | prints `embedded bytes`, which is false |
| A header link whose box holds an extension | §187 rule 14: the call passes the declared header only; C that reads the extension reads past the scratch copy (187.3) | prints the header pass; no extension note |
| One object passed to two written-back parameters (187.3 item 7) | aliasing of argument values | not detected |
| The result read | `read.rs` facts, not the write-direction fact | prints from `copies_boundary_bytes` (section 1, item 5) |

Host-callable exports and worker messages also cross a boundary. They
are not foreign call sites, and the prototype does not list them.

## Decisions a contract needs

1. The stage: LIR (total flat walk, one view with the tiers, lowering
   cost, generic repeats) or checked HIR (no lowering, a tree walk, the
   view the checker uses).
2. Where the reason and the walk live: `scratch_members` in `pass.rs`,
   and one crossing function that the command, the warning, and
   (optionally) both tiers call. Without it the command is a third walk.
3. The fixed-array-of-structs gap: add the fact to `FieldShape` (or
   reject the position in the binder and the checker) before the
   command states a pass for it.
4. The result line: from `read.rs` or left out.
5. Per site or per callee, and whether `--json` is in scope.
6. Whether a costly mark is a warning code of `check` (per callee or per
   site), a flag of this command, or neither.

## Implementation

Contract pin: `88f19be0`. Apple arm64.

### The plan

| Item | Place |
|---|---|
| Plan function | `subscript_compiler::crossing::call_plan(module: &lir::Module, callee: lir::ForeignFunctionId) -> Result<CallPlan, String>` (`compiler/src/crossing.rs`) |
| Plan cache, one plan per callee | `crossing::Plans::get` |
| Site walk (instructions and suspension terminators) | `crossing::call_sites` |
| Format and closed word set | `compiler/src/crossing/render.rs` (`Word`, `report`, `render_site`) |
| Members that make a struct a scratch struct | `subscript_boundary::scratch_members` (`boundary/src/pass.rs`), with the predicate of `struct_pass` |

The plan is in `subscript-compiler` because the LIR types are there, and
both `subscript-codegen` and `subscript-cli` depend on that crate.
`subscript-boundary` has no LIR types, and `subscript-compiler` depends
on it, so a plan there needs a dependency cycle.

A `CallPlan` holds one `ParameterPlan` per declared parameter, a
`ResultPlan`, and `scratch_scope`. A struct node (`StructPlan`) holds the
pass, the cause fields, and one `MemberPlan` per member that the build
gives (the userdata fields that a callback absorbs have no entry). A
pointer node (`PointerPlan`) holds the pass, the target build, and the
`WriteBackPlan`: the members that `copy_back` does not skip, with an
embedded scratch struct as a nested `WriteBackPlan`.

- The result plan (`ReadPlan`) comes from `read.rs`: `MemberKind::of`
  and `member_read` at `ReadRoot::Value`, and `first_unreadable` at
  `ReadRoot::Value`, `Reach::Root`. `read by members` names each member
  whose read goes through a pointer or pair elements, at the root or
  through embedded structs.
- The scratch scope test (`boundary_type_builds_scratch`) moved from
  `codegen/src/lir_types.rs` into the plan (`CallPlan::scratch_scope`).
- A fixed array of structs in a scratch struct is
  `MemberCrossing::NotLowered(NotLowered::FixedArrayOfStructs)`. The plan
  reads the LIR field type, which carries the fact that `FieldShape`
  does not carry (section 1, item 2).

### The tiers

C AOT (`codegen/src/cemit/marshal.rs`, `cemit/call.rs`) moved first, then
the dev JIT (`codegen/src/lower/func/boundary.rs`). Each tier gets the
plan of the callee at each foreign call and dispatches on its nodes. No
tier calls `parameter_pass`, `value_parameter_pass`, `member_pass`,
`embedded_pass`, `element_pass`, or `copy_back`. Each tier keeps one
plan per callee for the module (`crossing::Plans`). The tiers record no
plan for a test.

Both tiers stop with an internal error on a result plan with an
unreadable member. This replaces the dev JIT check on result field types
(`Func`, `Array`, `Str`). The checker rejects such a result first.

Two member walks of the dev JIT are outside the plan:

- The C-layout walk (`boundary_c_layout_in`) calls `struct_pass` for its
  check of a fixed array of structs in a struct that the call rebuilds.
  It stops with the text of `NotLowered::FixedArrayOfStructs`, before
  the member build reads the plan node.
- The stabilize walk of a struct result
  (`stabilize_boundary_return_value`) visits the embedded structs and
  the struct pair elements of the result. It decides no pass.

The member build of the dev JIT (`populate_boundary_value`) stops on a
plan node with the pass `Cycle` (the struct-cycle text) or `Bytes` (an
internal error), as the C AOT build does.

### Byte identity

The C emission of every accept entry (351) and trap entry (106) was
written before the change and after each tier moved: `program.c`,
`program.h`, and the allocation metadata, 457 files. All files are
byte-identical. `subscript emit examples/host/game.ts --no-entry` with
`examples/engine/engine.generated.d.ts` is byte-identical. The accept
goldens of both tiers (`codegen/tests/golden.rs`) and the LIR goldens
(`codegen/tests/lir.rs`) pass with no change.

### The command

`subscript boundary <file.ts> [--mirror <d.ts>]... [--enable-module
<module>] [--deny-warnings]`: the arguments of `check`
(`parse_source_arguments`). It loads, checks, writes the `check`
warnings, lowers with `lir::lower_module`, and prints `report`. Exit 0
when the program checks; the `check` status and diagnostics otherwise;
2 for a usage error.

| Word | Where |
|---|---|
| `value` | a scalar, enum, wire alias, or handle parameter or result |
| `by-value bytes`, `by-value scratch` | a by-value struct parameter |
| `script memory` | a pointer target or pair elements that the call passes in place |
| `scratch read-only, none for null` | a scratch copy that the call does not write back |
| `scratch written-back if changed, none for null` | a scratch copy that the call writes back |
| `embedded bytes`, `embedded scratch` | an embedded struct member |
| `pair` | a pair; its elements are on the `<path>[]` line |
| `scratch read-only per element` | pair elements that the call copies into a scratch array |
| `string view of script bytes` | a string parameter or member |
| `callback binding`, `callback registration` | a callback member (§111 lifetime) |
| `completion endpoint` | the endpoint of a host completion |
| `not lowered: <reason>` | a fixed array of structs in a scratch struct; a struct cycle; written-back pair elements |
| `read bytes`, `read by members` | a struct result |

A member that the build copies as bytes has no line. Causes print as
`<Struct>.<member> <shape>`. A generic function prints one line set per
instantiation, at the same position.

### Tests and costs

Debug, Apple arm64.

| Test | What it compares | Cost |
|---|---|---|
| `cli/tests/boundary.rs` `the_goldens_cover_every_word` | 7 corpus goldens and 1 bound header (`cli/tests/boundary/*.txt`), byte for byte; every word of `Word::ALL` in some golden | 0.84 s alone: 8 command processes and 1 `bind` |
| `cli/tests/boundary.rs` `a_rejected_program_gives_the_check_status_and_diagnostics` | stderr and status equal to `check`; usage exit 2 | 0.58 s alone: 4 command processes |
| `cli/tests/boundary.rs` `the_emitted_c_marks_and_writes_back_as_the_plan_says` | for each call of each callee of the 7 corpus programs of acceptance 1, the emitted C (`emit_c`, no C compiler) has a scratch mark exactly when the plan opens a scratch scope, and a write-back block exactly when a plan node is written back; the call count equals the site count of `call_sites`; both forms of each fact occur | 0.30 s alone |
| `cli/tests/boundary.rs` `a_c_text_or_a_plan_that_disagrees_fails_the_compare` | a C text with the write-back guard removed, a C text with an added scratch mark, a plan with another scope, and another site count each fail the compare; the test builds each form | 0.04 s |
| `crossing_plan.rs` plan, cache, and report tests | node passes, causes, write-backs, result read, one build per callee, sort order, instantiations | under 0.02 s |
| `compiler` `crossing::tests`, `boundary` `pass::tests` | distinct words and reasons; `scratch_members` against `struct_pass` | under 0.01 s |

The two words `not lowered` and `read by members` have no accept entry:
the first stops code generation, and no accept entry returns a struct
with a pointer member. The bound header in `cli/tests/boundary.rs` gives
both.

### Command time

Release, wall time of the process, median of 20 runs.

| Program | `check` | `boundary` |
|---|---|---|
| `a149-suspension-state` with `interop.generated.d.ts` | 10.5 ms | 19.9 ms |
| `examples/host/game.ts` with `engine.generated.d.ts` | 4.3 ms | 4.7 ms |

The LIR lowering is the added cost on `a149`, as in section 4.
