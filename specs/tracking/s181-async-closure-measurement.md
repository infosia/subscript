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
