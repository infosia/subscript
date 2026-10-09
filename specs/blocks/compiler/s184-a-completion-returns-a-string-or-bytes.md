<!-- §184 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 184. A completion returns a string or bytes

*(Added 2026-10-09.)* Origin: the owner's review of host I/O on
2026-10-09: a host that reads a file or an HTTP body needs to return
text or bytes, and §178 rule 6 admits only scalars, C-layout structs,
and `void`. On 2026-10-09 the owner selected this section and chose a
status (not a trap) for invalid input. The measurement round at
`339f01b4` is `specs/tracking/s184-string-completion-measurement.md`.

Problem: a host completion cannot return a `string` or a `u8[]`. The
binder rejects `--completion f=string` and `--completion 'f=u8[]'`. A
host must split the result into a handle and a second synchronous
call. The measurement prototype returned both in the dev JIT and C AOT
with byte-identical output, a zero allocation balance after 10,000
completions of each, and a 1 MiB copy of 32–141 µs.

### 184.1 Rules

1. **Results.** A completion result can be `string` or `u8[]`, in
   addition to the results of §178 rule 6. The binder takes the result
   tokens `string` and `u8[]`; the mirror declares `Promise<string>`
   and `Promise<u8[]>`. A C struct result with a string-view, a
   descriptor, or an array field stays rejected (184.3 item 1).
2. **Two functions.** The runtime C API adds:
   - `subscript_rt_complete_string(ctx, endpoint, const char* bytes, size_t length)`
   - `subscript_rt_complete_bytes(ctx, endpoint, const uint8_t* bytes, size_t length)`

   Each copies the bytes into a new Context-owned value. The host
   buffer is read only during the call. A zero length accepts a null
   pointer and gives an empty value. A nonzero length requires
   `length` readable bytes.
3. **Result kind.** A source records its result kind: value, `void`,
   `string`, or bytes. The kind is an explicit field of the source,
   the LIR, and the internal runtime ABI; the LIR verifier checks it
   against the checked result type. A completion function of another
   kind returns `MISMATCH` with no effect. `subscript_rt_complete_value`
   on a `string` or bytes source returns `MISMATCH`.
4. **Host input is checked, and a defect is a status.** Two new
   statuses, each with no effect (the source stays pending and the
   host can complete it again):

   | Status | Condition |
   |---|---|
   | `INVALID_UTF8` (5) | `subscript_rt_complete_string` or `subscript_rt_complete_error` with bytes that are not valid UTF-8 |
   | `TOO_LARGE` (6) | a length above `i32::MAX` (2,147,483,647) bytes, for `string`, bytes, or an `Error` message |

   The checks run in this order: `TRAPPED`, `STALE`, `DUPLICATE`,
   `MISMATCH`, `TOO_LARGE`, `INVALID_UTF8`. `TOO_LARGE` is decided
   before the bytes are read. This amends §178 rule 7 for
   `subscript_rt_complete_error`: it no longer stores invalid UTF-8.
5. **Allocation failure** keeps the Context memory policy: the
   Context traps with `AllocationFailure` (9), and the call returns
   `TRAPPED`. The completion publishes nothing, and each allocation
   that the call made is released before it returns: the live
   allocation count equals its value before the call.
6. **Roots and counts.** A completed source roots its value until the
   source ends; an awaited value is an ordinary script value after
   that. §178 rules 2, 8, 10, and 11 apply with no change: a dropped
   handle frees the value at the source release, and a dropped
   `Error` completion traps as §178 rule 8 states.
7. **Tiers.** The dev JIT and C AOT give the same output, statuses,
   and traps. The interpreter has no foreign call (§68.7).

### 184.2 Acceptance

1. Red first, at the contract pin: accept entry `a354` with an
   interop header and mirror that complete a `string` and a `u8[]`
   (with `00`, `80`, and `ff` bytes), a held handle across
   `Context.collect()`, `Promise.all` over two `string` sources, an
   `Error` completion, and an empty value of each; `interpreter: no`.
   The binder's mirror for that header is regenerated and committed.
2. Unit tests of the runtime, in both allocator modes:
   - each row of rule 4 and each kind pair of rule 3, with the status
     and the unchanged source count (core principle 9: the source is
     built in the test, not read back from the call);
   - `TOO_LARGE` at `i32::MAX + 1` with a pointer the call must not
     read, and a firing control at a length the call accepts;
   - each allocation failure point of rule 5, with the live
     allocation count compared to its value before the call;
   - 10,000 completions of each kind, with the live allocation count
     equal to the baseline after `Context.collect()`.
3. Binder tests: `string` and `u8[]` accepted; a struct with a
   string-view field rejected with its message.
4. Cost: record the 1 MiB completion time of each kind against the
   measurement (release, best of three).
5. Goldens: the C tutorial's completion text, the generated docs, and
   the LIR text golden (the `a347` completion lines name the result
   kind, and `a354` is added) move. No other `.expected` output moves.

### 184.3 Open

1. **A struct field with a string or an array** needs a recursive
   conversion with separate C and script offsets, a UTF-8 rule for each
   field, and partial-failure cleanup (measurement section 8). It
   waits for a section of its own.
2. **Other string boundaries keep invalid bytes.** A bound string-view
   parameter, a string field written through a C pointer, and
   `subscript_rt_str_from_view` copy bytes with no UTF-8 check
   (measurement section 2). This section does not change them.

### 184.4 Sections this one amends

- §178 rule 6: `string` and `u8[]` results are admitted.
- §178 rule 7: two new statuses, and `subscript_rt_complete_error`
  checks its message.
