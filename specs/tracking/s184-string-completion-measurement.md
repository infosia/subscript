# §184: String and byte-array completion results

Measurement pin: `339f01b4f27b3924035ffa878a672dd9e78a992a`.
Platform: aarch64 macOS. Rust uses release builds. The C AOT runner uses its optimized C build.

This is a Step 0 measurement. The prototype contradicts §178 rule 6, which excludes strings and reference types.
The prototype admits `Promise<string>` and `Promise<u8[]>`. It does not change that rule.
All prototype production changes and test files were removed after measurement. This note is the only repository change.

## 1. Representation at the pin

All three readers use the same Context string and dynamic-array storage.
The interpreter does not use a Rust `String` or `Vec<u8>` as the language value.
It uses `Value::Handle` and the shared runtime allocation functions.

| Property | String | `u8[]` |
|---|---|---|
| Language value | One payload pointer | One header payload pointer |
| Allocation header before each payload | 16 bytes: state `u64`, class `u32`, position `u32` | Same header, on both the array and its data |
| Payload | Byte length `u64`, then exactly that many bytes | `len:u64`, `cap:u64`, `elem_size:u64`, `data:pointer`, `holders:u32`, four padding bytes |
| Payload size | `8 + length` | Header: 40 bytes; data: `capacity` bytes |
| Counts | No string reference count | Holder count at offset 32 starts at 1; byte elements have no counted leaves |
| Terminator | No NUL terminator | No terminator |
| Allocation owner | Context | Context owns both allocations |

The string length counts bytes. The `u8[]` element size is one byte.
Both script length accessors return `i32`, despite the `u64` stored lengths.
An array holder is separate from a Promise frame count.
Strings and byte arrays survive through explicit collection roots, rather than a string count or a counted element description.

| Reader | Allocation route | Release route |
|---|---|---|
| Dev JIT | `Context::new`, exact-size system allocations | Delete or explicit collection releases the allocation |
| C AOT | `Context::new_releasing`, size-class arena; large values use separate system allocations | Small blocks return to arena free lists; large blocks return to the allocator |
| Interpreter | `Context::new`; `alloc_string`, `array_literal` call the shared runtime | Interpreter supplies live roots to Context collection |

Evidence: `runtime/src/context.rs` (`HEADER_SIZE`, header offsets, `ArrayHeader`),
`runtime/src/context/memory.rs` (`alloc_str_with`, `array_new`, `array_from_bytes`),
`runtime/src/context/lifecycle.rs` (`new`, `new_releasing`, `alloc`),
`codegen/src/interpreter/memory.rs` (`alloc_string`), and `codegen/src/interpreter/operations.rs` (`array_literal`).

A host view enters the Context through `subscript_rt_str_from_view` or string-field copy-in.
Both copy bytes into `Context::alloc_str`; they do not adopt a host pointer.
`array_from_bytes` creates the array header and copies the host bytes into its data allocation.
That helper is crate-private at the pin. No public host byte-array completion API exists.

The prototype stores the new payload pointer in eight completion bytes.
`Context::collect` scans completion bytes as native words, then follows the array header to its data allocation.
The source therefore roots its value before a waiter reads it.
The script's ordinary roots keep the returned value alive after the source ends.
Evidence: `runtime/src/context/memory.rs`, the `completion` root set and array payload scan.

The interpreter representation control uses this program in all three readers:

```ts
export function main(): void {
  const s = "hello";
  const b: u8[] = [0, 128, 255];
  print(`${s.length}:${b.length}:${b[2]}`);
  Context.collect();
  print(`${s}:${b[2]}`);
}
```

All three outputs are `5:3:255` and `hello:255`, each followed by a newline.
The interpreter cannot call a native completion function.
An attempted `await text()` returns `Unsupported { reason: "text requires a native library" }` before output.

## 2. Invalid UTF-8 at existing boundaries

Each row uses `ff`, `80`, and `e2 82` separately.
The last sequence lacks its third byte.
The prototype changes none of these existing boundary functions.

| Boundary | Result for each sequence | Trap or rejection |
|---|---|---|
| Bound string-view parameter | C receives the exact original bytes | None |
| String-field output through a bound C pointer parameter | Script receives the exact original bytes | None |
| `subscript_rt_str_from_view` | Exact bytes in Context-owned string | None |
| Direct bound string-view result by value | Binder rejects the declaration before execution | No return-value conversion exists |
| `subscript_rt_complete_error` message | Status `OK` (0); `Error.message` preserves the bytes | None while a holder keeps the source |
| Error report text | `Error: \u{fffd}` for each sequence | Report conversion replaces invalid UTF-8 |
| `JSON.parse<string>` of a quoted invalid string | Catchable `SyntaxError` | No Context trap |
| JSON runtime parse-begin | Parser id 0; message `JSON.parse: invalid syntax at byte 1` | No Context trap |
| `encodeURIComponent` of raw invalid bytes | `%FF`, `%80`, `%E2%82` | None |
| `decodeURIComponent` of raw invalid bytes without `%` | Exact bytes survive | None |
| `decodeURI` or `decodeURIComponent` of those percent escapes | Catchable `URIError` | No Context trap |

There is no `TextDecoder` or `TextEncoder` implementation or prelude declaration in the repository at the pin.
URI conversion is the nearest implemented byte conversion measured here.
URI decode validates escaped sequences. It copies raw, unescaped bytes without UTF-8 validation.

Evidence: `bindgen/src/emit.rs` rejects string-view results by value.
`runtime/src/ffi/strings.rs` copies a view through `alloc_str_from_view`.
`runtime/src/exception/host_error.rs` copies the message into a string.
`runtime/src/exception.rs` uses `String::from_utf8_lossy` for report text.
`runtime/src/json.rs` checks raw string spans with `std::str::from_utf8`.
`runtime/src/uri.rs` validates escaped sequences only.

The C boundary program, the generated mirror, and the script appear in section 5.
The script reports byte lengths and sums, so console text replacement cannot conceal the result.
The runtime probe in section 7 also compares the copied bytes directly.

The URI control runs in the interpreter, dev JIT, and C AOT:

```ts
export function main(): void {
  const xs: string[] = ["%FF", "%80", "%E2%82"];
  for (const x of xs) {
    try { decodeURI(x); print("accepted"); }
    catch (e) { if (e instanceof Error) print(e.name); else throw e; }
    try { decodeURIComponent(x); print("accepted"); }
    catch (e) { if (e instanceof Error) print(e.name); else throw e; }
  }
}
```

Each reader prints `URIError` six times. None prints `accepted`.

## 3. Length limits at the pin

The runtime has no single enforced maximum across all string and array creation routes.
The largest nonnegative script length is `i32::MAX`, or 2,147,483,647.
This is a script representation limit, not a universal allocation guard.

| Route | Limit or failure |
|---|---|
| `alloc_str_with(length, ...)` | Checks `8 + length` overflow; allocator also checks the aligned layout |
| `subscript_rt_str_len` | Casts stored byte length to `i32`; no range check |
| `array_from_bytes` | Rejects lengths above `i32::MAX` with `Internal` (11) before it reads the pointer |
| `array_with_capacity` | Checks capacity times element size; no `i32::MAX` guard |
| `array_push` | No explicit `i32::MAX` guard; increments stored length and casts to `i32` |
| `array_len` | Casts stored length to `i32`; no range check |

On this 64-bit target, the maximum representable 16-aligned allocation size is `isize::MAX - 15`.
The string layout therefore permits at most `isize::MAX - 39` byte slots before system memory availability matters.
A separate byte-array data layout permits at most `isize::MAX - 31` byte slots.
These are layout-derived ceilings, not measured successful allocations.
Array multiplication and header allocation introduce additional failure points.

The measurement does not allocate a 2 GiB value or a near-address-space-limit value.
It measures the guards and casts without such an allocation:

```rust
let p = ctx.alloc_str_with(usize::MAX, 7, |_| unreachable!());
// null; AllocationFailure (9): 8 + length cannot fit.
let p = ctx.alloc_str_with(isize::MAX as usize - 38, 7, |_| unreachable!());
// null; AllocationFailure (9): first length above the string layout ceiling.
let p = unsafe { ctx.array_from_bytes(std::ptr::null(), i32::MAX as usize + 1, 7) };
// null; Internal (11); no input read.
let p = ctx.array_with_capacity(isize::MAX as usize - 30, 1, 7);
// null; AllocationFailure (9): first capacity above the byte data layout ceiling.
```

Both allocator modes produce these results.
With allocation failure injected, `alloc_str_with(i32::MAX + 1, ...)` reaches the allocator and reports `injected allocation failure`.
`array_with_capacity(i32::MAX + 1, 1, ...)` also reaches its data allocator.
These probes establish the missing `i32` guard; they do not establish successful allocation at that size.
A header-only array probe stores `i32::MAX + 1` in `len`; `array_len` returns -2,147,483,648.
It is a cast probe, not a usable array with that capacity.

The completion prototype selects the common `i32::MAX` limit for both results.
Length `i32::MAX + 1` returns `TRAPPED` (4) with `AllocationFailure` (9), before input access, in both allocator modes.
At exactly the limit, the prototype proceeds to validation and allocation.
Success at exactly that length was not measured.
The contract must select a maximum and its failure semantics explicitly.

## 4. Allocation failure

`Context::alloc` returns null and records `AllocationFailure` (9) for a failed allocation or an invalid allocation layout.
It does not collect automatically or retry after collection.
There is no general Context byte budget at the pin.
The freed-handle diagnostic retention budget does not limit ordinary live allocations.

The existing completion cache uses `try_reserve_exact` and returns `TRAPPED` if that reservation fails.
The host-operation registry and task allocation also have explicit failure paths.
Other ordinary Rust `Vec`, map, formatting, and `Box` allocations can abort on host heap exhaustion.
Evidence: the module contract in `runtime/src/ffi.rs`, `Context::alloc`, and `host_complete_value`.
Real host heap exhaustion was not forced in this round.

The prototype reserves its eight completion bytes before the Context value allocations.
The fault probe uses `Context::fail_alloc_after`; it exercises Context allocation failure, not host heap exhaustion.

| Fault | Status | Trap | Extra live allocations before collection |
|---|---|---|---:|
| String allocation | `TRAPPED` (4) | `AllocationFailure` (9) | 0 |
| Byte-array header allocation | `TRAPPED` (4) | `AllocationFailure` (9) | 0 |
| Byte-array data allocation after header success | `TRAPPED` (4) | `AllocationFailure` (9) | 1 |

No fault publishes a completion or transfers the producer count.
The partial array header follows the existing `array_from_bytes` policy: explicit collection removes it.
A direct existing-helper fault also leaves one header; collection reduces its live allocation count to zero.
The contract must specify partial-result cleanup and whether failure traps the Context or completes with an Error.
Only the trap option was prototyped.

## 5. Binder, mirror, and two-reader program

At the pin, the binder rejects `--completion f=string` and `--completion 'f=u8[]'` under §178 rule 6.
It also rejects a completion result struct with a string-view field.
The baseline probe restores the pin's `bindgen/src/completion.rs` before those calls.

The CLI already preserves an arbitrary nonempty result token after `=`.
The prototype adds special result tokens `string` and `u8[]` to binder validation, checker validation, and completion layout verification.
They name language results rather than C object types. Both results use an eight-byte script handle layout.
The prototype leaves ordinary C scalar and struct spellings intact.

Existing C forms and directive spellings are:

```c
typedef struct View { const char *data; size_t length; } View;
typedef struct Slice { const uint8_t *data; size_t length; } Slice;
void take_view(View v);
void take_slice(Slice data);
void take_pair(size_t dataCount, const uint8_t *data);
```

```ts
// @subscript-c-string-view function="take_view" parameter="v" aggregate="View"
// @subscript-c-descriptor function="take_slice" parameter="data" aggregate="Slice" element="uint8_t" const=true
// @subscript-c-scalar-pair function="take_pair" parameter="data" element="uint8_t" const=true
```

The separate pair requires a count immediately before its matching pointer.
The count uses the pointer name plus `Count` or `_count`.
A descriptor uses pointer first and `size_t` second; its field names do not select the language type.
`const char*` selects a string view. `const uint8_t*` selects a byte descriptor.
Descriptor and string-view returns by value have no accepted provenance form at the pin.

The prototype mirror is generated from this header, with completion selections for `text`, `bytes`, and `fail`:

```c
#include <stdint.h>
#include <stddef.h>
#ifndef SUBSCRIPT_RUNTIME_H
typedef struct subscript_rt_completion {
  uint64_t context_id, operation_id;
} subscript_rt_completion;
#endif
void text(subscript_rt_completion ep);
void bytes(subscript_rt_completion ep);
void fail(int32_t which, subscript_rt_completion ep);
typedef struct View { const char *data; size_t length; } View;
typedef struct Holder { View label; } Holder;
void fill(Holder *out, int32_t which);
int32_t take(View v);
int32_t live(void);
```

Relevant generated mirror:

```ts
// @subscript-c-header include="measure.h"
// @subscript-c-completion function="text" result="string"
// @subscript-c-completion function="bytes" result="u8[]"
// @subscript-c-completion function="fail" result="int32_t"
// @subscript-c-string-view function="take" parameter="v" aggregate="View"
declare function text(): Promise<string>;
declare function bytes(): Promise<u8[]>;
declare function fail(which: i32): Promise<i32>;
declare class Holder { label: string; constructor(label: string); }
declare function fill(out: Holder | null, which: i32): void;
declare function take(v: string): i32;
declare function live(): i32;
```

The host supplies the Context before the script starts.
The same host behavior runs in both readers. Completion calls occur immediately inside the start functions.
The await still resumes through the checkpoint queue.
The script keeps both completed handles, calls `Context.collect()`, then awaits and reads both values.
The unchanged byte lengths and array elements establish completion-cache and array-data survival through that collection.

```c
#include "measure.h"
#include <assert.h>
static void *ctx;
extern uint32_t subscript_rt_complete_string(void*, subscript_rt_completion, const char*, size_t);
extern uint32_t subscript_rt_complete_bytes(void*, subscript_rt_completion, const uint8_t*, size_t);
extern uint32_t subscript_rt_complete_error(void*, subscript_rt_completion, const char*, size_t);
extern uint64_t subscript_rt_ctx_live_allocations(void*);
void setup(void *p) { ctx = p; }
void text(subscript_rt_completion ep) {
  assert(subscript_rt_complete_string(ctx, ep, "hello", 5) == 0);
}
void bytes(subscript_rt_completion ep) {
  uint8_t b[] = {0, 128, 255};
  assert(subscript_rt_complete_bytes(ctx, ep, b, 3) == 0);
}
View view(int32_t which) {
  static const unsigned char a[] = {255}, b[] = {128}, c[] = {226, 130};
  View views[] = {{(const char*)a, 1}, {(const char*)b, 1}, {(const char*)c, 2}};
  return views[which];
}
void fill(Holder *out, int32_t which) { out->label = view(which); }
int32_t take(View v) {
  int32_t sum = 0;
  for (size_t i = 0; i < v.length; i++) sum += (unsigned char)v.data[i];
  return sum;
}
void fail(int32_t which, subscript_rt_completion ep) {
  View v = view(which);
  assert(subscript_rt_complete_error(ctx, ep, v.data, v.length) == 0);
}
int32_t live(void) { return (int32_t)subscript_rt_ctx_live_allocations(ctx); }
```

The dev JIT uses equivalent `extern "C"` functions with the same signatures and byte constants.
A temporary accessor supplies its session Context to `setup` before `run_main`.
The C AOT runner supplies that Context through its pre-entry hook.
No host view pointer survives the completion call.

Measured script:

```ts
export async function main(): Promise<void> {
  const heldS = text(); const heldB = bytes(); Context.collect();
  const s = await heldS; const b = await heldB;
  print(`${s}:${s.length}:${b.length}:${b[0]}:${b[1]}:${b[2]}`);
  for(let i:i32=0;i<3;i++) {
   const h=new Holder(""); fill(h,i); const v=h.label; print(`view:${v.length}:${take(v)}`); print(`uri:${encodeURIComponent(v)}:${take(decodeURIComponent(v))}`);
   try { JSON.parse<string>(`\"${v}\"`); print("parsed"); } catch(e) { if(e instanceof Error) print(`json:${e.name}`); else throw e; }
   try { await fail(i); } catch(e) { if(e instanceof Error) print(`error:${e.message.length}:${take(e.message)}`); else throw e; }
  }
  Context.collect(); const before=live();
  for(let i:i32=0;i<10000;i++) { const x=await text(); const y=await bytes(); if(x.length!=5 || y[2]!=255) print("bad"); }
  Context.collect(); print(`balance:${live()-before}`);
 }
```

Both dev JIT and C AOT produce this byte-identical output:

```text
hello:5:3:0:128:255
view:1:255
uri:%FF:255
json:SyntaxError
error:1:255
view:1:128
uri:%80:128
json:SyntaxError
error:1:128
view:2:356
uri:%E2%82:356
json:SyntaxError
error:2:356
balance:0
```

The interpreter rejects the native dependency; it does not execute this boundary program.

## 6. Completion prototype and measured cost

Prototype APIs:

```c
subscript_rt_completion_status subscript_rt_complete_string(
    subscript_rt_context *ctx, subscript_rt_completion endpoint,
    const char *bytes, size_t length);
subscript_rt_completion_status subscript_rt_complete_bytes(
    subscript_rt_context *ctx, subscript_rt_completion endpoint,
    const uint8_t *bytes, size_t length);
```

The source stores a result-kind tag: raw value 0, void 1, string 2, byte array 3.
Both lowerings derive the tag from the checked completion result type.
The prototype carries this tag through the existing internal `is_void` ABI word.
A final implementation needs an explicit kind in its internal ABI and verified form.
The public endpoint remains two integer ids.

The adapter validates `TRAPPED`, `STALE`, `DUPLICATE`, then the result kind.
It checks the length before it reads bytes. A zero length permits a null pointer and creates a non-null empty value.
A nonzero length requires readable bytes for the duration of the call.
The string branch validates UTF-8, then copies. The byte branch copies without text validation.
The cache holds the Context value pointer, not the host pointer or host bytes.
The call queues waiters and releases exactly one producer count.

Prototype copy routine and public wrappers:

```rust
impl Context {
    pub(crate) fn host_result_kind(&mut self, handle: *mut u8, kind: u32) {
        if let Some(meta) = self.async_frames.get_mut(&(handle as usize)) {
            if let AsyncKind::Runtime(task) = &mut meta.kind {
                if let RuntimeTask::HostOperation(source) = task.as_mut() { source.result_kind = kind; }
            }
        }
    }
    pub(crate) unsafe fn host_complete_buffer(&mut self, endpoint: CompletionEndpoint, data: *const u8, len: usize, kind: u32) -> CompletionStatus {
        let handle = match self.host_operation_lookup(endpoint) { Ok(h) => h, Err(s) => return s };
        let meta = &self.async_frames[&handle];
        let AsyncKind::Runtime(task) = &meta.kind else { return CompletionStatus::Stale; };
        let RuntimeTask::HostOperation(source) = task.as_ref() else { return CompletionStatus::Stale; };
        if source.result_kind != kind { return CompletionStatus::Mismatch; }
        let pos = meta.create_pos_id;
        // Prototype policy: both results fit the script i32 length; strings require UTF-8.
        if len > i32::MAX as usize { self.trap(TrapKind::AllocationFailure, "completion length exceeds i32", pos); return CompletionStatus::Trapped; }
        let bytes = if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(data, len) } };
        if kind == 2 && std::str::from_utf8(bytes).is_err() { self.trap(TrapKind::StrRange, "completion string is not UTF-8", pos); return CompletionStatus::Trapped; }
        let mut result = Vec::new();
        if result.try_reserve_exact(8).is_err() { self.trap(TrapKind::AllocationFailure, "completion storage allocation failed", pos); return CompletionStatus::Trapped; }
        let value = if kind == 2 { self.alloc_str(bytes, pos) } else { unsafe { self.array_from_bytes(data, len, pos) } };
        if value.is_null() { return CompletionStatus::Trapped; }
        result.extend_from_slice(&(value as usize).to_ne_bytes());
        self.host_operation_finish(handle, Completion::Value(result));
        CompletionStatus::Ok
    }
}
/// Measurement prototype: copies a validated UTF-8 string into the Context.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_complete_string(ctx: *mut Context, endpoint: CompletionEndpoint, bytes: *const std::ffi::c_char, length: usize) -> CompletionStatus {
    unsafe { (&mut *ctx).host_complete_buffer(endpoint, bytes.cast(), length, 2) }
}
/// Measurement prototype: copies bytes into a Context-owned array.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_complete_bytes(ctx: *mut Context, endpoint: CompletionEndpoint, bytes: *const u8, length: usize) -> CompletionStatus {
    unsafe { (&mut *ctx).host_complete_buffer(endpoint, bytes, length, 3) }
}

```

| Completion attempt | Dev allocation mode | Ship allocation mode |
|---|---|---|
| String API on an `i32` source | `MISMATCH` (3); count stays 2 | Same |
| Value API with an `i32` on a string source | `MISMATCH` (3); count stays 2 | Same |
| String API on a byte-array source | `MISMATCH` (3); count stays 2 | Same |
| Bytes API on a string source | `MISMATCH` (3); count stays 2 | Same |
| Empty string or bytes | `OK` (0); count becomes 1 | Same |
| String API with `ff`, `80`, or `e2 82` | `TRAPPED` (4), `StrRange` (14) | Same |
| Bytes API with `00 80 ff` | `OK` (0); exact three bytes | Same |

These kind checks distinguish two eight-byte handles.
The existing raw-value API also rejects a string or byte-array source, rather than accepting arbitrary pointer bytes.

Each timing uses a fresh Context and source. The input is 1 MiB of ASCII `a`, already allocated before the timer starts.
The timer covers the complete-buffer call only: validation, value allocation, one copy, cache publication, and producer release.
It excludes source creation, await dispatch, output, collection, and Context destruction.
The string time includes a UTF-8 validation pass. It is not an isolated `memcpy` time.
The ship row uses the exact shared runtime function and ship allocator that the C AOT program calls.
It is a direct runtime measurement, not the elapsed time of the C AOT program.
Each result is read back and compared with all 1,048,576 input bytes after the timer stops.

| Allocator / result | Three samples, microseconds | Best of three, microseconds |
|---|---|---:|
| Dev / string | 287.084, 152.959, 140.875 | 140.875 |
| Dev / `u8[]` | 110.333, 32.583, 31.958 | 31.958 |
| Ship / string | 110.209, 104.833, 104.750 | 104.750 |
| Ship / `u8[]` | 43.583, 42.000, 42.083 | 42.000 |

There is no additional warmup. Each row uses its minimum of three samples.
These short allocation samples do not establish a throughput guarantee.

A separate direct-runtime count probe performs 10,000 completions per result and allocator mode.
Every source starts at count 2; each successful completion leaves count 1; the holder release removes the source.

| Mode / result | Live allocations before collect | Frames / endpoint entries | Live allocations after collect | Live bytes after collect |
|---|---:|---|---:|---:|
| Dev / string | 10,000 | 0 / 0 | 0 | 0 |
| Dev / bytes | 20,000 | 0 / 0 | 0 | 0 |
| Ship / string | 10,000 | 0 / 0 | 0 | 0 |
| Ship / bytes | 20,000 | 0 / 0 | 0 | 0 |

The baseline is an empty Context: zero allocations and bytes.
Values wait for explicit collection after their last source and script root ends.
The script also completes and awaits 10,000 strings plus 10,000 byte arrays in each reader.
Its post-collection allocation delta is zero in both readers.
These probes establish live-allocation and source-count balance, not zero arena reservation or zero Rust container capacity.

## 7. Runtime probe program

This temporary test sits inside `runtime/src/context/host_operation.rs`.
It can inspect the private frame registry, array layout, and cached Error representation.
`source` calls the internal constructor with the prototype result-kind word.
The test uses both `Context::new` and `Context::new_releasing`.

```rust
#[cfg(test)]
mod s184_measure {
 use super::*;
 use crate::ffi::*;
 fn source(ctx:&mut Context,kind:u32)->(*mut u8,CompletionEndpoint) { let mut ep=CompletionEndpoint::default(); let h=unsafe { subscript_rt_async_host_operation(ctx,if kind==0 {4}else{8},kind,42,[24u64,0,0,8,16,0].as_ptr(),&mut ep) }; (h,ep) }
 #[test]
 fn s184_runtime() { unsafe {
  for ship in [false,true] { let make=||if ship {Context::new_releasing()} else {Context::new()};
   for raw in [&[255u8][..],&[128u8][..],&[226u8,130][..]] {let mut c=make();let s=subscript_rt_str_from_view(&mut *c,raw.as_ptr(),raw.len() as u64,7);assert_eq!(c.str_bytes(s),raw);let mut j=vec![b'"'];j.extend(raw);j.push(b'"');let s=c.alloc_str(&j,7);let parser=subscript_rt_json_parse_begin(&mut *c,s,7);let e=subscript_rt_json_parse_failure(&mut *c,7);println!("HEAD utf ship={ship} raw={raw:?} parser={parser} text={:?} trap={:?}",String::from_utf8_lossy(c.str_bytes(e)),c.trap_record());}
   for raw in [&[255u8][..],&[128u8][..],&[226u8,130][..]] {let mut c=make();let(h,ep)=source(&mut c,0);let status=c.host_complete_error(ep,raw.as_ptr(),raw.len());if let Some(Completion::Exception(e))=&c.async_frames[&(h as usize)].completion {let p=e.exception.object as *const u8;let msg=p.add(16).cast::<*mut u8>().read_unaligned();println!("HEAD error ship={ship} raw={raw:?} status={status:?} field={:?} report={:?}",c.str_bytes(msg),e.exception.message);} }
   for (target,api) in [(0,2),(2,0),(3,2),(2,3)] {let mut c=make();let (h,ep)=source(&mut c,target);let status=if api==0 {subscript_rt_complete_value(&mut *c,ep,(&7i32 as *const i32).cast(),4)}else{c.host_complete_buffer(ep,b"x".as_ptr(),1,api)};println!("mismatch ship={ship} source={target} api={api} status={status:?} count={}",c.async_count(h));assert_eq!(status,CompletionStatus::Mismatch);}
   for kind in [2,3] {
    let mut c=make();let (h,ep)=source(&mut c,kind);let status=c.host_complete_buffer(ep,std::ptr::null(),0,kind);assert_eq!(status,CompletionStatus::Ok);let bytes=c.async_frames[&(h as usize)].completion.as_ref().unwrap().value().unwrap();let val=bytes.as_ptr().cast::<usize>().read_unaligned() as *mut u8;println!("empty ship={ship} kind={kind} pointer_null={} size={} count={}",val.is_null(),if kind==2 {c.str_bytes(val).len()}else{c.array_len(val) as usize},c.async_count(h));c.async_release(h,0);c.collect();assert_eq!(c.live_count(),0);
    for failure in 1..=if kind==2 {1}else{2} { let mut c=make();let (_,ep)=source(&mut c,kind);let before=c.live_count();c.fail_alloc_after(failure);let status=c.host_complete_buffer(ep,b"abc".as_ptr(),3,kind);println!("oom ship={ship} kind={kind} failure={failure} status={status:?} trap={:?} live_delta={}",c.trap_record().map(|t|t.kind),c.live_count()-before);assert_eq!(status,CompletionStatus::Trapped); }
    let mut c=make();let (_,ep)=source(&mut c,kind);let status=c.host_complete_buffer(ep,std::ptr::null(),i32::MAX as usize+1,kind);println!("limit ship={ship} kind={kind} status={status:?} trap={:?}",c.trap_record().map(|t|t.kind));
    let input=vec![b'a';1<<20];let mut samples=Vec::new();for _ in 0..3 {let mut c=make();let (h,ep)=source(&mut c,kind);let start=std::time::Instant::now();let status=c.host_complete_buffer(ep,input.as_ptr(),input.len(),kind);samples.push(start.elapsed().as_nanos());assert_eq!(status,CompletionStatus::Ok);let bytes=c.async_frames[&(h as usize)].completion.as_ref().unwrap().value().unwrap();let p=bytes.as_ptr().cast::<usize>().read_unaligned() as *mut u8;if kind==2 {assert_eq!(c.str_bytes(p),input)}else{assert_eq!(c.array_len(p),input.len() as i32);assert_eq!(std::slice::from_raw_parts((*(p as *mut ArrayHeader)).data,input.len()),input)}c.async_release(h,0);c.collect();assert_eq!(c.live_count(),0); }println!("copy ship={ship} kind={kind} ns={samples:?} best={}",samples.iter().min().unwrap());
    let mut c=make();let before=c.live_count();for _ in 0..10000 {let(h,ep)=source(&mut c,kind);assert_eq!(c.host_complete_buffer(ep,b"abc".as_ptr(),3,kind),CompletionStatus::Ok);assert_eq!(c.async_count(h),1);c.async_release(h,0);}println!("balance ship={ship} kind={kind} precollect={} frames={} operations={}",c.live_count(),c.async_frames.len(),c.host_operations.len());c.collect();println!("balance ship={ship} kind={kind} after={} baseline={before} bytes={}",c.live_count(),c.live_bytes());assert_eq!(c.live_count(),before);
   }
   for raw in [&[255u8][..],&[128u8][..],&[226u8,130][..]] {let mut c=make();let(_,ep)=source(&mut c,2);println!("invalid prototype ship={ship} raw={raw:?} status={:?}",c.host_complete_buffer(ep,raw.as_ptr(),raw.len(),2));println!("invalid trap={:?}",c.trap_record().map(|t|t.kind));}
   let mut c=make();c.fail_alloc_after(1);let p=c.alloc_str_with(i32::MAX as usize+1,7,|_|unreachable!());println!("HEAD string i32MAX+1 injected ship={ship} null={} trap={:?}",p.is_null(),c.trap_record().map(|t|(&t.kind,&t.message)));
   let mut c=make();c.fail_alloc_after(2);let p=c.array_with_capacity(i32::MAX as usize+1,1,7);println!("HEAD array capacity i32MAX+1 injected ship={ship} null={} trap={:?}",p.is_null(),c.trap_record().map(|t|(&t.kind,&t.message)));
   let mut c=make();let p=c.array_with_capacity(isize::MAX as usize-30,1,7);println!("HEAD array layout+1 ship={ship} null={} trap={:?}",p.is_null(),c.trap_record().map(|t|t.kind));
   let mut c=make();let a=c.array_from_bytes(std::ptr::null(),i32::MAX as usize+1,7);println!("HEAD array_from_bytes +1 ship={ship} null={} trap={:?}",a.is_null(),c.trap_record().map(|t|t.kind));
   let mut c=make();let s=c.alloc_str_with(usize::MAX,7,|_|unreachable!());println!("HEAD string usizeMAX ship={ship} null={} trap={:?}",s.is_null(),c.trap_record().map(|t|t.kind));
   let mut c=make();let s=c.alloc_str_with(isize::MAX as usize-38,7,|_|unreachable!());println!("HEAD string layout+1 ship={ship} null={} trap={:?}",s.is_null(),c.trap_record().map(|t|t.kind));
   let mut c=make();let a=c.array_new(1,7);(*(a as *mut ArrayHeader)).len=i32::MAX as u64+1;println!("HEAD synthetic array length ship={ship} i32={}",c.array_len(a));
   let mut c=make();c.fail_alloc_after(2);let a=c.array_from_bytes(b"x".as_ptr(),1,7);println!("HEAD array copy OOM ship={ship} null={} trap={:?} live={}",a.is_null(),c.trap_record().map(|t|t.kind),c.live_count());c.collect();println!("HEAD array copy OOM aftercollect={}",c.live_count());
  }
 } }
}

```

## 8. A struct result with a string field

A C field `View` occupies two native words. A script `string` field occupies one.
The existing raw completion copy cannot convert this layout.
At the pin, the binder rejects `Result { View text; }` as a completion result.
The output-pointer `Holder` probe shows an existing field conversion, but it does not supply a completion result conversion.

Such a completion needs a recursive conversion description with independent C and script offsets.
Each string field needs a byte pointer, a byte count, UTF-8 policy, and Context allocation.
The conversion must retain scalar fields, convert nested fields, root partial allocations, and specify failure cleanup.
The source must describe the converted script value for collection and any counted leaves.
A nested array or handle field adds its own ownership rules.

A later section can define this recursive conversion.
Top-level string and byte-array completions need no recursive C struct conversion.
Keep string-field completion structs rejected until that section defines and measures the conversion.

## 9. Decisions that a contract needs

| Decision | Measured options and consequence |
|---|---|
| Result spelling | Special `string` and `u8[]` tokens work through binder, mirror, checker, JIT, and C AOT; ordinary C view result returns remain rejected |
| Source identity | Explicit string/bytes kinds give `MISMATCH` for wrong APIs, including the two same-size handles |
| Value ownership | Copy-in works with ephemeral host buffers; cached handles and script roots survive explicit collection |
| String encoding | Existing views preserve invalid bytes; the prototype rejects them with status 4 and trap 14; existing Error reports use replacement |
| Byte encoding | Arbitrary bytes, including NUL and invalid UTF-8, survive unchanged |
| Null and empty | Null with length zero works for both APIs; nonzero length requires readable memory |
| Maximum length | The pin has inconsistent guards; the prototype applies `i32::MAX` to both and gives status 4 / trap 9 above it |
| Failure model | Injected Context allocation failure gives status 4 / trap 9 and no published result; completion with Error on allocation failure was not prototyped |
| Partial failure | Existing array copy leaves a header until collect; immediate partial-result cleanup needs an explicit rule |
| Counts and collection | 10,000 completions balance source counts and live allocations after explicit collection in each allocator mode |
| Error message encoding | `Error.message` preserves raw bytes but report text replaces them; a shared or distinct policy needs a decision |
| Struct fields | The raw C copy cannot materialize a string field; defer recursive field conversion while top-level results proceed |

## 10. Validation and final state

- The baseline binder rejects strings, byte arrays, and a string-field completion struct under §178 rule 6.
- The prototype binder emits both new Promise result types and the existing view/descriptor directive spellings.
- The program that consumes the generated mirror passes in dev JIT and C AOT with byte-identical output.
- Both completed handles survive explicit collection before their first await and result read.
- Stock TypeScript 5.9.2 accepts the generated mirror, measured client, and representation control with zero diagnostics.
- That check uses `prelude/lang.d.ts` and the repository `tsconfig.json` compiler options.
- The URI and representation controls agree across interpreter, dev JIT, and C AOT.
- The runtime probes pass in both allocator modes, with direct byte comparisons and source-count checks.
- Every production prototype file and temporary test file is reverted or removed.
- The final working tree contains only this measurement note. No commit was made.

## Implementation

At contract pin `da3d3133`, the binder rejects the `a354` header:

```text
subscript: bindgen: completion function `subCompletionText` result `string` is outside §178 rule 6: expected a mapped C scalar, a boundary class struct, or `void`
```

The pin binary rejects `a354` in both the dev checker and C emitter, with no stdout:

```text
error[S100]: completion function `subCompletionText` must return Promise<T> matching supported C result `string`
error[S100]: completion function `subCompletionBytes` must return Promise<T> matching supported C result `u8[]`
error[S100]: completion function `subCompletionTextError` must return Promise<T> matching supported C result `string`
error: 6 error(s)
```

The other three diagnostics reject direct awaits of these undeclared completion sources.
The generated mirror declares `Promise<string>` and `Promise<u8[]>`.
The binder test for these result tokens fails at the pin with the same result-set rejection.

The source, LIR instruction, and internal ABI carry `result_kind`: value=0, void=1, string=2, bytes=3.
The lowering derives the kind from the completion directive result token.
The LIR verifier compares that kind with the separately checked foreign result type.

The new runtime tests exercise both allocator modes and all sixteen source/API kind pairs.
Invalid input keeps the source count at two, publishes no result, and permits a valid retry.
Every Context allocation failure point restores the pre-call live allocation count and byte count before return.
The byte-array data failure releases its partial header immediately.
A source roots its copied value across collection. Await and aggregate reads transfer the value to ordinary script roots.
A dropped, unread value is released with its source.
A size-zero discarded await leaves the copied value unread and releases it with the source.

The release copy probe uses the section 6 workload: fresh Context and source, preallocated 1 MiB ASCII input, three calls per row.
The timer covers validation, allocation, copy, cache publication, and producer release.
A byte comparison follows each timed call. Source creation and collection remain outside the timer.

| Allocator / result | Three samples, microseconds | Best of three, microseconds | Measurement best, microseconds |
|---|---|---:|---:|
| Dev / string | 108.458, 60.833, 56.125 | 56.125 | 140.875 |
| Dev / `u8[]` | 23.000, 22.666, 22.791 | 22.666 | 31.958 |
| Ship / string | 59.875, 56.375, 56.041 | 56.041 | 104.750 |
| Ship / `u8[]` | 27.084, 23.208, 22.666 | 22.666 | 42.000 |

These samples do not establish a throughput guarantee.

The dev JIT and C AOT produce the committed `a354` output, byte for byte:

```text
hello:5:3:0:128:255
hello:hello
buffer failure
empty:0:0
```

The original seven runtime gates take 0.16 seconds together, including 40,000 completions across both allocator modes.
The seventeen binder completion tests take 0.22 seconds together.
The eleven completion LIR tests take 0.21 seconds together.
These elapsed test times exclude the Rust build.
The copy-cost probe is excluded from the ordinary test set.
The generated C header declares both functions and both new status values.
The C tutorial and generated corpus index describe the new surface.
The LIR text golden changes: `a347` instructions name the result kind, and `a354` adds its lowered module.
No existing `.expected` output changes.

The three added runtime tests use eight, four, and two sources, respectively, across both allocators.
They use small buffers; injected allocation failure prevents a boundary-length buffer allocation or input read.
Isolated test-binary elapsed times, including process startup and excluding the Rust build:

- `discarded_await_releases_copied_buffers_with_observed_value_controls`: 0.008046 seconds.
- `bytes_error_drop_traps_with_await_controls`: 0.003396 seconds.
- `admitted_length_boundary_reaches_allocation_failure_without_reading_input`: 0.009798 seconds.
