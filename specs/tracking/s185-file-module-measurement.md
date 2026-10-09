# §185: A file module that the host enables

Measurement pin: `e71340a0cdca00c377e748df8bab74c5de8e19e4`.
Platform: aarch64 macOS. Date: 2026-10-09.
The Rust measurements use release builds. The C measurements use the optimized shipping build.

This is a Step 0 measurement, under the revised standalone-runtime non-goal in `CLAUDE.md`.
The prototype adds file access to the present computation-only library scope.
It uses host I/O and existing §178/§184 completion functions. It adds no runtime I/O thread.
The prototype does not establish a contract. All production changes and the measurement runner were reverted.
The remaining repository change is this note. No commit was made.

## 1. Present standard-library surface

`prelude/lang.d.ts` declares the ambient `Context` namespace, a `JSON` interface augmentation, and the `Worker` class.
Stock ES2022 supplies the JSON value. The prelude adds its generic `parse<T>` signature.
The prelude adds Worker declarations because the selected TypeScript libraries do not supply this Worker surface.
The prelude directory contains only `prelude/lang.d.ts`. It supplies no importable standard-library module.
The compiler owns builtin identities. It does not obtain their implementations or signatures by ingesting this prelude as a C mirror.

| Stage | `Context.collect()` witness | Host completion witness |
|---|---|---|
| Declaration | `prelude/lang.d.ts`, `declare namespace Context` | A global C mirror declaration and completion directive |
| Checker | `compiler/src/ambient.rs::context_fn`; `check/expr/call.rs` resolves the builtin namespace | `compiler/src/check/mirror_provenance.rs` and completion signature checks select a foreign async source |
| HIR | `Callee::Ambient(AmbientFn::Collect)` | A foreign callee with `completion_result` |
| LIR | `codegen/src/lir/call.rs` selects the Ambient intrinsic family | `codegen/src/lir/completion.rs` derives size, result kind, and Error layout for `HostCompletion` |
| Dev JIT | `codegen/src/lower/func/intrinsic.rs` calls `subscript_rt_collect_at` with the Context | `lower/func/instruction.rs` creates the source, then calls the bound foreign address |
| Address resolution | `codegen/src/jit/symbols.rs` registers runtime addresses | `NativeLibrary::new` supplies symbol names and C ABI addresses |
| C AOT | `codegen/src/cemit/intrinsic.rs` emits `subscript_rt_collect_at(ctx, position)` | `cemit/graph.rs` creates the source, then emits a C foreign call with the endpoint |
| Link resolution | The runtime archive supplies the C symbol | NativeLibrary include directories and C sources supply the host facade |
| Interpreter | `codegen/src/interpreter/dispatch.rs` calls its root-aware collector | `interpreter/instruction.rs` returns Unsupported before it creates a source or calls the host |

The builtin control program runs in the interpreter, dev JIT, and C AOT:

```ts
export function main(): void {
  Context.collect();
  print(JSON.stringify(1));
}
```

All three outputs are `1` followed by a newline.
The checker also has specific JSON graph construction and Worker operations; ordinary namespace declarations do not install these operations.
Evidence: `compiler/src/check/json.rs`, `check/expr/namespace.rs`, and `codegen/src/cemit/worker.rs`.

### Candidate spellings

Each candidate uses this body, with the declaration or import in the table:

```ts
export async function main(): Promise<void> {
  const h = Files.readText("hello.txt");
  await h;
}
```

| Spelling | Stock tsc result and required input | Compiler result at the pin |
|---|---|---|
| Ambient `Files.readText` | Accepts with `declare namespace Files { function readText(path: string): Promise<string>; }` | The namespace in a mirror receives S100: `mirror declaration form outside the decided surface` |
| Static class `Files.readText` | Accepts with `declare class Files { static readText(path: string): Promise<string>; }` | Mirror static methods receive S100; the call also receives S018 |
| Relative namespace import | Accepts `import * as Files from "./files"` with `files.ts`, its mirror, and the prelude | Accepts the prototype below |
| Relative named import | Accepts `import { readText } from "./files"` with the same inputs; change the call to `readText` | Uses the existing named-export resolution; no builtin identity is needed |
| Bare namespace import | TS2307 without a resolver declaration; accepts with `paths` that maps `subscript:files` to `files.ts` | S100 when the module is absent from supplied SourceFiles |
| Bare ambient module | Accepts with `declare module "subscript:files" { export function readText(path: string): Promise<string>; }` | Requires a compiler module resolver or supplied implementation; tsc acceptance alone does not supply one |
| Global `readText` | Accepts with a global function declaration and the prelude | Accepts a valid completion mirror, including direct `await s185ReadText(path)` |

A supplied SourceFile named `subscript:files.ts` makes the bare import check successfully through the Rust compiler API.
`compiler/src/check/mod.rs::normalize_module_specifier` removes `./` and `.ts`; it has no standard-module registry.
The CLI loader only discovers sibling `./name` source imports.
Evidence: `cli/src/program_loader.rs`, the `strip_prefix("./")` and no-slash tests.
A reserved bare spelling therefore needs a CLI loader change, even though the compiler API can supply its source today.

The prototype synchronous forwarding function returns `Promise<string>`.
`await Files.readText(path)` receives S100: `` `Files.readText` is synchronous and cannot be awaited ``.
Stock tsc accepts that exact program.
Saving the result, then awaiting the handle, passes both checkers and both native tiers.
Evidence: `compiler/src/check/expr/entry.rs`, the `sig.is_async` test in direct-await resolution.
A builtin file function needs direct-await source recognition, or a deliberate handle-first spelling.
An async forwarding wrapper is another option; it adds a script frame that follows normal reload rules.
That wrapper option was not timed.

The tsc controls use strict ES2022, ESNext modules, Bundler resolution, and the ESNext.Disposable library with no ambient package types.
The bare mapping is `paths: { "subscript:files": ["files.ts"] }`, resolved to the supplied wrapper.

The compiler virtual-module control supplies the mirror, the wrapper under name `subscript:files.ts`, and this entry:

```ts
import * as Files from "subscript:files";
export async function main(): Promise<void> {
  const h = Files.readText("hello.txt"); await h;
}
```

Without the supplied wrapper SourceFile, that same entry receives the S100 missing-module diagnostic.

## 2. Host configuration at the pin

| Route | Present code or API | File-provider registration site |
|---|---|---|
| C Context | `subscript_rt_ctx_new()` creates `Context::new_releasing()` | Add a Context setter; a custom C host calls it before `subscript_init` |
| Context options | Independent setters configure observers, allocation diagnostics, random seed, date, regex budget, and fault injection | There is no general Context-options struct at the pin; a file setter follows the present form |
| Explicit callback registration (§111) | `subscript_rt_cb_register` creates a record with Context, code, environment, and two userdata slots | This record can carry a host provider handle, but it does not enable a standard namespace automatically |
| Callback Context recovery | `subscript_rt_cb_registration_context(registration)` returns the creating Context | A host facade can use a live registration to obtain its Context; an endpoint itself contains no Context pointer |
| C AOT configured runner | `run_c_aot_with_native_libraries_and_host_hooks` installs a pre-entry hook | The hook registers the prototype provider before main, but after module initialization |
| Rust dev host | `ReloadSession::new_with_native_libraries` owns a private boxed Context | Add a setter on ReloadSession, or an options/configuration constructor; the prototype exposes a temporary Context accessor |
| Dev configured runner | `RunConfig` has native libraries and host hooks; the dev runner rejects host hooks | Add dev initialization support; the shipping hook cannot be reused as a dev hook today |

Evidence: `runtime/src/context/lifecycle.rs`, `runtime/src/ffi.rs`, `runtime/src/registration.rs`,
`codegen/src/lib.rs::RunConfig`, `codegen/src/reload.rs`, `codegen/src/jit/run.rs`, and `codegen/src/ship.rs`.

`ReloadSession::build` allocates the Context, installs its function table and globals, then runs the initializer before it returns.
The C hook anchor is immediately after `call_script_entry(ctx, subscript_init)`.
A file call in a module initializer therefore needs a provider option before initialization in both convenience APIs.
The prototype places file calls inside exports, so its post-construction registration is sufficient.

The runtime stores one copied provider per Context.
The dispatch reads that provider at each script call, after source creation and before host I/O.
It copies the provider to a local value before it invokes the callback.
No exclusive Context reference remains active during the callback; immediate completion can call the Context API again.

The host owns userdata and callback lifetimes.
A deferred provider must copy the path before the start callback returns.
The script string view is borrowed only for that callback.
The host must stop delivery before Context destruction. The two endpoint ids do not make a destroyed Context pointer valid.

## 3. Check-time and run-time enablement

The enablement witness is the section 4 program and its supplied module and mirror.

| Build/runtime input | Measured result |
|---|---|
| Module and mirror supplied; provider registered | Both native tiers print the real file results |
| Module and mirror supplied; no provider | Both native tiers catch `Error:file module disabled` |
| Module omitted from compiler inputs | S100: `imported module ./files is not among the program's files` |
| Mirror omitted; module supplied | S016 for `s185ReadText`, `s185ReadBytes`, and the two host control functions |
| Ambient Files declaration omitted from tsc inputs | TS2304: `Cannot find name 'Files'` |
| CLI `check --mirror` supplied | No errors for the prototype program |
| CLI `check` without the mirror | Four S016 unknown-function diagnostics |
| CLI `check --enable-files` | Unknown option `--enable-files` |

There is no file enablement option in `CheckOptions` or `RunConfig` at the pin.
`CheckOptions` only carries `poison_missing_modules`.
The mirror mechanism already gives an explicit build input: `SourceFile::ambient` and CLI `--mirror`.

A check-time design needs a named CLI option, a Rust build option, and corresponding tsc declaration selection.
For an ambient builtin, the checker must consult the build option before it resolves Files operations.
For an import module, the loader must supply its module only when enabled, or reject use through an explicit module option.
The same build option must select declarations for editor/tsc input.
An always-present Files prelude cannot make stock tsc diagnose a disabled host module by itself.
A build declaration states intent; it cannot prove that every runtime Context has a provider.
Both checks can therefore coexist: build diagnostics for disabled use, plus a runtime Error for a missing provider.

The prototype selects the runtime Error path.
It first creates the §178 source, then completes that source with `subscript_rt_complete_error` when the provider or callback is absent.
The existing source machinery allocates the Error with the checker-derived layout and queues its waiter.
No provider callback or file access occurs on that branch.
A Context trap is another contract option. That option was not prototyped.

## 4. Reverted prototype and two-tier witness

The prototype changes four production files:
`runtime/src/context.rs`, `runtime/src/context/lifecycle.rs`, `runtime/src/ffi.rs`, and `codegen/src/reload.rs`.
It also adds a temporary measurement runner. All five changes were removed after measurement.
No checker, LIR, native lowering, interpreter, prelude, or contract block changed.

The Context gains `file_provider: Option<S185FileProvider>`, initially None.
The temporary ReloadSession accessor returns `&mut *self.ctx` as a raw owner-thread pointer.
The following runtime addition implements registration and dispatch:

```rust
/// Measurement-only file callback ABI.
pub type S185Read = unsafe extern "C" fn(*mut c_void, *const u8, usize, CompletionEndpoint);
/// Measurement-only Context file provider.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct S185FileProvider {
    /// Host data; the host owns its lifetime.
    pub userdata: *mut c_void,
    /// Byte reader.
    pub read_bytes: Option<S185Read>,
    /// Text reader.
    pub read_text: Option<S185Read>,
}
/// Copies a provider into the Context. Null disables new requests.
/// # Safety
/// The Context and callback data remain live on the owner thread.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_s185_set_files(ctx: *mut Context, provider: *const S185FileProvider) {
    unsafe { (*ctx).file_provider = provider.as_ref().copied(); }
}
/// Reads the provider once and calls it without an active Context borrow.
/// # Safety
/// The path holds length readable bytes; the endpoint belongs to the Context.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_s185_read(ctx: *mut Context, path: *const u8, length: usize, endpoint: CompletionEndpoint, text: u32) {
    let provider = unsafe { (*ctx).file_provider };
    if let Some(provider) = provider {
        let callback = if text != 0 { provider.read_text } else { provider.read_bytes };
        if let Some(callback) = callback {
            unsafe { callback(provider.userdata, path, length, endpoint); }
            return;
        }
    }
    let message = b"file module disabled";
    unsafe { subscript_rt_complete_error(ctx, endpoint, message.as_ptr().cast(), message.len()); }
}
```

The C ABI struct is:

```c
typedef void (*S185Read)(void *userdata, const uint8_t *path,
    size_t path_length, subscript_rt_completion endpoint);
typedef struct S185FileProvider {
    void *userdata;
    S185Read read_bytes;
    S185Read read_text;
} S185FileProvider;
```

Measured layout: size 24 bytes and offsets 0, 8, 16. Pointer fields give alignment 8 on this target.
The optional Rust storage occupies 32 bytes on this target.
The setter copies the struct; the host can release its stack copy after registration.
It does not copy userdata or own the resource behind it.
A null setter argument disables new requests.

The boundary selection uses `s185ReadText=string` and `s185ReadBytes=u8[]`.
The binder produces these declarations from `binding.h`:

```ts
// @subscript-c-header include="binding.h"
// @subscript-c-completion function="s185ReadText" result="string"
// @subscript-c-string-view function="s185ReadText" parameter="path" aggregate="S185View"
// @subscript-c-completion function="s185ReadBytes" result="u8[]"
// @subscript-c-string-view function="s185ReadBytes" parameter="path" aggregate="S185View"

declare function s185ReadText(path: string): Promise<string>;
declare function s185ReadBytes(path: string): Promise<u8[]>;
declare function s185Flush(): void;
declare function s185Disable(): void;
declare function s185Bench(): void;
```

The execution mirror uses the same directives and signatures, with header `measure.h`.
The importable wrapper is:

```ts
export function readText(path: string): Promise<string> { return s185ReadText(path); }
export function readBytes(path: string): Promise<u8[]> { return s185ReadBytes(path); }
```

The execution header is:

```c
#ifndef S185_MEASURE_H
#define S185_MEASURE_H
#include "subscript_runtime.h"
#include <stddef.h>
#include <stdint.h>
typedef struct S185View { const char *data; size_t length; } S185View;
typedef void (*S185Read)(void*, const uint8_t*, size_t, subscript_rt_completion);
typedef struct S185FileProvider { void *userdata; S185Read read_bytes; S185Read read_text; } S185FileProvider;
void subscript_rt_s185_set_files(subscript_rt_context*, const S185FileProvider*);
void subscript_rt_s185_read(subscript_rt_context*, const uint8_t*, size_t, subscript_rt_completion, uint32_t);
void s185ReadText(S185View, subscript_rt_completion);
void s185ReadBytes(S185View, subscript_rt_completion);
void s185Flush(void);
void s185Bench(void);
void s185Disable(void);
#endif
```

The binder input header is:

```c
#include <stdint.h>
#include <stddef.h>
#ifndef SUBSCRIPT_RUNTIME_H
typedef struct subscript_rt_completion { uint64_t context_id; uint64_t operation_id; } subscript_rt_completion;
#endif
typedef struct S185View { const char *data; size_t length; } S185View;
void s185ReadText(S185View path, subscript_rt_completion endpoint);
void s185ReadBytes(S185View path, subscript_rt_completion endpoint);
void s185Flush(void);
void s185Disable(void);
void s185Bench(void);
```

The C host implementation follows. Its Context pointer is a single-session facade detail.
The runtime provider itself lives in the Context.
A production intrinsic must pass the current Context directly, or a host facade must resolve it from a per-Context host handle.
A process-wide active-Context facade is insufficient for independent or reentrant multi-Context calls.
The C host uses its pre-entry `setup` hook. The Rust host uses its temporary ReloadSession accessor before `run_main`.

```c
#include "measure.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static subscript_rt_context *context;
static subscript_rt_completion pending;
static char pending_path[1024];
static void finish(void *userdata, const uint8_t *path, size_t n, subscript_rt_completion ep, int text) {
    subscript_rt_context *ctx = userdata;
    char full[4096];
    const char *base = getenv("TMPDIR");
    assert(n < 1024);
    snprintf(full, sizeof(full), "%s/s185-measure/%.*s", base, (int)n, path);
    FILE *f = fopen(full, "rb");
    if (!f) { assert(subscript_rt_complete_error(ctx, ep, "missing file", 12) == 0); return; }
    assert(fseek(f, 0, SEEK_END) == 0);
    long length = ftell(f); assert(length >= 0); rewind(f);
    unsigned char *bytes = malloc(length ? (size_t)length : 1); assert(bytes);
    assert(fread(bytes, 1, (size_t)length, f) == (size_t)length); fclose(f);
    int status = text ? subscript_rt_complete_string(ctx, ep, (const char*)bytes, length)
                      : subscript_rt_complete_bytes(ctx, ep, bytes, length);
    assert(status == 0); free(bytes);
}
static void read_text(void *userdata, const uint8_t *path, size_t n, subscript_rt_completion ep) {
    if (n == 7 && memcmp(path, "pending", 7) == 0) {
        pending = ep; memcpy(pending_path, "hello.txt", 10); return;
    }
    finish(userdata, path, n, ep, 1);
}
static void read_bytes(void *userdata, const uint8_t *path, size_t n, subscript_rt_completion ep) { finish(userdata, path, n, ep, 0); }
void setup(subscript_rt_context *ctx) {
    context = ctx;
    S185FileProvider p = {ctx, read_bytes, read_text};
    subscript_rt_s185_set_files(ctx, &p);
}
void disabled_setup(subscript_rt_context *ctx) { context = ctx; }
void s185ReadText(S185View v, subscript_rt_completion ep) { subscript_rt_s185_read(context, (const uint8_t*)v.data, v.length, ep, 1); }
void s185ReadBytes(S185View v, subscript_rt_completion ep) { subscript_rt_s185_read(context, (const uint8_t*)v.data, v.length, ep, 0); }
void s185Flush(void) { finish(context, (const uint8_t*)pending_path, strlen(pending_path), pending, 1); }
void s185Disable(void) { subscript_rt_s185_set_files(context, NULL); }
```

The Rust host uses the same ABI and completion functions:

```rust
#[repr(C)]
#[derive(Clone, Copy)]
struct View { data: *const u8, length: usize }
static mut CTX: *mut Context = std::ptr::null_mut();
static mut PENDING: CompletionEndpoint = CompletionEndpoint { context_id:0, operation_id:0 };
fn root() -> PathBuf { std::env::temp_dir().join("s185-measure") }
unsafe fn finish(ctx: *mut Context, path: &[u8], ep: CompletionEndpoint, text: bool) {
    let path = std::str::from_utf8(path).unwrap();
    match std::fs::read(root().join(path)) {
        Ok(bytes) => {
            let status = if text { unsafe { ffi::subscript_rt_complete_string(ctx, ep, bytes.as_ptr().cast(), bytes.len()) } }
                         else { unsafe { ffi::subscript_rt_complete_bytes(ctx, ep, bytes.as_ptr(), bytes.len()) } };
            assert_eq!(status, CompletionStatus::Ok);
        }
        Err(_) => { assert_eq!(unsafe { ffi::subscript_rt_complete_error(ctx, ep, b"missing file".as_ptr().cast(),12) },CompletionStatus::Ok); }
    }
}
unsafe extern "C" fn text(userdata: *mut c_void, path: *const u8, n: usize, ep: CompletionEndpoint) {
    let path=unsafe { std::slice::from_raw_parts(path,n) };
    if path == b"pending" { unsafe { PENDING=ep; } }
    else { unsafe { finish(userdata.cast(),path,ep,true); } }
}
unsafe extern "C" fn bytes(userdata: *mut c_void, path: *const u8, n: usize, ep: CompletionEndpoint) {
    unsafe { finish(userdata.cast(),std::slice::from_raw_parts(path,n),ep,false); }
}
unsafe extern "C" fn read_text(v:View, ep:CompletionEndpoint) { unsafe { ffi::subscript_rt_s185_read(CTX,v.data,v.length,ep,1); } }
unsafe extern "C" fn read_bytes(v:View, ep:CompletionEndpoint) { unsafe { ffi::subscript_rt_s185_read(CTX,v.data,v.length,ep,0); } }
unsafe extern "C" fn flush() { unsafe { finish(CTX,b"hello.txt",PENDING,true); } }
unsafe extern "C" fn disable() { unsafe { ffi::subscript_rt_s185_set_files(CTX,std::ptr::null()); } }
fn setup(ctx:*mut Context, enabled:bool) {
    unsafe { CTX=ctx; }
    if enabled {
        let p=ffi::S185FileProvider { userdata:ctx.cast(), read_bytes:Some(bytes), read_text:Some(text) };
        unsafe { ffi::subscript_rt_s185_set_files(ctx,&p); }
    }
}
```

The host fixtures are `hello.txt` with bytes `hello file`, and `bytes.bin` with bytes 0, 128, 255.
They reside beneath a fixture directory in `$TMPDIR`.
The host interprets each relative path beneath that directory.
The missing-file message is deliberately stable; it is not a platform errno string.

The script is:

```ts
import * as Files from "./files";
export async function main(): Promise<void> {
  const sh = Files.readText("hello.txt"); const s = await sh;
  const bh = Files.readBytes("bytes.bin"); const b = await bh;
  print(`${s}:${s.length}:${b.length}:${b[0]}:${b[1]}:${b[2]}`);
  try { const h = Files.readText("missing.txt"); await h; }
  catch (e) { if (e instanceof Error) print(`${e.name}:${e.message}`); else throw e; }
  const pending = Files.readText("pending");
  Context.collect();
  print("collected pending");
  s185Disable();
  s185Flush();
  print(await pending);
  try { const h = Files.readText("hello.txt"); await h; }
  catch (e) { if (e instanceof Error) print(`${e.name}:${e.message}`); else throw e; }
}
```

Both native outputs match byte-for-byte:

```text
hello file:10:3:0:128:255
Error:missing file
collected pending
hello file
Error:file module disabled
```

The provider defers `pending` without a thread and stores the endpoint and a copied fixture path.
`Context.collect()` leaves that read valid.
The script then removes the provider, and the host completes the pending read successfully.
Disabling the provider blocks new reads; it does not revoke the already-issued endpoint in this prototype.
The host performs deferred I/O in `s185Flush` and completes it on the owner thread.
The async checkpoint later resumes the script. Completion itself executes no script code.

A separate fresh-Context program measures a provider that was never installed:

```ts
import * as Files from "./files";
export async function main(): Promise<void> {
  try { const h = Files.readText("hello.txt"); await h; }
  catch (e) {
    if (e instanceof Error) print(`${e.name}:${e.message}`);
    else throw e;
  }
}
```

Both native tiers print `Error:file module disabled` and finish normally.
The C host's `disabled_setup` only stores the Context for its facade. It registers no provider.
The Rust host calls `setup(session.s185_context(), false)`.

## 5. One MiB cost

The fixture contains 1,048,576 ASCII `a` bytes.
Each row contains eleven operations with fresh Contexts and fresh §178 sources.
There is no excluded warmup. Rows run in the displayed order.
The direct control holds its input bytes before the timer starts.
The file timer includes dispatch, path interpretation, open, read, close, completion, and host buffer release.
The direct timer includes completion validation, result allocation in the Context, copy, cache publication, and producer release.
Both timers exclude Context creation and setup, source creation, result comparison, await dispatch, collection, and Context destruction.
Every result is read through `subscript_rt_async_result_uncounted` and compared with all fixture bytes after the timer stops.
The source holder is then released. Explicit collection leaves zero live Context allocations in each sample.
Text completion includes UTF-8 validation. Byte completion does not.

The C rows run inside the actual shipping executable with the C provider above.
The Rust dev rows use the dev Context allocator and the Rust provider above.
The Rust ship rows isolate the same provider with the ship Context allocator.
They are runtime controls, rather than extra C AOT script executions.
These repeated small reads do not measure cold-storage latency or a throughput guarantee.
The C timer samples resolve to whole microseconds in this run; the table preserves that resolution.

C cost code:

```c
#include <time.h>
extern void *subscript_rt_async_host_operation(subscript_rt_context*, uint64_t, uint32_t, uint32_t, const uint64_t*, subscript_rt_completion*);
extern uint8_t subscript_rt_async_result_uncounted(subscript_rt_context*, const void*, void*, uint64_t);
extern void subscript_rt_async_release(subscript_rt_context*, void*, uint32_t);
static uint64_t nanos(void) { struct timespec t; assert(clock_gettime(CLOCK_MONOTONIC, &t) == 0); return (uint64_t)t.tv_sec*1000000000u + t.tv_nsec; }
static int compare_samples(const void *a, const void *b) { uint64_t x=*(const uint64_t*)a, y=*(const uint64_t*)b; return (x>y)-(x<y); }
void s185Bench(void) {
    unsigned char *input=malloc(1<<20); assert(input); memset(input,'a',1<<20);
    for (int kind=2;kind<=3;kind++) for (int file=0;file<=1;file++) {
        uint64_t samples[11];
        for (int i=0;i<11;i++) {
            subscript_rt_context *ctx=subscript_rt_ctx_new();
            S185FileProvider provider={ctx,read_bytes,read_text};
            subscript_rt_s185_set_files(ctx,&provider);
            const uint64_t metadata[6]={24,0,0,8,16,0}; subscript_rt_completion ep;
            void *h=subscript_rt_async_host_operation(ctx,8,kind,1,metadata,&ep); assert(h);
            uint64_t begin=nanos();
            if(file) subscript_rt_s185_read(ctx,(const uint8_t*)"large.bin",9,ep,kind==2);
            else if(kind==2) assert(subscript_rt_complete_string(ctx,ep,(const char*)input,1<<20)==0);
            else assert(subscript_rt_complete_bytes(ctx,ep,input,1<<20)==0);
            samples[i]=nanos()-begin;
            void *value=NULL; assert(subscript_rt_async_result_uncounted(ctx,h,&value,8)==1); assert(value);
            const unsigned char *actual;
            if(kind==2) { assert(*(const uint64_t*)value==(1<<20)); actual=(const unsigned char*)value+8; }
            else { assert(*(const uint64_t*)value==(1<<20)); actual=*(const unsigned char**)((const unsigned char*)value+24); }
            assert(memcmp(actual,input,1<<20)==0);
            subscript_rt_async_release(ctx,h,1); subscript_rt_ctx_collect(ctx);
            assert(subscript_rt_ctx_live_allocations(ctx)==0); subscript_rt_ctx_release(ctx);
        }
        printf("C BENCH kind=%d file=%d ns=",kind,file);
        for(int i=0;i<11;i++)printf("%s%llu",i?",":"",(unsigned long long)samples[i]);
        qsort(samples,11,sizeof(samples[0]),compare_samples);
        printf(" median=%llu min=%llu\n",(unsigned long long)samples[5],(unsigned long long)samples[0]);
    }
    free(input);
}
```

Rust cost code uses `Instant` and nanosecond samples:

```rust
unsafe fn source(ctx:*mut Context,kind:u32) -> (*mut u8,CompletionEndpoint) {
    let mut ep=CompletionEndpoint::default();
    let metadata=[24,0,0,8,16,0];
    let h=unsafe { ffi::subscript_rt_async_host_operation(ctx,8,kind,1,metadata.as_ptr(),&mut ep) };
    assert!(!h.is_null()); (h,ep)
}
fn bench() {
    let input=std::fs::read(root().join("large.bin")).unwrap();
    for ship in [false,true] { for kind in [2,3] { for file in [false,true] {
        let mut samples=Vec::new();
        for _ in 0..11 {
            let mut ctx=if ship {Context::new_releasing()} else {Context::new()};
            let cp=&mut *ctx as *mut Context; setup(cp,true);
            let (h,ep)=unsafe {source(cp,kind)};
            let start=Instant::now();
            if file { unsafe { ffi::subscript_rt_s185_read(cp,b"large.bin".as_ptr(),9,ep,u32::from(kind==2)); } }
            else if kind==2 { assert_eq!(unsafe {ffi::subscript_rt_complete_string(cp,ep,input.as_ptr().cast(),input.len())},CompletionStatus::Ok); }
            else { assert_eq!(unsafe {ffi::subscript_rt_complete_bytes(cp,ep,input.as_ptr(),input.len())},CompletionStatus::Ok); }
            samples.push(start.elapsed().as_nanos());
            let mut p:usize=0;
            assert_eq!(unsafe {ffi::subscript_rt_async_result_uncounted(cp,h,(&mut p as *mut usize).cast(),8)},1);
            let actual=if kind==2 { unsafe {ctx.str_bytes(p as *const u8)} }
                       else { assert_eq!(unsafe {ctx.array_len(p as *const u8)},1<<20); unsafe { std::slice::from_raw_parts(ctx.array_data(p as *const u8),1<<20) } };
            assert_eq!(actual,input);
            unsafe {ffi::subscript_rt_async_release(cp,h,1);}
            ctx.collect(); assert_eq!(ctx.live_count(),0);
        }
        let raw=samples.clone(); samples.sort(); println!("BENCH ship={ship} kind={kind} file={file} ns={raw:?} median={} min={}",samples[5],samples[0]);
    } } }
}
```

| Host / Context | Result | Timed route | Median, µs | Minimum, µs |
|---|---|---|---:|---:|
| C host / ship | text | direct completion | 107.000 | 104.000 |
| C host / ship | text | file read + completion | 180.000 | 168.000 |
| C host / ship | bytes | direct completion | 36.000 | 36.000 |
| C host / ship | bytes | file read + completion | 111.000 | 107.000 |
| Rust host / dev | text | direct completion | 91.833 | 91.583 |
| Rust host / dev | text | file read + completion | 147.083 | 142.334 |
| Rust host / dev | bytes | direct completion | 32.208 | 32.042 |
| Rust host / dev | bytes | file read + completion | 90.500 | 89.958 |
| Rust host / ship | text | direct completion | 81.792 | 81.583 |
| Rust host / ship | text | file read + completion | 141.542 | 140.333 |
| Rust host / ship | bytes | direct completion | 32.917 | 30.250 |
| Rust host / ship | bytes | file read + completion | 86.792 | 83.708 |

All eleven samples per row, in nanoseconds:

```text
C host / ship; text; direct completion: 244000,108000,108000,105000,105000,108000,105000,107000,105000,107000,104000
C host / ship; text; file read + completion: 401000,230000,195000,185000,181000,174000,180000,174000,170000,168000,170000
C host / ship; bytes; direct completion: 40000,36000,36000,36000,36000,36000,36000,36000,37000,36000,36000
C host / ship; bytes; file read + completion: 116000,113000,112000,111000,111000,108000,109000,111000,108000,108000,107000
Rust host / dev; text; direct completion: 136375, 94625, 91917, 94000, 91833, 91750, 91750, 91791, 91916, 91625, 91583
Rust host / dev; text; file read + completion: 214042, 183375, 170542, 152125, 147083, 145958, 147500, 145542, 144417, 142958, 142334
Rust host / dev; bytes; direct completion: 32042, 32458, 32250, 32334, 32209, 32333, 32084, 32084, 32208, 32208, 32167
Rust host / dev; bytes; file read + completion: 94167, 95291, 91417, 90833, 90875, 90333, 90417, 90500, 89958, 90250, 90125
Rust host / ship; text; direct completion: 82625, 82792, 83709, 81625, 83583, 81792, 83583, 81584, 81625, 81583, 81709
Rust host / ship; text; file read + completion: 150542, 142667, 141542, 141084, 141083, 145166, 142542, 141209, 140375, 140333, 161417
Rust host / ship; bytes; direct completion: 33500, 33625, 33042, 58750, 32958, 32917, 32875, 32917, 32917, 32791, 30250
Rust host / ship; bytes; file read + completion: 93333, 96834, 86792, 85792, 86084, 85583, 87417, 89291, 94375, 83792, 83708
```

## 6. Interpreter

The interpreter runs the section 1 builtin control and prints `1`.
For the section 4 file program, it returns:

```text
Execution {
  output: [],
  source: Unsupported { reason: "s185ReadText requires a native library" }
}
```

It does not create a file source, call a provider, or complete a disabled-module Error in that program.
`interpreter::interpret(&lir)` accepts no provider or native-library argument.
Its internal interpreter owns `Context::new()` and rejects HostCompletion in `interpreter/instruction.rs`.
A setter on that Context alone cannot change the rejected instruction.
Interpreter support needs a separate host dispatch API and a HostCompletion execution path.
Keeping native file calls Unsupported is another explicit tier policy option.

## 7. Pending read across reload

The first witness awaits a source in an old script frame:

```ts
import * as Files from "./files";
export async function main(): Promise<void> {
  const p = Files.readText("pending");
  print("waiting");
  print(await p);
}
export function collect(): void { Context.collect(); }
```

The replacement changes only `waiting` to `new body`.
The Rust host calls main, calls collect, accepts the replacement, completes the saved endpoint, then calls `async_step`.
Before completion, pending is 0 and unfinished is 2: one source and one script invocation.
After completion, pending is 1 and unfinished is 1.
The host completion returns OK; the endpoint still identifies its source after reload.
The checkpoint traps `StaleCoroutine` (12) at the old await site.
The only output is `waiting` followed by a newline. No result or replacement-body effect occurs.
This is the old-waiter behavior of §178 rule 10.

A second witness stores the source without an old waiting script frame:

```ts
import * as Files from "./files";
let held: Promise<string>[] = [];
export function main(): void { held.push(Files.readText("pending")); }
export async function consume(): Promise<void> { print(await held[0]); }
```

The replacement changes `pending` to `hello.txt` in main's body.
The host calls main, accepts reload, completes the saved endpoint, then calls the new consume export.
Completion returns OK, but the new consume call also traps `StaleCoroutine` (12). Its output is empty.
The source keeps its completion identity, but a new waiter cannot use this pre-reload handle at the pin.

Evidence: `runtime/src/context/async_scheduler.rs::async_is_stale` compares every frame's created epoch with the current epoch.
It does not exempt runtime host-operation frames.
`codegen/src/lower/func/coroutine.rs::await_async_handle` applies that stale test to the source handle before await registration.
A contract must distinguish endpoint survival from post-reload handle usability.
If new waiters must consume old host sources, runtime-kind-aware stale checks need a separate change.
No stale behavior was changed in this measurement.

## 8. Initial operations and path rule

The four first whole-file operations are:

| Operation | Proposed result | Present completion route |
|---|---|---|
| `readText(path)` | `Promise<string>` | `subscript_rt_complete_string` |
| `readBytes(path)` | `Promise<u8[]>` | `subscript_rt_complete_bytes` |
| `writeText(path, text)` | `Promise<void>` | `subscript_rt_complete_void` |
| `writeBytes(path, bytes)` | `Promise<void>` | `subscript_rt_complete_void` |

Only the two read operations were prototyped.
A future metadata or handle operation can use `subscript_rt_complete_value` with an admitted scalar or C value struct.
Every operation can use `subscript_rt_complete_error` for host failures.

Comparable embedded-language surfaces below are documentation evidence only. Each row is marked *(docs)*.

| System | Documented file operations | Candidate additions |
|---|---|---|
| Lua 5.4 *(docs)* | `io.open` supports read, overwrite, append, and binary modes; file methods read, write, close, seek, and flush | Append, handles, bounded reads, line reads, seek, flush. [Lua I/O documentation](https://www.lua.org/manual/5.4/manual.html#6.8) |
| Lua 5.4 *(docs)* | `os.remove` and `os.rename` delete and rename paths | Remove and rename. [Lua OS documentation](https://www.lua.org/manual/5.4/manual.html#6.9) |
| AngelScript file addon *(docs)* | Registered file objects expose open, close, size, EOF, string/line and numeric reads, writes, position, and seek | Size/stat, line reads, explicit handles. Host registration enables the addon; a build definition can remove write support. [File addon documentation](https://www.angelcode.com/angelscript/sdk/docs/manual/doc_addon_file.html) |
| AngelScript filesystem addon *(docs)* | Registered filesystem objects expose directory lists, directory creation/removal, file deletion, copy, move, size, and timestamps | List, mkdir, remove, copy, rename, metadata. [Filesystem addon documentation](https://www.angelcode.com/angelscript/sdk/docs/manual/doc_addon_filesystem.html) |

These documents do not establish asynchronous or Context behavior for subscript.
The first scope can remain whole-file reads and writes.
`stat`, existence/type queries, list, remove, rename, mkdir, copy, and append are explicit extension decisions.
An existence check cannot replace the error result of a later read; the host state can change between calls.

The host interprets the path. The proposed module boundary requires the script to pass UTF-8 bytes and a byte length.
The prototype forwards valid literal bytes. It does not validate path bytes itself.
The runtime need not select a working directory, normalize separators, resolve links, or implement OS path rules.
The prototype host interprets paths relative to its fixture directory.
The callback accepts a length-delimited view, rather than a NUL-terminated string.
A host that retains the path must copy its bytes before callback return.
Non-ASCII paths, embedded NUL policy, normalization, and platform-native path conversion were not measured.
The final contract must state the UTF-8 boundary rule and the host policy for unrepresentable native paths.

Write buffers need an additional lifetime rule.
The host must copy borrowed text/bytes before the start callback returns, or hold an explicit Context root until completion.
Read completions already copy host buffers through §184.
UTF-8 errors and oversized read results leave a §184 source pending; the provider must handle those statuses.
A provider can convert those failures to a source Error. This conversion was not prototyped.

The start callback returns void. A provider that returns without completion or endpoint retention leaves the source pending indefinitely.
A contract must require one terminal completion for each accepted request, or define cancellation and timeout behavior.
A duplicate completion receives the existing §178 DUPLICATE status while the source remains live.

## 9. Decisions required by a contract

| Decision | Measured options and consequences |
|---|---|
| Public spelling | Relative Files import works today; builtin namespace and mirror static methods require checker support; bare import needs loader support |
| Direct await | Direct foreign completion call works; synchronous module forwarding requires handle-first await; async wrappers add a script frame |
| Check-time enablement | Conditional module/mirror input produces diagnostics; no file flag or build option exists; tsc inputs must select the same declarations |
| Runtime disabled behavior | Error completion works in both tiers; a Context trap remains an unmeasured option |
| Registration API | Context setter works; ReloadSession needs a setter or pre-initializer options; both convenience initialization paths need an earlier hook |
| Current Context routing | Prototype host facade stores one active Context; a standard intrinsic can pass Context directly; explicit host handles can route multiple Contexts |
| Provider ABI | Two callbacks plus userdata occupy 24 bytes; decide field order, optional callbacks, and future version/size extension |
| Provider replacement | New calls read the current provider; removal does not cancel an existing source; pending userdata must remain live |
| Completion timing | Immediate completion and deferred owner-thread completion work; the host must queue other-thread results to the owner thread |
| Pending lifetime | Collect preserves the source and waiter; Context destruction requires host delivery shutdown |
| Provider obligation | The start callback returns void; require one terminal completion for each retained request, or define cancellation/timeout behavior |
| Reload | Endpoints survive; old waiters trap; new waiters also reject pre-reload source handles at the pin |
| Interpreter | Unsupported before a source exists; host access requires a new interpreter dispatch API or an explicit native-only module policy |
| Initial operation set | Reads are measured; whole-file writes use void completion; metadata and path operations need explicit selections |
| Text and sizes | §184 supplies UTF-8 validation and the i32 maximum; provider status handling must prevent indefinitely pending invalid results |
| Path semantics | Host interprets UTF-8 views; decide NUL handling and native conversion without runtime filesystem policy |
| Write ownership | Copy input at start, or retain an explicit root; the borrowed read-path rule alone is insufficient for deferred writes |
| Failure details | Stable Error messages work; errno/code fields, partial writes, cancellation, and allocation-failure policy need decisions |
| I/O cost | File rows include host allocation and read plus runtime copy; direct rows exclude I/O; no zero-copy result adoption was measured |

The repository status after restoration contains only `specs/tracking/s185-file-module-measurement.md`.

## 10. Complete measurement driver

This temporary binary belongs to the `subscript-codegen` package, with its existing compiler and runtime dependencies.
The four production additions are specified in section 4. No other production patch is needed.
The fixtures, headers, mirror, wrapper, and main program above occupy the driver's `root()` directory.
The fixture `large.bin` contains 1 MiB of ASCII `a`.
The `binding.h` input above produces the binder control; execution uses `measure.h`.

The driver makes every checker, native tier, interpreter, and reload call that supplies the reported results:

```rust
//! Temporary §185 measurement runner.
use std::{ffi::c_void, path::PathBuf, time::Instant};
use subscript_codegen::{NativeLibrary, ReloadSession, run_c_aot_with_native_libraries_and_host_hooks};
use subscript_compiler::{SourceFile, check_program};
use subscript_runtime::{Context, ffi, context::{CompletionEndpoint, CompletionStatus}};

#[repr(C)]
#[derive(Clone, Copy)]
struct View { data: *const u8, length: usize }
static mut CTX: *mut Context = std::ptr::null_mut();
static mut PENDING: CompletionEndpoint = CompletionEndpoint { context_id:0, operation_id:0 };
fn root() -> PathBuf { std::env::temp_dir().join("s185-measure") }
unsafe fn finish(ctx: *mut Context, path: &[u8], ep: CompletionEndpoint, text: bool) {
    let path = std::str::from_utf8(path).unwrap();
    match std::fs::read(root().join(path)) {
        Ok(bytes) => {
            let status = if text { unsafe { ffi::subscript_rt_complete_string(ctx, ep, bytes.as_ptr().cast(), bytes.len()) } }
                         else { unsafe { ffi::subscript_rt_complete_bytes(ctx, ep, bytes.as_ptr(), bytes.len()) } };
            assert_eq!(status, CompletionStatus::Ok);
        }
        Err(_) => { assert_eq!(unsafe { ffi::subscript_rt_complete_error(ctx, ep, b"missing file".as_ptr().cast(),12) },CompletionStatus::Ok); }
    }
}
unsafe extern "C" fn text(userdata: *mut c_void, path: *const u8, n: usize, ep: CompletionEndpoint) {
    let path=unsafe { std::slice::from_raw_parts(path,n) };
    if path == b"pending" { unsafe { PENDING=ep; } }
    else { unsafe { finish(userdata.cast(),path,ep,true); } }
}
unsafe extern "C" fn bytes(userdata: *mut c_void, path: *const u8, n: usize, ep: CompletionEndpoint) {
    unsafe { finish(userdata.cast(),std::slice::from_raw_parts(path,n),ep,false); }
}
unsafe extern "C" fn read_text(v:View, ep:CompletionEndpoint) { unsafe { ffi::subscript_rt_s185_read(CTX,v.data,v.length,ep,1); } }
unsafe extern "C" fn read_bytes(v:View, ep:CompletionEndpoint) { unsafe { ffi::subscript_rt_s185_read(CTX,v.data,v.length,ep,0); } }
unsafe extern "C" fn flush() { unsafe { finish(CTX,b"hello.txt",PENDING,true); } }
unsafe extern "C" fn disable() { unsafe { ffi::subscript_rt_s185_set_files(CTX,std::ptr::null()); } }
fn setup(ctx:*mut Context, enabled:bool) {
    unsafe { CTX=ctx; }
    if enabled {
        let p=ffi::S185FileProvider { userdata:ctx.cast(), read_bytes:Some(bytes), read_text:Some(text) };
        unsafe { ffi::subscript_rt_s185_set_files(ctx,&p); }
    }
}
unsafe extern "C" fn c_bench_stub() {}
fn library() -> NativeLibrary {
    let r=root();
    unsafe { NativeLibrary::new(vec![r.clone(), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime/include")], vec![r.join("host.c")], vec![
        ("s185ReadText".into(),read_text as *const u8), ("s185ReadBytes".into(),read_bytes as *const u8),
        ("s185Bench".into(),c_bench_stub as *const u8), ("s185Flush".into(),flush as *const u8), ("s185Disable".into(),disable as *const u8),
    ]) }
}
fn files(main:&str) -> Vec<SourceFile> {
    vec![SourceFile::ambient("measure.generated.d.ts",std::fs::read_to_string(root().join("measure.generated.d.ts")).unwrap()),
         SourceFile::new("files.ts",std::fs::read_to_string(root().join("files.ts")).unwrap()),
         SourceFile::entry("main.ts",main)]
}
fn show(label:&str, output:&[u8]) { println!("{label}\n{}",String::from_utf8_lossy(output)); }
unsafe fn source(ctx:*mut Context,kind:u32) -> (*mut u8,CompletionEndpoint) {
    let mut ep=CompletionEndpoint::default();
    let metadata=[24,0,0,8,16,0];
    let h=unsafe { ffi::subscript_rt_async_host_operation(ctx,8,kind,1,metadata.as_ptr(),&mut ep) };
    assert!(!h.is_null()); (h,ep)
}
fn bench() {
    let input=std::fs::read(root().join("large.bin")).unwrap();
    for ship in [false,true] { for kind in [2,3] { for file in [false,true] {
        let mut samples=Vec::new();
        for _ in 0..11 {
            let mut ctx=if ship {Context::new_releasing()} else {Context::new()};
            let cp=&mut *ctx as *mut Context; setup(cp,true);
            let (h,ep)=unsafe {source(cp,kind)};
            let start=Instant::now();
            if file { unsafe { ffi::subscript_rt_s185_read(cp,b"large.bin".as_ptr(),9,ep,u32::from(kind==2)); } }
            else if kind==2 { assert_eq!(unsafe {ffi::subscript_rt_complete_string(cp,ep,input.as_ptr().cast(),input.len())},CompletionStatus::Ok); }
            else { assert_eq!(unsafe {ffi::subscript_rt_complete_bytes(cp,ep,input.as_ptr(),input.len())},CompletionStatus::Ok); }
            samples.push(start.elapsed().as_nanos());
            let mut p:usize=0;
            assert_eq!(unsafe {ffi::subscript_rt_async_result_uncounted(cp,h,(&mut p as *mut usize).cast(),8)},1);
            let actual=if kind==2 { unsafe {ctx.str_bytes(p as *const u8)} }
                       else { assert_eq!(unsafe {ctx.array_len(p as *const u8)},1<<20); unsafe { std::slice::from_raw_parts(ctx.array_data(p as *const u8),1<<20) } };
            assert_eq!(actual,input);
            unsafe {ffi::subscript_rt_async_release(cp,h,1);}
            ctx.collect(); assert_eq!(ctx.live_count(),0);
        }
        let raw=samples.clone(); samples.sort(); println!("BENCH ship={ship} kind={kind} file={file} ns={raw:?} median={} min={}",samples[5],samples[0]);
    } } }
}
fn main() {
    println!("PROVIDER size={} optional={} offsets={},{},{}",std::mem::size_of::<ffi::S185FileProvider>(),std::mem::size_of::<Option<ffi::S185FileProvider>>(),std::mem::offset_of!(ffi::S185FileProvider,userdata),std::mem::offset_of!(ffi::S185FileProvider,read_bytes),std::mem::offset_of!(ffi::S185FileProvider,read_text));
    let control=[SourceFile::entry("control.ts","export function main():void{Context.collect();print(JSON.stringify(1));}")];
    let control_hir=check_program(&control).unwrap();
    let control_lir=subscript_codegen::lir::lower_module(&control_hir).unwrap();
    let interpreted=subscript_codegen::interpreter::interpret(&control_lir).unwrap();
    let mut control_session=ReloadSession::new(&control).unwrap();control_session.run_main().unwrap();assert_eq!(control_session.take_output(),interpreted);
    assert_eq!(subscript_codegen::run_c_aot(&control).unwrap(),interpreted);show("BUILTIN THREE TIERS",&interpreted);
    let libs=[library()];
    let main=std::fs::read_to_string(root().join("main.ts")).unwrap();
    let input=files(&main);
    let h=check_program(&input).expect("check");
    let lir=subscript_codegen::lir::lower_module(&h).expect("LIR");
    println!("INTERPRETER {:?}",subscript_codegen::interpreter::interpret(&lir));
    let mut session=ReloadSession::new_with_native_libraries(&input,&libs).expect("JIT compile");
    setup(session.s185_context(),true);
    session.run_main().expect("JIT run"); let dev=session.take_output(); show("JIT",&dev);
    let ship=run_c_aot_with_native_libraries_and_host_hooks(&input,&libs,Some("setup"),None).expect("AOT");
    assert_eq!(dev,ship); show("AOT",&ship);
    let absent=files("import * as Files from './files'; export async function main():Promise<void>{try{const h=Files.readText('hello.txt');await h;}catch(e){if(e instanceof Error)print(`${e.name}:${e.message}`);else throw e;}}");
    let mut session=ReloadSession::new_with_native_libraries(&absent,&libs).unwrap(); setup(session.s185_context(),false); session.run_main().unwrap(); let dev=session.take_output();
    let ship=run_c_aot_with_native_libraries_and_host_hooks(&absent,&libs,Some("disabled_setup"),None).unwrap(); assert_eq!(dev,ship); show("ABSENT BOTH",&dev);
    // Old continuation: completion stays valid, then the old waiter traps.
    let old="import * as Files from './files'; export async function main():Promise<void>{const p=Files.readText('pending');print('waiting');print(await p);} export function collect():void{Context.collect();}";
    let new=old.replace("'waiting'","'new body'");
    let mut session=ReloadSession::new_with_native_libraries(&files(old),&libs).unwrap(); setup(session.s185_context(),true); session.call_main().unwrap();
    session.call_export("collect").unwrap(); println!("RELOAD before pending={} unfinished={}",session.async_pending(),session.async_unfinished());
    session.reload(&files(&new)).unwrap();
    unsafe { flush(); }
    println!("RELOAD completed pending={} unfinished={}",session.async_pending(),session.async_unfinished());
    println!("RELOAD resume {:?}",session.async_step()); show("RELOAD OUTPUT",&session.take_output());
    // A source kept in global storage, then read by a new invocation.
    let old="import * as Files from './files'; let held:Promise<string>[]=[]; export function main():void{held.push(Files.readText('pending'));} export async function consume():Promise<void>{print(await held[0]);}";
    let new=old.replace("'pending'","'hello.txt'");
    let mut session=ReloadSession::new_with_native_libraries(&files(old),&libs).unwrap(); setup(session.s185_context(),true); session.call_main().unwrap(); session.reload(&files(&new)).unwrap(); unsafe{flush();} println!("RELOAD NEW WAITER call {:?}", session.call_export("consume")); show("RELOAD NEW WAITER OUTPUT",&session.take_output());
    for (label, program) in [
        ("ambient namespace", "declare namespace Files { function readText(path:string):Promise<string>; } export async function main():Promise<void>{const h=Files.readText('hello.txt');await h;}"),
        ("ambient class", "declare class Files { static readText(path:string):Promise<string>; } export async function main():Promise<void>{const h=Files.readText('hello.txt');await h;}"),
        ("unknown global", "export async function main():Promise<void>{const h=readText('hello.txt');await h;}"),
        ("standard bare import", "import * as Files from 'subscript:files'; export async function main():Promise<void>{const h=Files.readText('hello.txt');await h;}"),
        ("builtin control", "export function main():void{Context.collect();print(JSON.stringify(1));}"),
    ] { println!("CHECK {label}: {:?}",check_program(&[SourceFile::entry("main.ts",program)]).map(|_|"accepted")); }
    for (label, declaration) in [
        ("mirror namespace", "declare namespace Files { function readText(path:string):Promise<string>; }"),
        ("mirror static class", "declare class Files { static readText(path:string):Promise<string>; }"),
    ] { println!("CHECK {label}: {:?}", check_program(&[SourceFile::ambient("candidate.d.ts",declaration),SourceFile::entry("main.ts","export async function main():Promise<void>{const h=Files.readText('hello.txt');await h;}")]).map(|_|"accepted")); }
    let mut direct=input.clone(); direct[2]=SourceFile::entry("main.ts","import * as Files from './files'; export async function main():Promise<void>{print(await Files.readText('hello.txt'));}");
    println!("CHECK direct wrapper await: {:?}",check_program(&direct).map(|_|"accepted"));
    let mut direct=input.clone(); direct[2]=SourceFile::entry("main.ts","export async function main():Promise<void>{print(await s185ReadText('hello.txt'));}");
    println!("CHECK direct mirror await: {:?}",check_program(&direct).map(|_|"accepted"));
    let mut virtual_files=input.clone();virtual_files[1].name="subscript:files.ts".into();virtual_files[2]=SourceFile::entry("main.ts","import * as Files from 'subscript:files'; export async function main():Promise<void>{const h=Files.readText('hello.txt');await h;}");
    println!("CHECK supplied virtual module: {:?}",check_program(&virtual_files).map(|_|"accepted"));
    let disabled=vec![input[0].clone(),input[2].clone()];
    println!("CHECK disabled module: {:?}",check_program(&disabled).map(|_|"accepted"));
    let no_mirror=vec![input[1].clone(),input[2].clone()];
    println!("CHECK disabled mirror: {:?}",check_program(&no_mirror).map(|_|"accepted"));
    let c_bench=files("export function main():void{s185Bench();}");
    let c_output=run_c_aot_with_native_libraries_and_host_hooks(&c_bench,&libs,Some("setup"),None).expect("C cost"); show("C COST",&c_output);
    bench();
    unsafe {CTX=std::ptr::null_mut();}
}
```

The stock tsc controls use the compiler options stated in section 1 and TypeScript `5.9.2` from the repository dependency.
Each control uses `tsc --project` with a config that lists the prelude and the candidate inputs explicitly.
The prototype config lists the prelude, its C mirror, `files.ts`, and `main.ts`.
The source program and its declarations use the same inputs for check-time and runtime enablement controls.
