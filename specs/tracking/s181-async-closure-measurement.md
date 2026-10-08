# §181 — async closure measurement

Date: 2026-10-08. Contract pin: `f561804d`.

A hidden reference class preserves the measured `const` captures in all three tiers.
The source conversion prototype does not implement the existing function-value ABI.
This round does not establish a complete compiler implementation of escaping async closures.

## Part A — Node and the current language

Node v24.18.0 ran each original program after TypeScript transpilation.
TypeScript accepted all ten programs with `--strict --target es2022 --lib es2022 --module commonjs` and `prelude/lang.d.ts`.
Node acts as a divergence detector.
The committed goldens remain the corpus reference.

Each async body below executes `await value(0)` before it reads its capture, except the `map` body.
`value(n)` is an async function that returns `n`.
The appendix contains each complete input.

| Program | Captured value and use | Node stdout | Current language |
|---|---|---|---|
| direct | `const base = 7`; direct arrow call with `3` | `10` | S009, capture `base` |
| held | Same arrow; store its handle before the await | `10` | S009, capture `base` |
| later | Pass the arrow to an async function; call after another await | `10` | S009, capture `base` |
| field | Store in `Box.f`; call after `make()` returns | `7` | S009, capture `base` |
| loop | Store three arrows; each captures its iteration's `const n` | `0`, `1`, `2` | S009, capture `n` |
| handle | Capture `const h = value(7)`; await `h` in the arrow | `7` | S009, capture `h` |
| reference | Capture a `Box`; change `b.n` before the call | `8` | S009, capture `b` |
| string | Capture `const s = "seven"` | `seven` | S009, capture `s` |
| let | Start the arrow, then change `n` from `7` to `8` | `8` | S009, capture `n` |
| map | Map `[1, 2]` with an async arrow and `base = 7` | `8`, `9` | S009 and S014 |

The S009 message is `async arrow captures NAME; an async arrow captures nothing`.
The diagnostic encloses `NAME` in backticks.
Its explanation is: "An async callable uses a null environment. It has no retained capture storage."
The S014 message is: "`map` cannot carry a counted callback result (§171)".
The release compiler built from `f561804d` emits exactly the same ten diagnostic results.
Every current-language check exited with status 1.
Every Node run exited with status 0.

## Part B — design answers

### 1. Environment layout and holders

Use one hidden reference object for each closure creation.
Give each captured `const` one field of its checked type.
Use the ordinary class layout, alignment, allocation header, pointer description, and counted-field description.
A scalar-only environment with one `i32` has a four-byte payload and a sixteen-byte allocation header in the development tier.
On this 64-bit target, strings, reference objects, handles, and generators occupy eight-byte fields.
The ordinary class layout aligns each field and rounds the payload to its maximum field alignment.
The ship allocator rounds that payload to sixteen bytes.

The closure value must hold the environment pointer.
Each active or suspended async frame must also root that pointer.
A reference object has no ordinary holder count today.
Thus, the last holder's exit removes a root; it does not free the environment immediately.
An explicit collection reclaims an unreachable environment and releases its counted fields.
Context release also reclaims its storage.
Do not give this object an implicit collector or an unmeasured reference count.

Evidence: `codegen/src/layout.rs`, `Layouts::build`; `codegen/src/lir/lowering.rs`, `Lowering::lower_classes`.
`runtime/src/context/lifecycle.rs`, `Context::alloc`, supplies the allocation.
`runtime/src/context/counted.rs`, `Context::describe_object` and `Context::unreachable_releases`, supplies the counted-field release.
The prototype uses these existing class paths.

### 2. Counted captures and §175

The environment must acquire a captured handle or generator when it stores that field.
It must use the same counted store path as an explicit class constructor.
A source binding's exit then releases only that binding's count.
Collection or explicit free releases the environment's field count once.
The async frame roots the environment across suspension.

An owned async capture no longer borrows the source binding.
Therefore, §175's block restriction does not apply to that owned capture.
Keep §175 for synchronous borrowed captures and any capture that still uses borrowed storage.
The compiler must carry the ownership distinction through transitive capture facts.
Deleting the block restriction for all lambdas does not supply that distinction.

Evidence: `codegen/src/lir/builder.rs`, `FunctionBuilder::acquire_stored_operands` and `FunctionBuilder::acquire_owner`.
`codegen/src/lir/construct.rs`, `FunctionBuilder::store_class_field`, uses the field store path.
`runtime/src/context/counted.rs`, `Context::take_object_releases` and `Context::release_reference_holder`, releases those fields.
`compiler/src/check/capture.rs`, `Analysis::fact`, currently marks a lambda with captures as an escape effect.
Its `Analysis::finish` also checks counted capture blocks.

An additional class control returns an environment with a handle from `make()`.
Its async method suspends, calls `Context.collect()`, and then awaits the captured handle.
JIT and interpreter print `7`; neither reports a released handle.
A generator control returns an environment with `Generator<i32>` from `make()`.
Its method suspends, collects, then reads `next().value`.
JIT, C AOT, and interpreter print `7`.
These controls establish the existing class path, not a new closure ownership path.

### 3. Mutable captures and cells

A copied `let` does not preserve shared-variable semantics.
The Part A probe prints `8` in Node after a write outside the arrow.

A cell needs one field for the variable and one shared cell identity for every holder of that binding.
Rewrite every enclosing read, write, increment, compound assignment, and nested capture to use the cell.
An escaping closure and a suspended enclosing frame must root the cell.
A counted cell field needs counted acquire, replacement release, and collection release.
A per-iteration lexical binding needs a distinct cell for each JavaScript iteration environment.

A cell adds an allocation, a pointer field, and an indirection for each access.
The compiler can group cells from the same lexical environment only if it preserves each binding's lifetime.
A temporal dead zone needs a separate initialization fact and a check before a read.
The current language's definite-assignment rules are a separate constraint.

Node measurements:

| Probe | Node result |
|---|---|
| Two closures share `let n`; one increments after suspension; enclosing code writes `10` | `shared 11 11 11` |
| Same program with one object cell and rewritten accesses | `boxed 11 11 11` |
| `for (let i = 0; i < 3; i++)` creates one arrow per iteration | `loop 0,1,2` |
| Same outputs with a distinct object cell per iteration | `cell-loop 0,1,2` |
| Invoke an arrow before its captured `let` initializes | `tdz ReferenceError` |
| Invoke that arrow after initialization | `initialized 4` |

The class-cell control prints `shared 11 11 11` in all three language tiers.
A cell with no initialization check fails the temporal-dead-zone requirement.
These probes do not establish complete JavaScript semantics for every cell form.
The prototype keeps `let` captures rejected.
Evidence: `compiler/src/check/lookup.rs`, the captured-local lookup rejects mutable synchronous captures.
Its async branch instead selects `AsyncArrowCapture` today.

### 4. Loops and collection

Each evaluated arrow site needs a new environment, including each loop iteration.
The three-iteration probe prints `0`, `1`, `2` after the loop ends in all three converted tiers.
A single reused environment changes this result.

The creation-only probe keeps 1,000 environments before collection.
The development tier reports 4,000 live payload bytes and 20,000 reserved bytes.
The ship tier reports 1,000 live allocations and 16,000 live payload bytes.
An explicit collection after the loop reduces both live counts to zero.
The handwritten class has the same measured growth.
No implicit collection occurs.

### 5. `map(async …)`

Keep §171 rule 5a for this prototype.
Closure conversion does not supply ownership for callback results.
The original `map` probe reports both S009 and S014.

To admit a counted callback result, the map operation must take the fresh returned handle count into each output element.
It must not acquire that fresh count twice.
A borrowed callback result instead needs an acquire.
The output array needs its counted-element description and its ordinary holder count.
A trap or exception partway through map must release every element already stored.
The callback's suspended frame must root its environment after map returns.
The verifier needs count facts for this internal callback-result store.
The three tiers need the same path.

Evidence: `compiler/src/check/expr/method.rs`, `Checker::expect_callback_shape`, rejects a counted callback result.
`codegen/src/lir/verify_counted_operations.rs` checks counted operations.
`codegen/src/lower/func/intrinsic.rs` passes callback code and environment to array operations.
The source prototype changes the callback to `S181Env`; the checker then reports S100 for its callback type.
That S100 proves no map support in this prototype.

### 6. Hot reload

A suspended frame must retain its old environment and its old layout description until retirement.
A reload must not interpret old fields with a new class layout.
Keep the existing stale-frame rule unless a separate frame migration design supplies those facts.
An old closure value also needs a policy for its code generation and environment layout after reload.
A successful stale-frame control does not establish that old-value policy.

A class environment control suspends inside `Env.run()`.
Without reload, it collects and prints `7`.
After reload, its next async step traps `StaleCoroutine`.
Evidence: `codegen/tests/async_function_values.rs`, `arrow_suspended_across_reload_traps_with_no_reload_control`, supplies the same-shape test pattern.
The prototype's temporary `environment_reload` test uses `ReloadSession::reload` and `ReloadSession::async_step`.

### 7. The three tiers and the interpreter

The shared checked form must distinguish borrowed synchronous storage from an owned async environment.
Each function value needs its code pointer, environment pointer, and enough static facts for roots and releases.
The producer must place the environment root into its async frame before the first body instruction.
The interpreter must mirror native counted fields and explicit collection.
An `Rc` of interpreter capture values alone does not establish native lifetime parity.

Existing synchronous lowering evidence:

- `codegen/src/lir/lambda.rs`, `FunctionBuilder::lower_lambda_with_id`, emits `MakeClosure` with capture operands.
- `codegen/src/lower/func/builtin.rs`, `Body::make_closure`, stores captures into temporary environment storage.
- `codegen/src/lower/func/value.rs`, `Body::relocate_closure_environment`, copies that storage into a destination.
- `Body::closure_environment_address` selects the synchronous shadow frame or the coroutine frame.
- `codegen/src/cemit/emitter.rs` emits `SubEnv` and the shared `SubEnvStorage` union.
- `codegen/src/cemit/body.rs` copies environment bytes on function-value stores and snapshots.
- `codegen/src/interpreter/instruction.rs` constructs an `Rc<Callable>` with a vector of capture values for `MakeClosure`.

These paths support the current non-escape rule.
They do not allocate an independently collectible environment for each async closure.
A retained environment must bypass the old environment-byte relocation path.

`codegen/src/lower/func/async_callable.rs`, `define_async_callable`, currently drops the wrapper's incoming environment argument.
`codegen/src/cemit/async_callable.rs`, `Emitter::emit_async_callable`, passes an environment for a lambda.
The current capture rejection masks this difference.
A production prototype needs both producer paths to consume the same owned-environment fact.

## Part C — bounded source conversion prototype

The prototype parses the source and replaces one captured async arrow site with a hidden class.
It creates typed fields, a constructor, and an async `run` method.
It changes captured identifier reads into `this.FIELD` reads.
It changes the arrow expression into `new S181Env(CAPTURES)`.
It changes each measured indirect call into a `run` method call.
It changes the measured callable annotations into the environment's nominal class type.
The existing compiler then checks and executes that source in each tier.

This is a source conversion experiment, not an integrated HIR or LIR closure implementation.
It does not preserve arbitrary callable substitution, multiple closure sites, higher-order library callbacks, or capture analysis for arbitrary syntax.
It rejects mutable captures.
It supports the scalar, string, reference, and handle sites below.
Its default field arrow control uses a scalar default environment.
That default is sufficient for the measured field assignment; it is not a general conversion of default arrows.

| Converted Part A program | JIT | C AOT | Interpreter | Node comparison |
|---|---|---|---|---|
| direct | `10` | `10` | `10` | Equal |
| held | `10` | `10` | `10` | Equal |
| later | `10` | `10` | `10` | Equal |
| field | `7` | `7` | `7` | Equal |
| loop | `0`, `1`, `2` | `0`, `1`, `2` | `0`, `1`, `2` | Equal |
| handle | `7` | `7` | `7` | Equal |
| reference | `8` | `8` | `8` | Equal |
| string | `seven` | `seven` | `seven` | Equal |
| let | Prototype rejects | Prototype rejects | Prototype rejects | Diverges |
| map | S100 callback type | S100 callback type | Not executed | Unsupported |

The additional cell, generator, and escaping-handle interpreter controls also pass.
The temporary measurement test has three tests; all pass in release mode.

The corpus results at the unchanged production pin:

| Test | Result |
|---|---|
| `golden::jit_ship_c_aot_and_golden_agree_byte_for_byte` | 340 compared; 0 skipped; pass; 23.01 seconds |
| `lir::lir_interpreter_profile_matches_corpus_goldens`, full release sweep | 274 selected; 274 matched; 66 declared exclusions; pass; 190.421 seconds |

The corpus tests use the existing production compiler.
They do not establish coverage of the source converter over arbitrary corpus syntax.
Neither test starts `tools/gate.sh`.

### Release cost, best of three

Each standalone ship binary ran alone.
Each run used at least three warmups, a 200-millisecond warmup floor, and eleven timed samples.
The table reports each run's median, then the lowest median of the three runs.
No threshold applies.
The driver comes from `benchmarks/async-cost-entry.c`.
The C flags are `-std=c11 -O2 -ffp-contract=off -fwrapv`.

Each timed span includes a fresh Context, initialization, the whole script, async progress, and Context release.
Compilation and source conversion are outside the span.
The workload creates and calls 10,000 environments, then prints checksum `50005000`.
Its body returns `n + base` without an internal await.
Thus, the timing does not measure a suspended arrow body.
Both workloads have zero unfinished tasks and one checkpoint.

| Form | Median run 1, ns | Median run 2, ns | Median run 3, ns | Best median, ns | Best median / 10,000 |
|---|---:|---:|---:|---:|---:|
| Converted async arrow | 841,000 | 759,000 | 901,000 | 759,000 | 75.9 ns |
| Handwritten class | 877,000 | 818,000 | 765,000 | 765,000 | 76.5 ns |

The measured ratio is 0.9922.
Both binaries lower to ordinary class creation and method calls.
This ratio does not measure a callable pair, an indirect call, or a new retained-closure implementation.
The samples do not establish a meaningful speed difference.

| Creation-only form, 1,000 creations | Run medians, ns | Best median | Live allocations | Ship live payload |
|---|---|---:|---:|---:|
| Converted arrow, no calls | 9,000 / 9,000 / 9,000 | 9,000 ns | 1,000 | 16,000 bytes |
| Handwritten class, no calls | 9,000 / 9,000 / 9,000 | 9,000 ns | 1,000 | 16,000 bytes |
| Zero creations | 0 / 0 / 0 | Below clock resolution | 0 | 0 bytes |
| 1,000 creations, then collection | 10,000 / 10,000 / 10,000 | 10,000 ns | 0 | 0 bytes |

Each environment creation adds one Context allocation.
A call adds an async frame allocation; the handle release retires that frame.
The create-and-call workload leaves 10,001 allocations and 160,016 ship payload bytes before Context release.
The extra allocation holds the printed checksum string.
These live-allocation counts do not measure total system allocator calls.

### Remaining divergences and missing measurements

1. The current compiler still rejects every capturing async arrow with S009.
2. This prototype replaces function types with class types; it does not preserve the public function-value contract.
3. An integrated environment ownership fact, producer lowering, and function-value root description remain unimplemented.
4. The source converter does not process arbitrary lexical scopes or arbitrary identifier contexts.
5. Counted `map` callback results stay rejected; no ownership implementation or cost measurement covers them.
6. Shared cells match the measured ordinary accesses, but no compiler cell conversion exists.
7. The temporal dead zone and all per-iteration mutable-binding cases remain outside the cell experiment.
8. An old closure value across reload remains unmeasured; only a suspended class frame has a reload control.
9. These timings cover the class rewrite in C AOT; they do not measure integrated JIT or interpreter closure overhead.
10. The experiment does not measure counted-capture exceptions, environment cycles, or multiple heterogeneous closure sites.

## Files and final tree

The only repository file created temporarily was `codegen/tests/s181_measurement.rs`.
It contained interpreter, allocation, and reload controls.
The final tree removes that file.
`git status --short` prints only `?? specs/tracking/s181-async-closure-measurement.md`.
`git diff --stat` prints no output.
`git diff --exit-code HEAD` exits with status 0.
No production source, corpus file, golden, or contract changed.
The only retained file is `specs/tracking/s181-async-closure-measurement.md`.

## Appendix — complete measured inputs

### direct

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { const base: i32 = 7; print(`${await (async (n: i32): Promise<i32> => { await value(0); return n + base; })(3)}`); }
```

### held

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { const base: i32 = 7; const f = async (n: i32): Promise<i32> => { await value(0); return n + base; }; const h = f(3); print(`${await h}`); }
```

### later

```ts
async function value(n: i32): Promise<i32> { return n; }
async function later(f: (n: i32) => Promise<i32>): Promise<i32> { await value(0); return await f(3); } export async function main(): Promise<void> { const base: i32 = 7; print(`${await later(async (n: i32): Promise<i32> => { await value(0); return n + base; })}`); }
```

### field

```ts
async function value(n: i32): Promise<i32> { return n; }
class Box { f: () => Promise<i32> = async (): Promise<i32> => 0; } function make(): Box { const base: i32 = 7; const b = new Box(); b.f = async (): Promise<i32> => { await value(0); return base; }; return b; } export async function main(): Promise<void> { const b = make(); print(`${await b.f()}`); }
```

### loop

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { const fs: (() => Promise<i32>)[] = []; for (let i: i32 = 0; i < 3; i++) { const n: i32 = i; fs.push(async (): Promise<i32> => { await value(0); return n; }); } for (const f of fs) { print(`${await f()}`); } }
```

### handle

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { const h = value(7); const f = async (): Promise<i32> => { await value(0); return await h; }; print(`${await f()}`); }
```

### reference

```ts
async function value(n: i32): Promise<i32> { return n; }
class Box { n: i32 = 7; } export async function main(): Promise<void> { const b = new Box(); const f = async (): Promise<i32> => { await value(0); return b.n; }; b.n = 8; print(`${await f()}`); }
```

### string

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { const s: string = "seven"; const f = async (): Promise<string> => { await value(0); return s; }; print(await f()); }
```

### let

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { let n: i32 = 7; const f = async (): Promise<i32> => { await value(0); return n; }; const h = f(); n = 8; print(`${await h}`); }
```

### map

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { const base: i32 = 7; const ids: i32[] = [1,2]; const hs = ids.map(async (id: i32): Promise<i32> => { return await value(id + base); }); for (const h of hs) { print(`${await h}`); } }
```

### Representative conversion

The converted held-handle input is:

```ts
class S181Env {
base: i32;
constructor(base: i32) { this.base = base; }
async run(n: i32): Promise<i32> { await value(0); return n + this.base; }
}
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { const base: i32 = 7; const f = new S181Env(base); const h = f.run(3); print(`${await h}`); }

```

### bench-arrow

```ts
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { let sum: i32 = 0; for (let i: i32 = 0; i < 10000; i++) { const base: i32 = i; const f = async (n: i32): Promise<i32> => { return n + base; }; sum += await f(1); } print(`${sum}`); }
```

### bench-class

```ts
class Manual { base: i32; constructor(base: i32) { this.base = base; } async run(n: i32): Promise<i32> { return n + this.base; } }
async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> { let sum: i32 = 0; for (let i: i32 = 0; i < 10000; i++) { const c = new Manual(i); sum += await c.run(1); } print(`${sum}`); }
```

### alloc-arrow

```ts
export function main(): void { for (let i: i32 = 0; i < 1000; i++) { const base: i32 = i; const f = async (): Promise<i32> => { return base; }; } }
```

### handle-escape

```ts
async function value(n: i32): Promise<i32> { return n; }
class Env { h: Promise<i32>; constructor(h: Promise<i32>) { this.h = h; } async run(): Promise<i32> { await value(0); Context.collect(); return await this.h; } } function make(): Env { const h = value(7); return new Env(h); } export async function main(): Promise<void> { const e = make(); print(`${await e.run()}`); }
```

### generator

```ts
function* numbers(): Generator<i32> { yield 7; yield 8; }
class Env { g: Generator<i32>; constructor(g: Generator<i32>) { this.g = g; } async run(): Promise<i32> { await Context.suspend(); Context.collect(); return this.g.next().value; } }
function make(): Env { const g = numbers(); return new Env(g); }
export async function main(): Promise<void> { const e = make(); print(`${await e.run()}`); }

```

### cell

```ts
async function value(n: i32): Promise<i32> { return n; }
class Cell { n: i32 = 1; }
class Env { cell: Cell; constructor(cell: Cell) { this.cell = cell; } async bump(): Promise<i32> { await value(0); this.cell.n++; return this.cell.n; } async read(): Promise<i32> { await value(0); return this.cell.n; } }
export async function main(): Promise<void> { const cell = new Cell(); const a = new Env(cell); const b = new Env(cell); const h = a.bump(); cell.n = 10; print(`shared ${await h} ${await b.read()} ${cell.n}`); }

```

### Source conversion core

The prototype uses the TypeScript parser API.
This function records its exact limited transform.
It returns source for the ordinary compiler input.

```js
function convert(ts, text) {
const sf=ts.createSourceFile('input.ts',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TS);
const arrows=[], bindings=new Map();
function scan(n){
 if(ts.isVariableDeclaration(n)&&ts.isIdentifier(n.name)){
  let type=n.type?.getText(sf);
  if(!type&&n.initializer){ const x=n.initializer; type=ts.isNewExpression(x)?x.expression.getText(sf):ts.isCallExpression(x)?'Promise<i32>':ts.isStringLiteral(x)?'string':'i32'; }
  bindings.set(n.name.text,{type,mutable:!(n.parent.flags & ts.NodeFlags.Const)});
 }
 if(ts.isArrowFunction(n)&&n.modifiers?.some(m=>m.kind===ts.SyntaxKind.AsyncKeyword)) arrows.push(n);
 ts.forEachChild(n,scan);
}
scan(sf);
let chosen, captures;
for(const a of arrows){
 const params=new Set(a.parameters.map(p=>p.name.getText(sf))), found=new Set();
 function refs(n){if(ts.isIdentifier(n)&&bindings.has(n.text)&&!params.has(n.text)&&!(ts.isPropertyAccessExpression(n.parent)&&n.parent.name===n))found.add(n.text);ts.forEachChild(n,refs);}
 refs(a.body);
 if(found.size){ if(chosen)throw Error('prototype admits one captured arrow site per module'); chosen=a; captures=[...found]; }
}
if(!chosen)throw Error('no captured arrow');
for(const c of captures)if(bindings.get(c).mutable)throw Error('mutable capture stays rejected: '+c);
const edits=[];
function edit(n,text){edits.push([n.getStart(sf),n.end,text]);}
const bodyEdits=[];
function bodyVisit(n){if(ts.isIdentifier(n)&&captures.includes(n.text)&&!(ts.isPropertyAccessExpression(n.parent)&&n.parent.name===n))bodyEdits.push([n.getStart(sf)-chosen.body.getStart(sf),n.end-chosen.body.getStart(sf),'this.'+n.text]);ts.forEachChild(n,bodyVisit);}
bodyVisit(chosen.body);
let body=chosen.body.getText(sf);
for(const [s,e,t] of bodyEdits.sort((a,b)=>b[0]-a[0]))body=body.slice(0,s)+t+body.slice(e);
if(!ts.isBlock(chosen.body))body='{ return '+body+'; }';
const params=chosen.parameters.map(p=>p.getText(sf)).join(', '),ret=chosen.type.getText(sf);
let cls='class S181Env {\n'+captures.map(c=>`${c}: ${bindings.get(c).type};`).join('\n')+'\nconstructor('+captures.map(c=>`${c}: ${bindings.get(c).type}`).join(', ')+') { '+captures.map(c=>`this.${c} = ${c};`).join(' ')+' }\nasync run('+params+'): '+ret+' '+body+'\n}\n';
edit(chosen,'new S181Env('+captures.join(', ')+')');
const fnNames=new Set(['f']);
function rewrite(n){
 if(n===chosen)return;
 if(ts.isFunctionTypeNode(n)) {edit(n,'S181Env');return;}
 if(ts.isArrowFunction(n)&&arrows.includes(n)) { // default field has the same callable shape
  const args=captures.map(c=>{const t=bindings.get(c).type;return t==='string'?'""':t==='i32'?'0':null;});
  if(args.some(x=>x===null))throw Error('non-scalar default closure');
  edit(n,'new S181Env('+args.join(', ')+')');return;
 }
 if(ts.isCallExpression(n)){
  const e=n.expression;
  if(ts.isIdentifier(e)&&fnNames.has(e.text))edit(e,e.getText(sf)+'.run');
  else if(ts.isPropertyAccessExpression(e)&&e.name.text==='f')edit(e,e.getText(sf)+'.run');
  else if(ts.isParenthesizedExpression(e)&&e.expression===chosen)edits.push([e.end,e.end,'.run']);
 }
 ts.forEachChild(n,rewrite);
}
rewrite(sf);
let src=sf.text;
for(const [s,e,t] of edits.sort((a,b)=>b[0]-a[0]))src=src.slice(0,s)+t+src.slice(e);
return cls + src;
}
```

### Node shared-cell and temporal-dead-zone probes

```js
const print=console.log;
async function shared(){let n=1;const a=async()=>{await 0;return ++n};const b=async()=>{await 0;return n};const h=a();n=10;print('shared '+await h+' '+await b()+' '+n)}
async function boxed(){const cell={n:1};const a=async()=>{await 0;return ++cell.n};const b=async()=>{await 0;return cell.n};const h=a();cell.n=10;print('boxed '+await h+' '+await b()+' '+cell.n)}
async function loops(){const fs=[];for(let i=0;i<3;i++)fs.push(async()=>{await 0;return i});print('loop '+(await Promise.all(fs.map(f=>f()))).join(','));const gs=[];for(let i=0;i<3;i++){const cell={i};gs.push(async()=>{await 0;return cell.i})}print('cell-loop '+(await Promise.all(gs.map(f=>f()))).join(','));}
async function tdz(){let f;{f=async()=>n;try{await f()}catch(e){print('tdz '+e.name)}let n=4;print('initialized '+await f())}}
shared().then(boxed).then(loops).then(tdz);
```


## Implementation

### Checked captures and shared form

An async arrow accepts immutable captures and rejects mutable captures with S009.
The mutable diagnostic says: "copy it into a `const` first, or use a class with a field".
A capture of `this` stays S009.
An owned async environment cannot store a transitive borrowed synchronous closure (§118).
The checker rejects both an escaped arrow and a started handle that retains such a closure.
Direct counted captures bypass §175; transitive borrowed capture facts retain their block restrictions.
A counted `map` callback result stays S014.

Each capturing async arrow creates a hidden reference class in LIR.
The class has one field per captured binding, with ordinary class layout and counted-field release descriptions.
Counted field stores acquire counts through the existing class store operations.
The callable pair holds the code and the allocated environment pointer.
`OwnedEnvironment` identifies the frame parameter in LIR.
JIT, C AOT, and interpreter frame parameters root that pointer through completion.
Async callable wrappers pass the environment to their creators.
Borrowed synchronous closure storage still uses byte relocation; owned environments keep their allocation pointers.
The interpreter packs the same function-id and environment-pointer pair in fields and arrays.
Explicit collection releases unreachable environments and their counted fields.

### Corpus and independent checks

At `a16ba016`, a353 failed with S009 for captured `base`, `h`, `n`, `b`, and `s`.
The pin diagnostic read: "async arrow captures `base`; an async arrow captures nothing".
At that pin, r398 failed at line 9 with the old S009 message for `n`.
TypeScript 5.9.2 accepts both entries.
Node v24.18.0 matches all ten output lines of a353.
The Node shim supplies a completed Context suspension and a collection operation with no output.
That comparison checks output; the three-tier count test checks collection.

C24 row 36 names `a336`, `a353`, `r379`, and `r398`.
The existing r379 now captures a mutable binding.
The new a353 covers direct, held, passed, field, loop, handle, reference, and string captures.
Its synchronous starter returns a handle after the callable local exits.
The arrow then suspends, collects, and reads its captured handle.

The independent HIR/LIR witness compares hidden class fields against HIR captures, ownership releases, and allocation sites.
The text golden adds only a353; other corpus text remains byte-identical.
The document generator updates the corpus index and language reference.
No existing `.expected` file changes.

The collection test creates 20 and 100 environments with completed captured handles.
After collection, the unreachable cases report zero tasks in each tier.
The reachable controls report 20 and 100 tasks in each tier.
The four cases take 2.11 seconds in total, with four interpreter runs, four JIT sessions, and four C builds.
The new corpus comparison uses one execution per tier.
The new verifier case uses one checker/lowering call and no native build.
The checker ownership cases use four checker calls and no native build.
The reload cases use four JIT sessions; the layout-changing cases take 0.19 seconds in total.
An old value runs its old body after reload, including a changed capture layout and collection before its call.
A suspended call traps with `StaleCoroutine`; the no-reload control prints `8`.

### Integrated release cost

Each standalone binary runs alone with at least three warmups, a 200-millisecond warmup floor, and eleven timed samples.
The table gives each run median and the lowest median of three runs.
Each span includes a fresh Context, initialization, execution, checkpoints, and Context release.
Compilation stays outside the span.
The creation-and-call inputs match the appendix workloads and print `50005000`.
Both forms report one checkpoint, zero unfinished tasks, 10,001 live allocations, and 160,016 live payload bytes before release.
No threshold applies.

| Form | Run 1, ns | Run 2, ns | Run 3, ns | Best median, ns | Best / 10,000 |
|---|---:|---:|---:|---:|---:|
| Integrated async arrow | 731,000 | 727,000 | 785,000 | 727,000 | 72.7 ns |
| Handwritten class | 851,000 | 837,000 | 950,000 | 837,000 | 83.7 ns |

The measured arrow/class ratio is 0.8686.
The body has no internal await; these numbers do not measure a suspended body.

The first implementation measures the §94 workloads before the function-value root change, against `f561804d`.
Each binary uses the same timing driver and the same warmup and sample counts.

| Workload | Implementation run medians, ns | Pin run medians, ns | Best implementation / best pin |
|---|---|---|---:|
| settled-awaits | 13,200,000 / 13,228,000 / 13,149,000 | 13,187,000 / 13,096,000 / 13,287,000 | 1.0040 |
| held-handles | 3,846,000 / 3,840,000 / 3,838,000 | 3,874,000 / 3,881,000 / 3,859,000 | 0.9946 |
| deep-chains | 9,282,000 / 9,295,000 / 9,282,000 | 9,293,000 / 9,459,000 / 9,253,000 | 1.0031 |

## Implementation: function-value roots

`HandleKind::Func` and `HandleKind::NullableFunc` expose a managed environment word through `contains_managed()`.
They remain pairs, rather than allocation handles.
The common layout derives a two-word root range from their 16-byte C layout.
The collector scans the environment word and ignores a code word that does not name a Context allocation.
Value-class fields, fixed-array elements, and iterator results inherit this fact through the common containment description.

The fact reaches these native root registration sites:

- `root_storage::plan_with_interference`: SSA temporaries, block parameters, and synchronous function parameters.
- JIT `initialize_storage`: activation-local shadow ranges and temporary shadow ranges.
- JIT `initialize_global_roots`: module-global root ranges.
- C `Body::new` and `emit_declarations`: activation locals and temporary shadow frames.
- C `emit_init`: module-global root ranges.
- JIT `plan_coroutine` and C `emit_frame_type`: parameter, local, and suspension storage within collector-scanned coroutine payloads.
- Runtime class, array, and Map payload scans: stored function pairs retain their environment allocations.

Root registrations exist in native storage plans, after LIR.
`verify_function_storage` compares the LIR function type against the byte range of a native root registration.
The comparison requires both words of the pair.
The shared shadow-value plan checks every function-value temporary and parameter.
Each native tier also checks its activation-local and module-global registrations.
The negative test constructs a native plan with function-value storage and no root slots.
It reports `function-value storage has no complete environment root`.
The valid control accepts the complete plan.
The existing independent frame check compares LIR parameters, locals, and suspension operands against C fields and Cranelift stores.
The earlier `OwnedEnvironment` parameter-kind check is removed.

Each holder test runs one interpreter execution, one JIT session, and one C build.
Each stores a scalar-capture arrow and a reference-capture arrow.
Collection precedes 100 allocations of four-byte `J` objects and eight-byte `K` objects.
Those sizes match the scalar and reference environment payloads.
Each test prints `42`, then `42`.
The async-local test suspends before collection.
The reference factory returns before collection, so its local cannot retain the captured object.

The scratch control removes the managed function-kind fact and disables the new registration check.
It retains the owned-environment implementation and both native lowerings.
The interpreter passes all six holders.
The native results are:

| Holder | Fixed JIT and C AOT | Scratch JIT and C AOT |
|---|---|---|
| Synchronous local | `42`, `42` | `1001`, then SIGSEGV |
| Async local across await | `42`, `42` | `1001`, then SIGSEGV |
| Synchronous parameter | `42`, `42` | `1001`, then SIGSEGV |
| Module global | `42`, `42` | `1001`, then SIGSEGV |
| Field | `42`, `42` | `42`, `42` |
| Array element | `42`, `42` | `42`, `42` |

Fields and arrays already expose the environment through their conservative payload scans.
Their unchanged scratch results are controls; they are not new failures.
A separate mixed-environment test copies borrowed and owned arrows across suspension and collection.
It prints `42`, then `42`, in all three tiers.

The borrowed-environment predicate remains necessary: a root does not convert borrowed stack bytes into an owned allocation.
The largest accept source is `a204-static-long-string.ts`, at 130,516 bytes.
Its emitted C contains 353,416 bytes, including an 80-byte constant borrowed-environment helper.
This module contains no capturing synchronous lambda and no owned environment.
Both native tiers add zero code-identity comparisons for this entry.
The optimized C object has 65,531 text bytes with the helper and without it.
The object contains no borrowed-environment helper symbol.
These sizes do not measure a module with many mixed closure sites.
The owned-environment corpus entry `a353` has 1,895 source bytes and 84,588 emitted C bytes.
It also contains an 80-byte constant helper, with zero code-identity comparisons.
Its C object has 11,851 text bytes with the helper and without it.
The mixed-environment test covers the branch that selects borrowed byte copies instead of owned allocation pointers.

### Root-change release measurements

Each release driver runs alone, after all crate tests and builds finish.
Each subject discards at least three warmups and 200 milliseconds of measured execution.
Each run then measures eleven samples and reports their median.
The comparison uses the lowest median from three runs, against `f561804d`.
The pin checkout matches all 573 tracked compiler, codegen, runtime, boundary, and benchmark source files checked against that revision.
Compilation and linking stay outside the measured span.
The ten ordinary workloads time execution only; the three async workloads time a fresh Context through release.

| Async workload | Root-change medians, ns | Pin medians, ns | Best ratio |
|---|---|---|---:|
| settled-awaits | 13,166,000 / 13,040,000 / 13,270,000 | 13,256,000 / 13,093,000 / 13,145,000 | 0.9960 |
| held-handles | 3,855,000 / 3,821,000 / 3,879,000 | 3,834,000 / 3,850,000 / 3,837,000 | 0.9966 |
| deep-chains | 9,361,000 / 9,284,000 / 9,337,000 | 9,255,000 / 9,292,000 / 9,260,000 | 1.0031 |

| Ordinary workload | Tier | Root-change medians, ms | Pin medians, ms | Best ratio |
|---|---|---|---|---:|
| fib-recursive | ship | 3.730 / 3.737 / 3.726 | 3.724 / 3.730 / 3.723 | 1.0008 |
| fib-recursive | jit | 8.128 / 8.123 / 8.122 | 8.125 / 8.134 / 8.140 | 0.9996 |
| fib-loop | ship | 30.922 / 30.941 / 30.058 | 30.114 / 30.115 / 30.084 | 0.9991 |
| fib-loop | jit | 73.003 / 73.269 / 73.272 | 73.280 / 73.347 / 73.113 | 0.9985 |
| mandelbrot | ship | 124.583 / 124.327 / 124.454 | 124.426 / 124.353 / 124.598 | 0.9998 |
| mandelbrot | jit | 133.162 / 133.131 / 133.331 | 129.775 / 129.676 / 129.547 | 1.0277 |
| primes | ship | 21.135 / 21.158 / 21.105 | 21.121 / 21.141 / 21.147 | 0.9992 |
| primes | jit | 31.793 / 31.816 / 31.820 | 31.795 / 31.807 / 31.814 | 0.9999 |
| sort | ship | 18.112 / 18.080 / 18.073 | 18.099 / 18.072 / 18.063 | 1.0006 |
| sort | jit | 34.490 / 34.487 / 34.507 | 34.518 / 34.448 / 34.524 | 1.0011 |
| tree | ship | 101.740 / 111.468 / 101.696 | 101.716 / 101.833 / 101.901 | 0.9998 |
| tree | jit | 406.701 / 402.032 / 402.335 | 404.545 / 404.531 / 408.375 | 0.9938 |
| queen | ship | 25.898 / 25.692 / 25.762 | 25.761 / 25.767 / 25.755 | 0.9976 |
| queen | jit | 35.695 / 35.683 / 35.621 | 35.675 / 35.790 / 35.858 | 0.9985 |
| particles | ship | 74.437 / 74.402 / 74.464 | 74.405 / 74.428 / 74.440 | 1.0000 |
| particles | jit | 446.769 / 453.154 / 453.554 | 443.408 / 444.212 / 441.944 | 1.0109 |
| callbacks | ship | 36.946 / 36.950 / 41.749 | 36.858 / 36.908 / 36.933 | 1.0024 |
| callbacks | jit | 256.782 / 256.803 / 256.724 | 257.795 / 257.806 / 257.815 | 0.9958 |
| collect | ship | 35.022 / 34.738 / 34.908 | 35.294 / 34.970 / 35.253 | 0.9934 |
| collect | jit | 114.350 / 114.600 / 113.520 | 114.449 / 114.619 / 112.995 | 1.0046 |

All measured workloads preserve their checksums.
The cross-language driver reports no excessive interquartile spread in any run.
No performance threshold applies to this comparison.

### Root-change checks

| Check | Measured result |
|---|---|
| Compiler and codegen crate suites, with corpus comparisons in all three tiers | 1,998 passed; two existing performance tests ignored |
| Final async function-value suite | 18 passed |
| Missing-root native-form test | Passed, with a valid registration control |
| `cargo fmt --check` | Exit 0 |
| `cargo clippy --workspace --all-targets` | Exit 0; existing warnings at unchanged sites |
| `cargo build --offline --locked --workspace --all-targets` | Exit 0; no warning line |

## Implementation: reload predicate

The dev JIT decides that a function value carries a borrowed environment only when its code word equals the code address of a synchronous function with a `Capture` parameter in the current generation.
The decision does not depend on whether the current generation has an owned-environment function.
An old async arrow value that survives a reload (rule 7) therefore keeps its owned environment word; no store copies it into shadow or frame bytes.
The C emitter uses the same code-word comparison.

`old_owned_arrow_survives_reload_without_current_owned_environment` (`codegen/tests/async_function_values.rs`) installs an async arrow that captures a reference object and suspends, then reloads a module that keeps a capturing synchronous lambda and has no capturing async arrow.
It prints `42` after allocations that follow the reload.
The same-shape controls (no reload; a new module that also has a capturing async arrow; a new module with no capturing synchronous lambda) print `42`.
Before the predicate change, the reload case failed and the controls passed.

`verify_function_storage` (`codegen/src/root_storage.rs`) now compares each storage type with the number of managed words that the shared layout counts for it, so a `FixedArray` of function values needs a root registration for every element pair.
A negative test builds a `FixedArray` of function values with no root slots and reads the message.
