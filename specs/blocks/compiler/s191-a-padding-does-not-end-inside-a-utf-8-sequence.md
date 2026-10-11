<!-- §191 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 191. A padding does not end inside a UTF-8 sequence

*(Added 2026-10-11.)* Origin: a string review on 2026-10-11 that the
owner supplied. On the same day the owner selected rejection (policy
A) over rounding the target down or up.

Problem: `padStart` and `padEnd` measure the target in bytes (Q21).
The last copy of the pad is cut at the target byte. If the pad holds a
multibyte UTF-8 sequence, the cut can fall inside that sequence. The
result is not valid UTF-8, and every string must be valid UTF-8.
Measured at `3112f7f7` with `subscript run`:
`"A".padStart(2, "あ")` gives the bytes `E3 41`. Then `charCodeAt(0)`
gives 227, and `split("")` gives 1 piece. `"A".padEnd(2, "あ")` gives
length 2. `node` 24 gives `"あA"` and `"Aあ"`, because its target counts
UTF-16 units (Q5).

### 191.1 Rules

1. **The cut must be on a boundary.** Before the result is allocated,
   the runtime finds the byte where the last copy of the pad ends. If
   that byte is inside a UTF-8 sequence of the pad, the call traps.
   The trap kind is the string range trap that `slice` uses (Q5). The
   message names the method, the target, and the byte of the pad where
   the cut falls.
2. **Other calls do not change.** A cut on a boundary, a whole number
   of pad copies, an empty pad, and a receiver that is already long
   enough give the same bytes as before.
3. **No malformed string exists.** The runtime does not allocate a
   string and then check it. The check runs on the pad bytes and the
   fill length only.
4. **Every tier.** The dev tier, the ship tier, and the interpreter
   give the same output and the same trap.
5. **Docs.** The TypeScript tutorial shows byte units with a Japanese
   string: `length`, `indexOf`, `slice`, `at`, and `charCodeAt` (which
   reads one byte). It shows `for...of` for code-point steps, and it
   states that `position + 1` is not the next character.

### 191.2 Acceptance

1. Red at `3112f7f7`: a trap entry with `"A".padStart(2, "あ")`, and one
   with `padEnd`. An accept entry with cuts on a boundary: a mixed pad
   `"あx"` with a partial cycle that ends after `あ`, a supplementary
   character pad (`"𠮷"`) with a whole copy, and ASCII cases. Each
   accept entry is `js-comparable` only where the byte target and the
   UTF-16 target give the same text; otherwise it cites Q5.
   The accept entry is the rule 2 control: it is green at the pin, and
   it fails if the check traps on a boundary cut.
2. A unit test of the boundary rule in `strops.rs`, with a same-shape
   control for each trap case.
3. Collision Q21 states the padding trap.
4. No cost change: the check reads at most 3 bytes of the pad for each
   call.
