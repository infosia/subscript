<!-- §84 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 84. Worker messages carry `string` fields by copy

*(Owner decision 2026-09-03.)* Origin: owner request. `stdlib.md`
§16.2 (v1) excludes `string` fields from a message class and says
"widen only with evidence".

Measured at `ccc47f7`, this host: a message class with `text: string`
fails at `Worker.spawn` with S100 "message class `TextMessage` is not
transferable: innermost field `TextMessage.text` has non-transferable
type `string`" (r108 pins it). A `FixedArray<string, 2>` field on a
reference class checks and runs outside a message.

Why the v1 exclusion exists. A `string` value is a handle to a
Context-owned string object (`ctx.alloc_str`; Q5). A message class
lowers to its C layout, and the runtime copies `payload_size` bytes
into a queue record (`runtime/src/worker.rs`, `Queue::post_fixed`)
and copies them back into a fresh `CLASS_WORKER_MESSAGE` allocation
on the receiving side (`materialize`). A string slot would carry a
raw pointer into the sender's Context, which §38 forbids.

### 84.1 Rule

1. **`string` is transferable.** A message class can hold `string`
   fields and `FixedArray<string, N>` fields. A value class still
   holds no string (C2), so a string slot sits at the message class's
   top level or inside one of its fixed arrays, and nowhere deeper.
   Reference, growable-array, function, and nullable fields stay
   non-transferable.
2. **Delivery copies the bytes.** `post` reads each string slot of
   the payload on the sender's Context, and the queue record holds
   the fixed payload bytes followed by every string's byte length
   and bytes, in slot order. The receiving side allocates the message
   object, then allocates one fresh Context-owned string per slot
   from the record's bytes and writes its handle into the slot. The
   sender's object and its strings are unaffected. A null handle in a
   string slot is sent as the empty string. The record copies the
   fixed payload bytes verbatim. A padding byte of the payload is
   indeterminate, and the record holds that byte as it is.
3. **A message layout descriptor replaces the two payload sizes.**
   Generated code hands `subscript_rt_worker_spawn` one static
   descriptor per message class in place of each `u64` size:
   `{ payload_size: u64, string_slot_count: u64, string_slot_offsets:
   *const u64 }`, where the offsets are the byte offsets of every
   string slot in the class's C layout, including the `N` slots of
   each `FixedArray<string, N>` field, in ascending order. The
   descriptor is program-image data; both tiers emit it from the same
   layout the class already has, and the two tiers' descriptors are
   byte-identical. A class with no string slot has count 0, and its
   record is the fixed bytes alone, as today.
4. **Queues stay unbounded and posting never blocks.** A record's
   length is the fixed size plus the string bytes; the Q29 limits
   bound the fixed part, and a string's own length bound (`i32`)
   bounds each slot.
5. **Nothing else moves.** Entry shape, Context affinity, `wait` /
   `poll` / `close` / `join`, and the trap model (§39) do not change.
   The reference interpreter keeps its worker exclusion (a112's row).
   Hot reload keeps its refusal while a worker lives.

### 84.2 Sites

- `compiler/src/check/expr.rs` `non_transferable_message_field`:
  `Type::Str` is a transferable leaf.
- The checker or `codegen/src/layout.rs`: one function that lists
  the string slot offsets of a message class from its layout, used by
  both tiers.
- `codegen/src/lower/func.rs` (spawn, dev JIT) and
  `codegen/src/cemit.rs` (spawn, C): emit the descriptor as data and
  pass its address.
- `runtime/src/ffi.rs` `subscript_rt_worker_spawn`: two descriptor
  pointers. `runtime/src/worker.rs`: `Queue` holds the descriptor;
  `post_fixed` serializes; `materialize` allocates the strings.
  `runtime/src/host_header.rs` if the generated header documents
  spawn.
- `stdlib.md` §16.2, `collisions.md` Q35, `language_reference.rs`.

### 84.3 Corpus and gate (pre-registered exit criteria)

1. `corpus/accept/a182-worker-string-message.ts` + `.expected`: a
   message class with `text: string`, `count: i32`, and
   `tags: FixedArray<string, 2>`; the parent posts a message whose
   text holds non-ASCII bytes and prints the text's byte length; the
   worker replies with a new message whose text is a concatenation
   made on the worker Context, an empty string in one tag, and the
   count plus one; the parent rebinds its own message's `text` after
   `post` and prints both the reply and its own object to show that
   neither side sees the other's change; a second round trip on the
   same worker. Red at `ccc47f7`: the S100 above. `tsc: accepts`;
   `js-comparable: no Q35`. The interpreter exclusion row names it as
   a112's does.
2. `corpus/reject/r182-worker-reference-field.ts`: a message class
   with a reference-class field; S100 with the
   `WorkerContextAffinity` block; `tsc: accepts`.
3. `corpus/reject/r183-worker-growable-array-field.ts`: a message
   class with a `T[]` field; the same S100; `tsc: accepts`.
4. r108 retires (`retired:r108` in `collisions.md` Q35 and
   `stdlib.md` §16.2, the harness row, the reference row).
5. Runtime unit tests in `runtime/src/worker.rs`, each comparing two
   separately derived facts: the record `post` writes for a two-slot
   message equals a **hand-written** byte vector (fixed bytes, then
   each slot's `u64` length and bytes). A test that asserts record
   bytes must supply the payload as a byte buffer that the test
   defines in full. Every padding byte of that buffer must hold a
   non-zero value, so the assertion pins the verbatim copy of rule 2.
   A test must not post a Rust struct value, because the compiler
   leaves the struct's padding indeterminate. `materialize` fed a
   hand-written record yields strings equal by content with handles
   distinct from any sender handle; a null slot arrives as the empty
   string; an empty string round-trips; a descriptor with count 0
   produces the fixed bytes; a `FixedArray<string, N>` slot set.
   Positive controls: every descriptor rejection arm (null
   descriptor, unrepresentable size or count, null offsets with a
   count above 0, a slot outside the payload, offsets not ascending)
   and every malformed-record arm of `materialize` (a short length
   word, a length past the record, trailing bytes) is reached by a
   hand-built input and reports; an allocation failure in
   `materialize` through the P21 fault injection reports
   `AllocationFailure`. The queue record's own `try_reserve_exact`
   failure (`PostResult::AllocationFailed`) has no injection path,
   because P21 controls `Context::alloc` only; recorded, not tested.
   *(Amended
   2026-09-03 after the review: the first tests posted through
   `post_fixed` and read back through `materialize`, so a matched
   change to both sides stayed green, and no rejection arm ran.
   Amended 2026-09-05: the two-slot test posted a Rust struct value,
   so its assertion read the struct's indeterminate padding. The
   debug profile passed and the release profile failed.)*
6. Codegen test: the C tier's descriptor for a182's message class
   equals hand-derived offsets and equals the field offsets the
   layout reports; the dev tier's descriptor is witnessed by a182
   running on the dev tier, and holds no test-only side channel that
   records its own inputs. A runtime test asserts the generated
   header's descriptor struct text against `offset_of!` and
   `size_of` of the Rust `#[repr(C)]` definition (0, 8, 16; 24).
   *(Amended 2026-09-03 after the review.)*
6a. a182 also holds a second worker whose `In` and `Out` are two
   different message classes, one with strings in a `u8`-led layout
   (a handle after a `u8`, an `i64`, a `FixedArray<string, 3>`) and
   one with no strings, so an input/output descriptor swap fails
   the golden.
7. Gates: both profiles, zero-warning build, fmt, `tsc`, hygiene,
   clippy at 7 / 18 / 13, and `tools/hygiene.sh`. No pre-existing
   golden moves.

### 84.4 Review round 1, 2026-09-03

Fresh review of `e645b67..a3b5bc2`, execution-verified on both tiers:
strings across Contexts under `collect`, rebinds, `join`, a 1 MiB
string, the same handle in four slots, close/join ordering, a
trapping worker — no finding. MAJOR: the runtime tests compared the
serializer with itself and reached no rejection arm (item 5
amended); the dev-tier descriptor test read a `#[cfg(test)]` record
of its own inputs (item 6 amended: side channel removed). MINOR: the
header's struct body is a second copy (item 6: an `offset_of!` test);
the dev tier hard-codes the 24-byte image (the same test names the
constants); no pin for two message classes on one worker (item 6a);
the tracking note's Red text named r108's class; the divergence
reason said "specially" (rewritten); and, outside this diff, a null
`string` handle in a field with no initializer makes `print` emit
nothing on both tiers (a shared defect under principle 12; it needs
its own request and its own corpus entry).
