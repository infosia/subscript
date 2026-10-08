# §182 Diagnostic message measurement

HEAD: `93453f87`. Stock TypeScript: `5.9.2`. Every TypeScript measurement includes `prelude/lang.d.ts`.

TypeScript options: `strict`, `noEmit`, `target: ES2022`, `module: ESNext`, `moduleResolution: Bundler`, and `forceConsistentCasingInFileNames`.

The library set is `ES2022` and `ESNext.Disposable`. The `types` list is empty.

The diagnostic blocks below preserve the exact message, rule, examples, and reason. They omit file paths and source displays.

Each accepted result means CLI exit 0 with `no errors`, and TypeScript exit 0 with no output.

The section review includes §154, §171, §172, §173, §175, and §181.

The §154 total tests compare witness messages and variants. They also test executable fragments.

`compiler/src/check/rejection_total.rs:188` compares messages. Lines 78 and 227 test fragments and block presence.

These tests do not require each example or reason to describe the form at each rejection site.

This measurement proposes diagnostic and test-name changes only. It does not propose a language-rule change.

## 1. §157.3 item 1: `this` in a static-method lambda

**Program** (`static-bare.ts`):

```typescript
class C { static f(): void { const g = () => this; } }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S100]: a static method must name its class instead of `this`; use `ClassName.member`
  = rule: Constructs outside the decided language surface are rejected.
  = TypeScript accepts:
  |   class C { static x:i32=0; static f():i32 { return this.x; } }
  = subscript:
  |   class C { static x:i32=0; static f():i32 { return C.x; } }
  = why: A static method has no instance receiver; it must name its class explicitly. (collisions.md C24)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** §173.1 rule 5 fixes the wrong reason at HEAD. The diagnostic now names the static-method restriction.

The corrected message is the current message. A member access also gives ``a static method must name its class instead of `this` ``.

**Accepted proposed form** (`static-fix.ts`):

```typescript
class C { static n: i32 = 1; static f(): void { const g = (): i32 => C.n; } }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/expr/entry.rs:350`, site `ThisInStaticMethodArrow`; member access uses the static-`this` check.

The example table is `compiler/src/divergence/type_flow.rs:42`, entry `THISSTATICMETHODMEMBER`.

**Text tests:** `compiler/src/check/rejection_fact_tests.rs:168`, `static_arrow_this_names_the_class_remedy`, asserts the exact bare-`this` message.

The §154 total test also reads this site’s messages in `compiler/src/check/rejection_targets.txt`.

## 2. §170.3 item 3: S009 in a generator body

**Program** (`group-generator-small.ts`):

```typescript
function* values(): Generator<i32> { new TaskGroup(); yield 1; }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S009]: TaskGroup is not allowed in a generator body
  = rule: A capturing lambda stays in its defining function and each captured counted binding block; a task group requires one lexical owner.
  = TypeScript accepts:
  |   class Holder { group: TaskGroup; constructor(group: TaskGroup) { this.group = group; } } export function main(): void {}
  = subscript:
  |   export async function main(): Promise<void> { const group: TaskGroup = new TaskGroup(); await group.join(); }
  = why: A group needs a lexical release: array counts miss removals, fields end only at collection, and dropped generators have no scope exit. (collisions.md C24)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. The current rule includes a task-group clause, but it still starts with an unrelated lambda restriction.

The TypeScript example uses a field. It does not show the rejected generator form.

The current subscript example is accepted. The generator witness contains no lambda and TypeScript accepts it.

**Proposed rule:** `A generator body cannot use a TaskGroup because a dropped iterator has no scope exit.`

**Proposed reason:** `A dropped generator does not execute a lexical scope exit. Put the group in an async function and await its join.`

Use the measured generator program for the TypeScript example. Use the next program for the subscript example.

**Accepted proposed form** (`group-fix.ts`):

```typescript
async function values(): Promise<void> { const g = new TaskGroup(); await g.join(); }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/expr/task_group.rs:46` and `compiler/src/check/expr/entry.rs:188`, site `TaskGroupGeneratorBody`.

`compiler/src/diag.rs:119` supplies the shared rule. `compiler/src/divergence/entries.rs:42` supplies `TaskGroupPosition`.

**Text tests:** the §154 total test reads the exact site message in `compiler/src/check/rejection_targets.txt:1414`.

`compiler/tests/task_group.rs`, `generator_groups_reject_with_an_async_body_control`, asserts S009 and accepts an async control.

That test does not assert the rule line or the example’s generator shape.

## 3. §167.3 items 1 and 3: cascade and reasons

### Handle expression body

**Program** (`handle-body.ts`):

```typescript
async function value(n: i32): Promise<i32> { return n; } async function probe(): Promise<void> { const f = async () => value(7); print(`${await f()}`); }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S100]: type mismatch: the return value expects `i32`, got `Promise<i32>`
  = rule: Constructs outside the decided language surface are rejected.
  = TypeScript accepts:
  |   async function f(h: Promise<i32>): Promise<i32> { return h; }
  = subscript:
  |   async function f(h: Promise<i32>): Promise<i32> { return await h; }
  = why: An async return carries its fulfilled value. The language has no implicit handle adoption. (compiler.md §167)
error[S100]: type `Promise<i32>` cannot be interpolated into a template
  = rule: Constructs outside the decided language surface are rejected.
  = TypeScript accepts:
  |   class C { x: i32 = 1; }
  |   export function main(): void { print(`${new C()}`); }
  = subscript:
  |   no equivalent; join the array or call a formatting method
  = why: Interpolation formats scalars, strings, enums, and literal aliases. (collisions.md C24)
error: 2 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. TypeScript adopts the returned Promise. The first rejection states this language’s explicit-await rule correctly.

The second rejection follows an incorrect inferred callable result. The checker keeps `Promise<i32>` as the fulfilled result, then adds another handle.

The later await therefore produces `Promise<i32>` instead of `i32`. The template diagnostic reports the consequence of the first error.

**Proposed message:** keep ``type mismatch: the return value expects `i32`, got `Promise<i32>` `` as the only diagnostic.

Use `await value(7)` in the expression body. Give the rejected callable an error result to suppress the later template diagnostic.

**Accepted proposed form** (`handle-fix.ts`):

```typescript
async function value(n: i32): Promise<i32> { return n; } async function probe(): Promise<void> { const f = async () => await value(7); print(`${await f()}`); }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/expr/lambda.rs:328` stores the inferred handle result; line 338 checks the async return.

Line 430 wraps that result in another `AsyncHandle`. `compiler/src/check/type_rules.rs:260` emits `AsyncReturnHandle`.

`compiler/src/check/expr/literal.rs` emits the later `TemplateInterpolationKind` diagnostic.

`compiler/src/divergence/surface_forms.rs:629`, `ASYNC_RETURN_HANDLE`, supplies the first block.

**Text tests:** `compiler/tests/async_function_values.rs:100`, `handle_returns_reject_with_explicit_await_controls`, asserts the message and one diagnostic.

Its programs do not consume the rejected callable later. The measured cascade escapes that assertion.

### Async capture rule

**Program** (`mutable-capture.ts`):

```typescript
function f(): void { let n: i32 = 1; const job = async (): Promise<i32> => n; }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S009]: async arrow captures mutable `n`; copy it into a `const` first, or use a class with a field
  = rule: A capturing lambda stays in its defining function and each captured counted binding block; a task group requires one lexical owner.
  = TypeScript accepts:
  |   function f(): void { let n: i32 = 1; const job = async (): Promise<i32> => n; }
  = subscript:
  |   function f(): void { const job = async (n: i32): Promise<i32> => n; }
  = why: An async arrow owns immutable captures. A mutable binding needs an explicit const copy or a class field. (collisions.md C24)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** partly fixed by §181.1 rule 1. The message and reason now permit immutable captures and name a const copy.

The shared S009 rule still describes a synchronous lambda’s scope restriction. This program contains no escape.

**Proposed rule:** `An async arrow can own const captures. Copy a mutable value into a const or use a class field.`

**Accepted proposed form** (`mutable-copy-fix.ts`):

```typescript
function f(): void { let n: i32 = 1; const copy = n; const job = async (): Promise<i32> => copy; }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Accepted proposed form** (`mutable-class-fix.ts`):

```typescript
class Job { n: i32; constructor(n: i32) { this.n = n; } async run(): Promise<i32> { return this.n; } } function f(): void { let n: i32 = 1; const job = new Job(n); }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/lookup.rs:254` and `compiler/src/check/capture.rs:472`, site `AsyncArrowCapture`.

`compiler/src/diag.rs:119` supplies the rule. `compiler/src/divergence/surface_forms.rs:619` supplies `ASYNC_ARROW_CAPTURE`.

**Text tests:** `compiler/tests/async_function_values.rs:46` reads the message’s binding name and const-copy suggestion.

The §154 total test reads the exact mutable-capture witness message. Neither test asserts that the shared rule fits a non-escape.

### Indirect await reason

**Program** (`await-indirect-string.ts`):

```typescript
async function probe(): Promise<void> { await ((): string => "x")(); }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S100]: await requires a call that returns an async handle
  = rule: Constructs outside the decided language surface are rejected.
  = TypeScript accepts:
  |    async function probe(): Promise<void> { await (() : i32 => 1)(); }
  |   export function main(): void {}
  = subscript:
  |   no equivalent; await a call that returns a handle
  = why: The call returns an integer, so it supplies no completion for an await. (compiler.md §167)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. This call returns `string`. The reason says that it returns an integer.

A local function identifier uses `AwaitNonHandle`, whose reason already says `a synchronous value carries no completion`.

The immediate-arrow call reaches `AwaitIndirectCall`, so the wrong reason remains reachable.

**Proposed reason:** `The call returns a synchronous value. It supplies no async completion for an await.`

**Proposed subscript guidance:** `Call the synchronous function without await, or await a function that returns an async handle.`

**Accepted proposed form** (`await-indirect-fix.ts`):

```typescript
async function probe(): Promise<void> { ((): string => "x")(); }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Accepted proposed form** (`await-handle-fix.ts`):

```typescript
async function probe(): Promise<void> { const f = async (): Promise<string> => "x"; await f(); }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |

**Source:** `compiler/src/check/expr/entry.rs:699`, site `AwaitIndirectCall`; another emitter appears at line 940.

`compiler/src/divergence/builtin_calls.rs:300`, `AWAIT_INDIRECT_CALL`, supplies the wrong reason.

**Text tests:** the §154 total test reads the message in `compiler/src/check/rejection_targets.txt:527` and tests the integer fragment.

It does not assert the reason on a string result.

## 4. §149.3 item 2: a generic function as a value

**Program** (`generic-value.ts`):

```typescript
function id<T>(x: T): T { return x; } function apply(f: (x: i32) => i32, x: i32): i32 { return f(x); } export function main(): void { apply(id, 3); }
```

**Current diagnostic text**:

```text
error[S100]: generic function `id` has no first-class value; call it directly or use a lambda
  = rule: Constructs outside the decided language surface are rejected.
  = TypeScript accepts:
  |   function id<T>(x:T):T{return x;} function apply<T>(f:(x:T)=>T,x:T):T{return f(x);} export function main():void { apply(id,3); }
  = subscript:
  |   no equivalent; call the function directly or use a lambda
  = why: Generic function values require instantiation outside the admitted inference surface. (compiler.md §149.1)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** §154.2 acceptance 7 fixes the nonexistent remedy at HEAD. The current message asks for a direct call or a lambda.

The corrected message is the current message. The lambda form below demonstrates its executable remedy.

**Old suggested form** (`generic-explicit.ts`):

```typescript
function id<T>(x: T): T { return x; } function apply(f: (x: i32) => i32, x: i32): i32 { return f(x); } export function main(): void { apply(id<i32>, 3); }
```

**Current diagnostic text**:

```text
error[S100]: expression form outside the decided surface
  = rule: Constructs outside the decided language surface are rejected.
  = TypeScript accepts:
  |   function id<T>(x: T): T { return x; } function f(): void { id<i32>; }
  = subscript:
  |   no equivalent; use Math.pow, an explicit comparison, or a null check
  = why: No lowering is decided for this operator or expression form. (collisions.md C24)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

The old spelling still fails. TypeScript accepts it, but this language rejects the instantiation expression.

**Accepted proposed form** (`generic-fix.ts`):

```typescript
function id<T>(x: T): T { return x; } function apply(f: (x: i32) => i32, x: i32): i32 { return f(x); } export function main(): void { apply((x: i32): i32 => id<i32>(x), 3); }
```

CLI: accepted. TypeScript: accepted.

**Accepted proposed form** (`generic-direct-fix.ts`):

```typescript
function id<T>(x: T): T { return x; } export function main(): void { id<i32>(3); }
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |

**Source:** `compiler/src/check/expr/literal.rs:435`, site `GenericFunctionValue`.

`compiler/src/divergence/surface_forms.rs:385`, `GENERICFUNCTIONVALUE`, supplies its block.

**Text tests:** the §154 total test reads the exact current message in `compiler/src/check/rejection_targets.txt:413`.

Its fragment test permits the `no equivalent; ...` guidance without an executable remedy.

## 5. §153.3 item 2: a Date method value

**Program** (`date-value.ts`):

```typescript
export function main(): void { const d = new Date(0); const g = d.getTime; }
```

**Current diagnostic text**:

```text
error[S014]: `getTime` may only be called, not read as a value (Q20)
  = rule: Out-of-subset standard-library use and arithmetic on storage-only `f16` are rejected.
  = TypeScript accepts:
  |   const held = Array;
  = subscript:
  |   const xs: i32[] = [1]; const copy: i32[] = Array.from(xs);
  = why: Compiler-owned namespaces and methods lower to direct operations; the language has no value or writable storage for them. (stdlib.md §9.0)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. The diagnostic rejects a Date method value. Its examples instead show the Array namespace and Array.from.

The current suggested program is accepted, but it does not preserve the Date operation.

`stdlib.md` §9.0 describes the Array namespace and its source forms. It does not describe Date methods.

`stdlib.md` §3 states the Date representation and methods. `collisions.md` C24 row 12 states the method-value restriction.

**Proposed message:** keep the current rejection line.

**Proposed reason:** `A Date method lowers to a direct operation. Use a lambda that calls the method on the Date value.`

Cite `stdlib.md §3` and C24 row 12. Use the measured Date program for the TypeScript example.

**Accepted proposed form** (`date-fix.ts`):

```typescript
export function main(): void { const d = new Date(0); const g = (): i64 => d.getTime(); g(); }
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/expr/member.rs:726`, site `DateMethodValue`.

`compiler/src/check/rejection_surface_classes.rs:260` maps it to `CompilerOwnedValue`.

`compiler/src/divergence/established_tail.rs:74`, `COMPILEROWNEDVALUE`, supplies the unrelated examples and citation.

**Text tests:** the §154 total test reads the exact Date message in `compiler/src/check/rejection_witnesses.txt:483`.

It tests the shared Array fragments. It does not require a Date-specific example or citation.

## 6. §143.3 item 2: the function-array type

**Program** (`function-map.ts`):

```typescript
function id(x: i32): i32 { return x; } export function main(): void { [1].map((x: i32): ((x: i32) => i32) => id); }
```

**Current diagnostic text**:

```text
error[S014]: `map` produces a `(i32) => i32[]`; `(i32) => i32` is outside the supported element kinds (Q22)
  = rule: Out-of-subset standard-library use and arithmetic on storage-only `f16` are rejected.
  = TypeScript accepts:
  |   const value: i32 = 1; value.toFixed(2);
  = subscript:
  |   const value: i32 = 1; const text: string = (value as f64).toFixed(2);
  = why: Each method has a fixed receiver, element, result, and accumulator domain; TypeScript generic method domains include more kinds. (stdlib.md §9)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. `(i32) => i32[]` denotes a function that returns an array. The rejected map result is an array of functions.

The correct spelling is `((i32) => i32)[]`. The current Number examples do not describe this map result.

**Proposed message:**

```text
`map` produces a `((i32) => i32)[]`; `(i32) => i32` is outside the supported element kinds (Q22)
```

Build the displayed array type with the type printer. Use the measured map program for the TypeScript example.

**Proposed reason:** `map does not support a function result. Use a typed array and push each function in a for-of loop.`

**Accepted proposed form** (`function-map-fix.ts`):

```typescript
function id(x: i32): i32 { return x; } export function main(): void { const fs: ((x: i32) => i32)[] = []; for (const x of [1]) { fs.push(id); } }
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/expr/method.rs:924`, site `ArrayMapResult`. The emitter appends `[]` to `type_name(&u)`.

`compiler/src/check/rejection_surface_classes.rs:272` maps it to `MethodTypeDomain`.

`compiler/src/divergence/established_tail.rs:109`, `METHODTYPEDOMAIN`, supplies the Number examples.

**Text tests:** the §154 total test reads a `Box[]` message at this site in `compiler/src/check/rejection_witnesses.txt:513`.

No located text assertion tests the parentheses for an array of functions.

## 7. §175.4 item 3: the test name “owns”

**Program** (`loop-borrow.ts`):

```typescript
async function work(): Promise<i32> { return 7; } export function main(): void { let f: () => Promise<i32> = work; for (const h of [work()]) { f = () => h; } }
```

**Current diagnostic text**:

```text
error[S009]: captured binding `h` is released when its block exits; local `f` is declared outside that block
  = rule: A capturing lambda stays in its defining function and each captured counted binding block; a task group requires one lexical owner.
  = TypeScript accepts:
  |   async function work(): Promise<i32> { return 7; } export function main(): void { let f: () => Promise<i32> = work; { const h = work(); f = () => h; } }
  = subscript:
  |   async function work(): Promise<i32> { return 7; } export function main(): void { { const h = work(); let f: () => Promise<i32> = () => h; } }
  = why: A lambda borrows a captured counted binding. Its block exit releases that binding, so a local outside the block cannot receive the lambda. (collisions.md C5)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. `each_loop_iteration_owns_its_counted_bindings` includes a for-of binding that borrows an array element.

§171.1 rule 2 gives the array one count for each element. §175.4 item 2 also records the borrowed-binding message defect.

The rejection remains a block restriction. The measured name must not claim that the iteration binding owns an element count.

**Proposed test name:** `each_loop_iteration_limits_its_counted_captures_to_its_block`.

The test name requires no language change. Its accepted control stays inside the iteration block.

**Accepted proposed form** (`loop-fix.ts`):

```typescript
async function work(): Promise<i32> { return 7; } export function main(): void { for (const h of [work()]) { let f: () => Promise<i32> = () => h; } }
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/tests/counted_capture_block.rs:64` defines the test name.

`compiler/src/check/capture/blocks.rs:172`, site `CaptureOutlivesBlock`, emits the measured diagnostic.

`compiler/src/divergence/entries.rs` supplies its `CaptureOutlivesBlock` examples. `compiler/src/diag.rs:119` supplies the rule.

**Text tests:** `compiler/tests/counted_capture_block.rs:14` asserts the exact rejection message for this test.

The test runner uses the name as an identifier. No located assertion reads the name as evidence of count ownership.

## 8. S013: Promise.then and Promise.catch examples

**Program** (`promise-then-small.ts`):

```typescript
function cb(v: i32): void {} async function probe(h: Promise<i32>): Promise<void> { h.then(cb); await h; }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S013]: Promise combinator `.then(...)` is not in the language
  = rule: Promise constructors and unsupported combinators are not in the language; every async handle must have an awaited completion.
  = TypeScript accepts:
  |   const pending = Promise.resolve(1);
  = subscript:
  |   async function leaf(): Promise<i32> { return 1; }
  |   async function probe(): Promise<void> { const value: i32 = await leaf(); }
  = why: Only async handles and Promise.all over handle arrays exist. Other Promise object operations have no runtime representation. (collisions.md C8)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Program** (`promise-catch-small.ts`):

```typescript
function cb(e: Error): void {} async function probe(h: Promise<i32>): Promise<void> { h.catch(cb); await h; }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S013]: Promise combinator `.catch(...)` is not in the language
  = rule: Promise constructors and unsupported combinators are not in the language; every async handle must have an awaited completion.
  = TypeScript accepts:
  |   const pending = Promise.resolve(1);
  = subscript:
  |   async function leaf(): Promise<i32> { return 1; }
  |   async function probe(): Promise<void> { const value: i32 = await leaf(); }
  = why: Only async handles and Promise.all over handle arrays exist. Other Promise object operations have no runtime representation. (collisions.md C8)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. Both TypeScript examples show Promise.resolve. Neither example reaches the measured combinator site.

The subscript example awaits a leaf but omits the callback. It gives no catch translation.

The final `await h` in each witness prevents an unrelated unawaited-handle diagnostic. TypeScript accepts both witnesses.

**Proposed messages:** keep the two rejection lines. Give `.then` and `.catch` separate example entries.

**Proposed then reason:** `A handle has no then method. In an async function, await the handle and call the callback with its value.`

**Proposed catch reason:** `A handle has no catch method. In an async function, await it inside try and handle the error inside catch.`

Use the measured combinator witnesses for the TypeScript examples. The accepted forms below show the corresponding operations.

**Accepted proposed form** (`promise-then-fix.ts`):

```typescript
async function leaf(): Promise<i32> { return 1; } function cb(v: i32): void {} async function probe(): Promise<void> { const h = leaf(); const v = await h; cb(v); }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Accepted proposed form** (`promise-catch-callback-fix.ts`):

```typescript
async function leaf(): Promise<i32> { return 1; } function cb(e: Error): void {} async function probe(): Promise<void> { const h = leaf(); try { await h; } catch (e) { if (e instanceof Error) { cb(e); } } }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

The current Promise.resolve example fails with S013. Its current subscript example is accepted, but it omits the callback.

**Source:** `compiler/src/check/expr/call.rs:831`, site `PromiseCombinatorCall`.

`compiler/src/check/rejection.rs:1299` maps it to `PromiseObject`.

`compiler/src/divergence/established_tail.rs:442`, `PROMISEOBJECT`, supplies the shared examples.

**Text tests:** the §154 total test reads the exact `.then` message in `compiler/src/check/rejection_targets.txt:450`.

Its fragment test measures Promise.resolve. It does not require a then-specific or catch-specific example.

## 9. S009 async-this and S014 async-map remedies

### A const capture at HEAD

**Program** (`const-capture.ts`):

```typescript
function f(): void { const n: i32 = 1; const job = async (): Promise<i32> => n; }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

§181.1 rules 1–7 fix the rejection of const captures. S009 cannot say that every async arrow captures nothing.

### Direct this capture

**Program** (`async-this.ts`):

```typescript
class C { n: i32 = 1; f(): void { const job = async (): Promise<i32> => this.n; } }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S009]: async arrow captures `this`; an async arrow captures nothing
  = rule: A capturing lambda stays in its defining function and each captured counted binding block; a task group requires one lexical owner.
  = TypeScript accepts:
  |   function f(): void { let n: i32 = 1; const job = async (): Promise<i32> => n; }
  = subscript:
  |   function f(): void { const job = async (n: i32): Promise<i32> => n; }
  = why: An async arrow owns immutable captures. A mutable binding needs an explicit const copy or a class field. (collisions.md C24)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. The primary message says that an async arrow captures nothing. The accepted const witness disproves that sentence.

The block describes a mutable numeric binding, not a receiver. Its accepted parameter example does not preserve the receiver operation.

**Proposed message:**

```text
an async arrow cannot capture `this` directly; copy the receiver into a const or use a class field with an async method
```

**Proposed rule:** `An async arrow owns const captures. A direct this capture remains outside the accepted surface.`

**Proposed reason:** `The receiver must appear as an explicit const capture or a class field.`

Use the measured receiver program for the TypeScript example. Both accepted forms below preserve the receiver read.

**Accepted proposed form** (`async-this-copy.ts`):

```typescript
class C { n: i32 = 1; f(): void { const self = this; const job = async (): Promise<i32> => self.n; } }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Accepted proposed form** (`async-this-class.ts`):

```typescript
class Job { owner: C; constructor(owner: C) { this.owner = owner; } async run(): Promise<i32> { return this.owner.n; } } class C { n: i32 = 1; async f(): Promise<i32> { const job = new Job(this); return await job.run(); } }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/expr/entry.rs:275`, site `AsyncArrowCapture`.

`compiler/src/check/capture.rs:472` also handles receiver capture through nested lambdas.

`compiler/src/divergence/surface_forms.rs:619`, `ASYNC_ARROW_CAPTURE`, supplies the mutable-binding block.

`compiler/src/diag.rs:119` supplies the shared S009 rule.

**Text tests:** `compiler/tests/async_function_values.rs:46` reads `an async arrow captures nothing`.

`nested_receiver_capture_rejects_with_synchronous_controls` at line 129 asserts the exact receiver message.

These assertions need the proposed text if the message changes.

### map with an async callback

**Program** (`async-map.ts`):

```typescript
async function probe(): Promise<void> { const hs = [1].map(async (v: i32): Promise<i32> => v); await Promise.all(hs); }
export function main(): void {}
```

**Current diagnostic text**:

```text
error[S014]: `map` cannot carry a counted callback result (§171)
  = rule: Out-of-subset standard-library use and arithmetic on storage-only `f16` are rejected.
  = TypeScript accepts:
  |   const value: i32 = 1; value.toFixed(2);
  = subscript:
  |   const value: i32 = 1; const text: string = (value as f64).toFixed(2);
  = why: Each method has a fixed receiver, element, result, and accumulator domain; TypeScript generic method domains include more kinds. (stdlib.md §9)
error: 1 error(s)
```

CLI exit 1. TypeScript exit 0; no output.

**Status:** open. §181.1 rule 8 keeps the counted map result rejected. The diagnostic’s Number example gives no handle-array replacement.

Its current Number conversion is accepted, but it does not create or await the tasks.

§171.1 rule 5a rejects counted callback results. Its array rule accepts explicit push of each handle.

**Proposed message:** keep the current rejection line.

**Proposed reason:** `map cannot transfer a counted callback result. Use a for-of loop, push each handle, and await the handle array.`

Use the measured async-map program for the TypeScript example. Use the class and loop below for the subscript example.

**Accepted proposed form** (`async-map-fix.ts`):

```typescript
class Job { n: i32; constructor(n: i32) { this.n = n; } async run(): Promise<i32> { return this.n; } } async function probe(): Promise<void> { const hs: Promise<i32>[] = []; for (const v of [1]) { const job = new Job(v); hs.push(job.run()); } await Promise.all(hs); }
export function main(): void {}
```

CLI: accepted. TypeScript: accepted.

**Current example measurements**:

| Example in the diagnostic | CLI | TypeScript |
|---|---|---|
| TypeScript example | rejected | accepted |
| subscript example | accepted | accepted |

**Source:** `compiler/src/check/expr/method.rs:1774`, site `ArrayMapResult`, in `expect_callback_shape`.

`compiler/src/check/rejection_surface_classes.rs:272` maps it to `MethodTypeDomain`.

`compiler/src/divergence/established_tail.rs:109` supplies the unrelated Number examples.

**Text tests:** the §154 total test reads the exact counted-callback message in `compiler/src/check/rejection_witnesses.txt:1500`.

Its `ArrayMapResult` witness list includes §171 and §172 forms. The shared fragment test still measures the Number example.

No located assertion requires the class, handle push, or Promise.all example.


## Implementation

The implementation changes the listed diagnostic text and the §175 test name. Examples use fixed entries at their emission sites.

No message selects an example. The §154 witnesses and the language acceptance rules stay unchanged.

An inferred async expression body retains its intended `Promise<inner>` callable result after a rejected handle return.

Inference unwraps the apparent type only when it is an async handle; otherwise, it retains the written type. The `T extends Base` parameter and const-capture controls both preserve `Promise<T>`. A `T extends Promise<i32>` expression-body control rejects implicit handle adoption with one return error in the CLI; TypeScript accepts it. Its downstream await retains `i32` without a cascade.

The template consumer reports only the return error. The string assignment reports that error and its independent type mismatch.

TypeScript reports TS2322 for the string assignment.

The generator, mutable capture, and receiver capture diagnostics carry their proposed rule text.

Date guidance cites `stdlib.md §3` and `collisions.md C24 row 12`.

The type printer writes `((i32) => i32)[]` for the function-array map result.

The then, catch, and finally examples preserve their callbacks. The async-map example constructs and awaits a handle array.

The generated language reference includes the listed examples. No reject header quotes a changed message. No accept `.expected` file changes.

### Suggested-form results

Every row uses the TypeScript options and prelude stated above. Accepted means exit 0 with no errors.

| Suggested form | CLI | TypeScript |
|---|---|---|
| `group-fix.ts` | accepted | accepted |
| `handle-fix.ts` | accepted | accepted |
| `mutable-copy-fix.ts` | accepted | accepted |
| `mutable-class-fix.ts` | accepted | accepted |
| `await-indirect-fix.ts` | accepted | accepted |
| `generic-fix.ts` | accepted | accepted |
| `generic-direct-fix.ts` | accepted | accepted |
| `date-fix.ts` | accepted | accepted |
| `function-map-fix.ts` | accepted | accepted |
| `promise-then-fix.ts` | accepted | accepted |
| `promise-catch-callback-fix.ts` | accepted | accepted |
| `promise-finally-callback-fix.ts` | accepted | accepted |
| `async-this-copy.ts` | accepted | accepted |
| `async-this-class.ts` | accepted | accepted |
| `async-map-fix.ts` | accepted | accepted |
| Existing explicit-await return example | accepted | accepted |

The finally replacement (`promise-finally-callback-fix.ts`) is:

```typescript
async function leaf(): Promise<i32> { return 1; } function cb(): void {} async function probe(): Promise<void> { const h = leaf(); try { await h; } finally { cb(); } }
export function main(): void {}
```

### New test costs

The measurements include each test's first checker initialization. The remedies share one TypeScript batch. Both compiler tests live in the existing `async_function_values` binary; there is no additional test binary to link or launch.

| Test | Cost (ms) | Work |
|---|---:|---|
| `listed_diagnostics_render_the_contract_text` | 4.770 | Twelve checker calls and rendered text comparisons |
| `rejected_handle_body_keeps_the_intended_consumer_type` | 3.976 (previous four-call measurement) | Five checker calls; template and string consumers, two `T extends Base` accepted controls, and one `T extends Promise<i32>` rejected control |
| `listed_diagnostic_remedies_pass_cli_and_typescript` | 1290.013 | Sixteen CLI checks and one TypeScript batch |

The text test reads each message, rule, TypeScript example, subscript example, and reason.

The consumer test preserves a real downstream type error. The remedy test proves the suggested forms pass both checkers.

The finally replacement awaits the handle inside try and calls the callback inside finally (§180).

The indirect-await example uses the same i32 lambda on both sides. Its subscript form omits await.

The remedy controls retain `mutable-class-fix` because the mutable capture message names a class field, `generic-direct-fix` because the generic value message names a direct call, and `async-this-class` because the receiver message names a class field with an async method. The unmentioned `await-handle-fix` and `loop-fix` forms are removed.
