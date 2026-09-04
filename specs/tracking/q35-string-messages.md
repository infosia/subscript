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
  **This figure is wrong.** `587d6da` added
  `post_two_string_slots_matches_a_hand_written_record`, and that
  test fails in the release profile at that pin. See the correction
  below.
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

## Release-profile correction, 2026-09-05

Measured at `f4c296c` on this host, after `git pull`:
`cargo test --offline --release --workspace --no-fail-fast` reports
66 suites, 1,228 passed, 1 failed, 1 ignored. The debug profile
reports 66 suites, 1,231 passed, 0 failed, 1 ignored, 615 s. Fmt,
hygiene, and the zero-warning build exit 0. Clippy is 7 / 18 / 13.
`generated-docs/` regenerates with no diff.

The one failure is
`worker::tests::post_two_string_slots_matches_a_hand_written_record`
at `runtime/src/worker.rs:799`. The test builds
`#[repr(C)] TwoStringMessage { first: *mut u8, count: i32, second:
*mut u8 }` as a Rust value. That struct holds four padding bytes at
offset 12, and the compiler leaves them indeterminate. `post_fixed`
copies `payload_size` bytes verbatim, so the record holds the
padding. The debug profile found zero bytes on the stack. The
release profile found `246, 127, 0, 0`. The failure repeats 3 of 3
runs in release.

The check therefore read a value that §84.1 rule 2 does not promise.
§84.1 rule 2 and §84.3 item 5 are amended: the record copies the
fixed payload bytes verbatim, and a test that asserts record bytes
must supply the payload as a byte buffer that the test defines in
full, with a non-zero value in every padding byte.

No corpus entry, golden, or `.expected` moves. The emitted C copies
the same indeterminate padding, and the receiver reads fields only,
so no tier-differential output changes.

## The fix and the gate, 2026-09-05

The test now builds the fixed payload as a byte array of
`size_of::<TwoStringMessage>()` bytes. It writes the two handles, the
`i32`, and `[1, 2, 3, 4]` in the four padding bytes, then posts that
buffer. The hand-written expected record holds the same four bytes, so
the assertion pins the verbatim copy. The test fails if `post_fixed`
zeroes the fixed bytes. Red measured again at `f4c296c` in release; the
padding read `247, 127, 0, 0` on that run and `246, 127, 0, 0` on the
first. The value is indeterminate, so the two runs agree.

The other `post_fixed` test posts a `u64`, which has no padding. Every
other record test builds a `Vec<u8>` by hand. The class has no third
site.

Gates on this host at the fix:

- debug: 66 suites, 1,231 passed, 0 failed, 1 ignored, 674 s.
- release: 66 suites, 1,229 passed, 0 failed, 1 ignored, 167 s.
- Zero-warning build in both profiles; fmt and hygiene exit 0; clippy
  7 / 18 / 13; `generated-docs/` regenerates with no diff.
- No corpus entry, golden, or `.expected` moved.

Review round 1 on the diff raised one MINOR: a 107-column comment
against the file's 72-column wrap. Fixed.
