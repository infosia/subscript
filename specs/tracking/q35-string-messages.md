# Worker messages carry `string` fields by copy

Status: **landed 2026-09-03** against `specs/blocks/compiler.md` §84,
`specs/blocks/stdlib.md` §16.2, and `specs/blocks/collisions.md` Q35.
Origin: owner request 2026-09-03. Contract `e645b67`, amended `87927e2` after the review;
implementation `a3b5bc2`, fixes `587d6da`.

## The request and the v1 rule

stdlib §16.2 (v1) excluded `string` fields from a message class:
a `string` value is a handle to a Context-owned string object, and
the runtime copied the message's C-layout bytes as they were, so a
string slot would have carried a pointer into the sender's Context.

## What landed

- Checker: `string` is a transferable leaf at a message class's top
  level and inside `FixedArray<string, N>`; reference, growable-array,
  function, and nullable fields keep the rejection.
- Layout: one function in `codegen/src/layout.rs` lists a message
  class's string slot offsets from its C layout; both tiers emit a
  static descriptor `{ payload_size, string_slot_count,
  string_slot_offsets }` per message class and pass its address to
  `subscript_rt_worker_spawn` in place of the two `u64` sizes.
- Runtime: the queue record is the fixed bytes followed by each
  slot's length and bytes; `materialize` allocates the object and one
  fresh Context-owned string per slot; a null handle posts as the
  empty string; a descriptor with count 0 produces the pre-§84 record.
  `runtime/include/subscript_runtime.h` regenerated.
- Corpus: `a182-worker-string-message` (`js-comparable: no Q35`; Red at
  `ccc47f7`/`e645b67`: S100 "message class `StringMessage` is not
  transferable: innermost field `StringMessage.text` has
  non-transferable type `string`" at line 7); `r182-worker-reference-field`,
  `r183-worker-growable-array-field`; retired:r108.
- Tests: five runtime cases in `runtime/src/worker.rs`; a codegen
  descriptor test (`[0, 16, 24]` for a182's class, both tiers, equal
  to the layout's offsets); the divergence text of
  `WorkerContextAffinity` revised.
- Counts: accept `.ts` 179 → 180; rejects 171 → 172 (one retired, two
  added).

## Gates (this host, `587d6da`)

- debug (the coding agent's final run): 66 suites, 1,254 passed, 0
  failed, 1 ignored, 2,229 s.
- release: 66 suites, 1,252 passed, 0 failed, 1 ignored, 343 s.
- Zero-warning build; fmt, `tsc`, hygiene exit 0; clippy 7 / 18 / 13.
- No pre-existing golden or `.expected` moved; a112 byte-identical.

## Review round 1 (fresh no-context subagent)

§84.4 holds the record. Execution-verified on both tiers with no
finding: strings across Contexts under `collect`, rebinds, and
`join`; a 1 MiB string; one handle in four slots; close/join
ordering; a trapping worker. MAJOR: the runtime tests compared the
serializer with itself and reached no rejection arm; the dev-tier
descriptor test read a side channel of its own inputs. MINOR: the
header's struct body as a second copy; the hard-coded 24-byte image;
no pin for two message classes on one worker; the Red text in this
note named r108's class; "specially" in the divergence reason.
Adjacent defect outside this diff, both tiers: a `string` field with
no initializer holds a null handle, and `print` of it emits nothing;
needs its own request and corpus entry. Fixed in `587d6da`: the
record facts are hand-written; every descriptor rejection arm and
every malformed-record arm of `materialize` has a control;
an allocation failure in `materialize` through the P21 fault
injection (the queue record's `try_reserve_exact` failure has no
injection path; recorded in §84.3 item 5); the side channel
is gone; a header `offset_of!` test; a182 holds a second worker with
two distinct message classes (`u8`-led layout, offsets 8, 24, 32, 40).
