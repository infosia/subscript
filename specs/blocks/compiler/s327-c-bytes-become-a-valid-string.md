<!-- §327 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 327. C bytes become a valid string

*(Added 2026-11-18.)* Origin: §184.3 item 2, rank 6 of
`specs/tracking/backlog.md`. The owner selected it on 2026-11-18 and
decided the form: bytes that are not valid UTF-8 are replaced with
U+FFFD.

Problem, measured at `bbf7e756` (a measurement round; development tier
and ship tier give the same output in all 100 cases). A script
`string` is UTF-8 (`specs/blocks/stdlib.md`, Q21). The host completion
checks its bytes (§184 rule 4). These entries copy C bytes into a
script string with no check:

- A. a string-view field that C writes through a pointer parameter or
  a written-back scratch target;
- B. a string-view parameter of a C callback with the Context
  lifetime;
- C. the same with an explicit lifetime (a registration);
- D. a string view in a pair or struct that the call receives (§325);
- E. the host function `subscript_rt_str_from_view`.

With `a<seq>b` from C, for each of `ff`, `80`, `e2 82`, `ed a0 80`,
`c0 80`:

| Operation | Result |
|---|---|
| `codePointAt(0)` | the process stops: "codePointAt(0) out of range", although index 0 is `a` |
| `slice(1, -1)`, `substring` | the process stops: "not on a UTF-8 boundary" |
| `for…of` | wrong value: the valid `a` is lost (`["�", "�", "b"]` for `ff`) |
| `toUpperCase` | wrong value: `a<ff>b`, ASCII is not changed |
| `split("")` | wrong value: one element |
| `print`, template, `JSON.stringify` | invalid UTF-8 in the output |

The cost of a UTF-8 check at entry E, release, ship allocator: 16 B
6.28 to 8.47 ns, 1 KiB 23.0 to 45.8 ns, 1 MiB 23.3 to 54.3 µs.

### 327.1 Rules

1. **Each C byte entry decodes with replacement.** At each entry A to
   E, and at each entry that a later section adds, the runtime decodes
   the C bytes as UTF-8. Each maximal invalid subpart becomes one
   U+FFFD (the Unicode substitution of maximal subparts, as
   `String::from_utf8_lossy` and the WHATWG decoder do). Valid bytes
   copy with no change.
2. **A script string is valid UTF-8.** After rule 1, no entry puts
   invalid UTF-8 into a script string. The host completion keeps its
   `INVALID_UTF8` status (§184 rule 4): the host can complete again.
   `Context::alloc_str` is the allocation of every string operation, so
   it does not decode. Its documentation states that its bytes must be
   valid UTF-8, and a Rust host that holds other bytes calls the rule 3
   decoder.
3. **One decoder.** One runtime function does the decode for every
   entry. Both tiers call it; neither tier decodes inline.
4. **Cost.** A valid string pays one validation pass over its bytes and
   no second copy. An invalid string pays the decode. This cost comes
   from the data that C passes (core principle 16).
5. **An unchanged view stays.** A string-view field that C does not
   change keeps the script string, with no decode (§187 rule 7).

### 327.2 Acceptance

1. Red first: an interop accept entry with a C fixture that gives each
   of the five sequences at entries A, B, and D. For each received
   string, the entry prints the result of `length`, `codePointAt` at
   the start of each code point (an index off a UTF-8 boundary traps by
   Q21), `slice(1, -1)`, `for…of`, `toUpperCase`, `split("")`,
   `JSON.stringify`, and `===` against the literal with U+FFFD at the
   same place. Each result is the result of the same operation on that
   literal. At the pin, the entry stops or prints other values.
2. A runtime unit test for `subscript_rt_str_from_view` and the rule 3
   decoder with each sequence, valid multi-byte text, an
   empty view, and a null pointer with length 0.
3. The 16 B, 1 KiB, and 1 MiB cost of entry E, valid ASCII and one
   invalid byte, at the pin and after, release. The report states the
   difference.
4. The development tier and the ship tier print the same output. The
   CLI `check` output of each corpus entry is the same at the pin and
   after.

### 327.3 Sections this one amends

- §184.3 item 2 closes.
- §187 rule 2 and §28: a string view that C writes is decoded by rule 1.

### 327.4 Phase Review

Form. One runtime function, `alloc_str_from_view` in
`runtime/src/ffi/strings.rs`, decodes for each entry A to E. A valid
view pays one `from_utf8` pass and one copy. An invalid view is
measured once by `utf8_chunks()` from the end of its valid prefix, then
written directly into the string allocation. A length that cannot be
represented, or an allocation failure, gives the Context
`AllocationFailure` trap. `Context::alloc_str` and
`alloc_str_with` state that their bytes must be valid UTF-8.

Measured with a477 at entries A, B, C, and D, development tier and ship
tier the same:

| C bytes in `a<seq>b` | Pin | After (string, `length`) |
|---|---|---|
| `ff`, `80`, `e2 82` | `codePointAt(0)` stops the process | `a�b`, 5 |
| `ed a0 80` | the same | `a���b`, 11 |
| `c0 80` | the same | `a��b`, 8 |

Each operation of acceptance 1 gives the result of the literal with
U+FFFD. The CLI `check` output of 1,077 corpus entries is the same at
the pin and after.

Cost of entry E, release, ns per call, pin then after (development
allocator, ship allocator):

| Input | 16 B | 1 KiB | 1 MiB |
|---|---|---|---|
| Valid ASCII | 28.4 to 29.6, 6.5 to 10.1 | 67.5 to 99.3, 24.0 to 45.9 | 23,641 to 55,902, 23,651 to 56,107 |
| One invalid byte | 24.6 to 55.7, 6.8 to 47.5 | 68.0 to 430.3, 23.9 to 375.9 | 23,572 to 342,375, 23,573 to 342,408 |

One fresh review found no CRITICAL or MAJOR finding. Its four MINOR
findings are fixed: the invalid path validated twice and copied up to
three times through a temporary `String`; that `String` could fail with
no trap; the unit test had no truncated end sequence and no two
adjacent invalid subparts; a callback ran after a failed string
allocation.

### 327.5 Open

1. A view with one invalid byte costs about 14 times the pin at 1 MiB:
   `utf8_chunks()` has no ASCII fast path. A loop over `from_utf8` with
   `valid_up_to()` and `error_len()` is the cheaper form. Only invalid
   input pays it.
