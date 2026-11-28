<!-- §360 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 360. A byte conversion accepts a scalar

*(Added 2026-11-28.)* Origin: a downstream proposal of 2026-11-28
(<https://github.com/infosia/subscript-typegpu>), and the owner
decision of the same day to proceed.

Problem, measured at `135974d5`:

1. **A program cannot read or write the bytes of one scalar.**
   `Context.fromBytes<u32>(bytes, 4)` gives ``error[S100]:
   `Context.fromBytes<T>` cannot use `u32`; it is not a @ValueType
   value class or FixedArray``. `Context.bytesOf<u32>(a)`,
   `Context.bytesOf<boolean>(b)`, and `Context.bytesOf<E>(E.A)` for
   an enum `E` give the same diagnostic.
2. **The same storage in a one-element array is accepted.**
   `Context.fromBytes<FixedArray<u32, 1>>(bytes, 4)[0]` gives 22 for
   the bytes `11,0,0,0,22,0,0,0`. `FixedArray<f32, 1>`,
   `FixedArray<boolean, 1>`, and `FixedArray<E, 1>` are accepted
   also. `FixedArray<S, 1>` and `S` have the same storage, so the
   rejection has no reason in the layout.
3. **`tsc` accepts the scalar call.** `prelude/lang.d.ts` declares
   the three members with a type parameter `T`. The rejection is a
   divergence.
4. **Ten downstream call sites have the wrapper**, at the downstream
   commit `ef54ad6`: the library byte writer, the first program of
   its tutorial, five readback programs, and two examples. Each
   wraps one `u32` or one `f32` in `FixedArray<S, 1>`.

### 360.1 Rules

1. **The type argument `T` of `Context.bytesOf`, `Context.bytesInto`,
   and `Context.fromBytes` can be a scalar.** A scalar is a sized
   numeric (`i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`,
   `f16`, `f32`, `f64`), a `boolean`, or an enum. The types that the
   three members accepted before this section do not change.
2. **One rule states the accepted types.** `T` is accepted if each
   byte of its storage is a sized numeric, a `boolean`, an enum, or
   padding (`stdlib.md` §18.1 rule 2). The top level of `T` has no
   rule of its own.
3. **`sizeof(T)` of a scalar is the size of the scalar.** A scalar
   has no padding range.
4. **The bytes of a scalar are the bytes of `FixedArray<S, 1>`.**
   `bytesOf<S>(v)` returns the bytes of
   `bytesOf<FixedArray<S, 1>>([v])`. `fromBytes<S>(b, o)` returns the
   value of `fromBytes<FixedArray<S, 1>>(b, o)[0]`. `bytesInto<S>`
   writes the bytes that `bytesOf<S>` returns.
5. **A `boolean` and an enum follow the rule of a field.**
   `fromBytes<boolean>` gives the result that a `boolean` field of a
   value class gives for the same byte. `fromBytes<E>` gives the
   result that a field of type `E` gives for the same bytes (§200
   records a value that is no member).
6. **The argument `value` has the type `T` exactly** (`stdlib.md`
   §18.1 rule 3). A numeric literal takes the type `T`. A value of a
   different sized numeric is the S100 of that rule.
7. **The range trap does not change** (`stdlib.md` §18.1 rule 6). The
   message has the offset, the size of the scalar, and the length.
8. **A type parameter follows its instance** (§143 rule 2a). An
   instance with a scalar is accepted.
9. **Each other rejection stays**: a reference class, a `string`, a
   handle, a nullable reference, a function, a string-literal alias, a boundary
   struct, a dynamic array, and a `FixedArray` or a value class with
   one of those in its storage.
10. **The diagnostic of a rejected `T` names the accepted types.**
    The text is ``"`Context.<name><T>` cannot use `<T>`; it is not a
    @ValueType value class, a FixedArray, a sized numeric, a boolean,
    or an enum"``.
11. **The three tiers agree.** The interpreter, the development
    tier, and the ship tier give the same bytes and the same values.
12. **A nullable scalar is not a type** (`collisions.md` C7). `u32 |
    null` as `T` gives the S011 of the type, as it does in each
    other position.

### 360.2 Acceptance

1. **Red first.** At the pin, the program of problem 1 gives the
   S100 diagnostics.
2. **An accept entry**, `corpus/accept/a513-scalar-bytes.ts`. For
   each of the eleven sized numerics it prints the length of
   `bytesOf<S>`, the bytes, the value of a `fromBytes<S>` of those
   bytes, and the bytes after a `bytesInto<S>` at an offset that is
   not 0. For each type, one line compares the bytes with the bytes
   of the `FixedArray<S, 1>` form.
3. The entry has these values: a negative `i32`, an `f32` from the
   bytes `1,0,192,127` (a NaN with a payload; `bytesOf<f32>` returns
   the same bytes), a `u64` above 2^53, and a `fromBytes<u32>` at
   an offset that is not a multiple of 4.
4. The entry has a `boolean` and an enum: the bytes of each, and a
   round trip of each. A `fromBytes<boolean>` of the byte 2 gives
   `true`, and its `bytesOf<boolean>` gives the byte 1, as the
   `FixedArray<boolean, 1>` form does at the pin.
5. The entry has a literal argument (`Context.bytesOf<u32>(25)`), an
   argument that is an expression (`a + 1`), and a call in a generic
   function with a scalar instance.
6. **A trap entry**, `corpus/trap/t123-scalar-bytes-range.ts`:
   `fromBytes<u32>` at offset 6 of an array of 8 bytes. Each tier
   gives kind 1 with the message of `t51` in its form.
7. **A reject entry**, `corpus/reject/r566-bytes-of-string.ts`:
   `Context.bytesOf<string>("x")` gives S100 with the text of rule
   10. Its header states what `tsc` does, measured.
8. Unit tests in the same commit: `bytesOf<u32>` with an `i32`
   argument is S100 (rule 6); a nullable reference class, a string-literal
   alias, and a dynamic array are S100 (rule 9); `u32 | null` is
   S011 (rule 12); a type parameter
   with a `string` instance is rejected, and with a `u32` instance
   is accepted (rule 8).
9. `r138`, `r139`, and `t51` do not change, except for the
   diagnostic text of rule 10 where an entry has it.
10. The matrix of `compiler/tests/generic_tsc_matrix` and
    `compiler/tests/builtin_call_tsc_agreement.rs` record that the
    checker and `tsc` agree for a scalar `T`.
11. The texts that state what the three members accept are correct:
    `stdlib.md` §18.1, the rejection alternative blocks (the block
    for a call with no type argument shows `Context.bytesOf<i32>(1)`),
    the API text and the language reference (generated).
12. Cost: one accept entry and one trap entry. Each is a program of
    less than 150 lines with no loop above 16 turns. The added gate
    time is the build and the run of two small programs in each
    tier.
13. `tools/gate.sh full` passes.

### 360.3 Sections this one amends

- `stdlib.md` §18.1 rule 2: the top-level limit on `T` goes. Rule 2
  of this section replaces it.
- `stdlib.md` §18.1 rule 7: the development tier copies a scalar
  through a store or a load of its representation.

### 360.4 Not in this section

- A conversion between byte orders. The bytes are the storage bytes
  in memory order (`stdlib.md` §18.1 rule 4).
- A dynamic array as `T` (`u32[]`). Its storage is a handle.
- A view of a `u8[]` as an array of a different element type with no
  copy.

### 360.5 Phase Review

Form.

- The checker accepts a sized numeric, a `boolean`, and an enum at
  the top level of `T` (`check_context_bytes_call`). The storage walk
  decides each byte, as before. A type parameter takes the result of
  its instance (§143 rule 2a).
- The development tier stores a scalar to a stack slot with the size
  and the alignment of its layout, and copies from the slot
  (`context_bytes_source`). `fromBytes` copies into a slot, and a
  scalar loads its representation from it.
- The ship tier gives a scalar operand storage with a compound
  literal of the scalar type, `&(ctype){value}`. An aggregate operand
  stays `&value`.
- The interpreter makes each `boolean` byte 0 or 1 in `fromBytes`,
  from the boolean offsets of the layout.

Measured.

| Item | Result |
|---|---|
| The accept entry at the contract pin | 54 diagnostics, each the S100 of problem 1 |
| The ship tier before the compound literal | The C compiler rejects `&-2`, `&300`, and `&25u`: no address of an rvalue |
| `fromBytes<FixedArray<boolean, 1>>` of the byte 2, then `bytesOf`, at the pin | Byte 1 in the development tier and the ship tier; byte 2 in the interpreter |
| The same form after this section | Byte 1 in the three tiers |
| `a513` in the three tiers | Equal output; each line of bytes is equal to the `FixedArray<S, 1>` form |
| The bytes of `a513`, by hand | Each row is the two's-complement or the IEEE 754 encoding of its value |
| `r566` under `tsc` 5.9.2 | Accepted |
| Omitted instances of the `tsc` matrix | 9,705 to 9,666: 39 cells of the three members are now compared, and each agrees |
| A scalar live across a `yield`, and across `Context.suspend` | Correct output in the development tier |
| The emitted C under clang with `-std=c11 -pedantic` | Accepted |
| The new compiler tests | 0.09 s and 0.08 s for the two test files |

The implementation round found two errors of the first contract
text. A nullable scalar is not a type, so it gives S011 and not S100
(rule 12). The first text had a rule for a sandbox profile, which
§113 removed.

Phase Review (a subagent with no context): no CRITICAL, no MAJOR, 4
MINOR. Each MINOR is in §360.6.

`tools/gate.sh full` on the tree before this record:

```text
gate full c66379dae93ada024d24586194882ddf0b0846ad dirty:22 debug 4263/0/10 release 4244/0/10 skips 2/0 clippy 4/10/13 goldens-moved 0 exit 0
```

`tools/hygiene.sh`: exit 0.

### 360.6 Open

1. The ship tier writes the result of `fromBytes<boolean>` with a
   copy of one byte into an `int32_t` local. The result is correct
   on a little-endian target, because the local starts at zero.
2. The corpus list of the memory model in the language reference
   does not cite `a513`, `t123`, and `r566`.
3. A comment in `compiler/tests/generic_tsc_matrix/api.rs` states
   "13 scalar kinds". The 13 kinds of the matrix are the ten sized
   numerics other than `i32`, `numeric`, `boolean`, and an enum.
4. No run under MSVC has the compound literal of the ship tier. MSVC
   accepts a compound literal in C *(docs)*. The Windows full gate is
   the measurement.
5. The items of §360.4.
