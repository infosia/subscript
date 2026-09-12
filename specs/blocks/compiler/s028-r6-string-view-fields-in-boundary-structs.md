<!-- §28 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 28. R6 — string-view fields in boundary structs

Owner decision 2026-08-01 (downstream report R6, blocking; also a
**bug report accepted as such**): a `string`-typed field in a
pointer-passed boundary struct mirrors today but the shared lowering
places the string handle in the struct storage instead of the
16-byte `{data,len}` view — every later field reads at a wrong
offset in C, identically in both tiers, so the differential gate
cannot see it. That is accept-and-miscompile against invariant 1.

### 28.1 Rules

1. **No accept-and-miscompile, anywhere.** The mirror may accept a
   string field only where the lowering below exists; every other
   position — by-value boundary-struct crossings with string
   fields, arrays of such structs, and any aggregate position not
   listed — fails loud at bind time with the §13-style named
   construct. An audit test enumerates the mirror's string-field
   positions and asserts accepted ⇒ lowered.
2. **R6.1 — script → C (pointer-passed).** At the call site, both
   tiers build a C-layout scratch struct: the string field expands
   to the view `{const char* data; size_t len}` pointing at the
   language string's bytes — zero-copy, valid for the duration of
   the call (the parameter-position string-view rule at field
   granularity); remaining fields copy at their C offsets. The
   callee copies if it retains, as with parameter views.
3. **R6.2 — C → script.** Reading a `string`-typed field from a
   C-filled struct materializes a language string from the stored
   view (the existing view-to-string copy-in, as the callback
   trampoline does for messages); an all-zero view reads as the
   empty string. May land with R6.1 or immediately after — the
   fail-loud rule covers whichever direction is not yet lowered.

### 28.2 Corpus and staging — Red first

The fix starts by pinning the bug: `a97` (accept) constructs a
descriptor-shaped fixture struct — string-view field first, scalar
fields after it — passes it to a C checker that returns the scalar
fields and the viewed bytes, and prints them. Under the pre-fix
compiler this entry's golden CANNOT be captured correctly (the
scalars come back wrong); the entry lands **with** the fix and its
golden proves the offsets. The read direction adds a C-filled record
carrying a message view (`a98` or folded into `a97` — implementer's
call, contract requires both directions pinned). The fixture gains
the checker/filler; mirror regenerated via `subscript bind`.

### 28.3 Exit criteria (pre-registered)

1. `a97` (and the read-direction pin) byte-identical under both
   tiers; the scalar-after-string values prove the C offsets.
2. The audit test: every mirror-accepted string-field position has
   a lowering; by-value structs and arrays of string-field structs
   fail loud at bind time, unit-tested.
3. The downstream shape — `SGPUStringView label` first, `uint64_t`
   fields after — is covered verbatim by the fixture struct.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep green.
