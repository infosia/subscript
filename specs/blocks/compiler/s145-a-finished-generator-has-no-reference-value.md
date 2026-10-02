<!-- §145 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 145. A finished generator has no reference value

*(Added 2026-10-02.)* Origin: the §144 verification review. Owner
decision of 2026-10-02: trap at the read.

Problem: `.next()` returns `{ done: boolean; value: T }`, with `value`
zero-initialized when `done` (`collisions.md` C8). For a `T` whose zero
is a null reference and that is not nullable, that zero is not a value
of `T`: the program holds a `Box` that is null. Measured at `de41409f`:

| Program | dev | ship | `node` |
|---|---|---|---|
| a finished `Generator<Box>`, then `const b: Box = r.value; print(\`${b.v}\`)` | `program terminated abnormally (dev-JIT child signal 11)` | exit 1, no message | `TypeError: Cannot read properties of undefined` |
| the same `b` stored in a `Box[]`, then `print(\`${xs.length}\`)` | prints `1` | — | prints `1` |

`tsc` and `subscript check` accept both programs. The second row shows
that the null reference escapes into the program with no error; the
crash comes later, at a use, or never.

### 145.1 Rules

1. A read of the `value` of an `IterResult<T>` whose `done` is `true`
   traps with the new trap kind `generator-done-value` when the zero of
   `T` is not a value of `T`. One total function over the `Type`
   variants decides it, with no default arm, recursively through every
   component that the zero holds inline (a `FixedArray` element, a
   `@ValueType` field). The zero is not a value when it holds a null
   reference of a type that is not nullable, a null `string` handle, or
   the wire value 0 of a wire-mapped alias whose members exclude 0
   (§52.1). The function reads the alias table to decide the last case.
   A read is a member read `r.value`.
1a. A binding pattern over the `IterResult<T>` of `.next()` is rejected,
   as §107.3 rejects every non-array, non-class source; the diagnostic
   names the form `const r = it.next(); if (r.done) ...`. *(Revised
   2026-10-02, owner decision, after the verification review: a first
   revision accepted that source, so `const { done, value } = it.next();
   if (done) break;` trapped at the last step for every reference or
   `string` `T`, where the pin rejected it at check time.)*
2. For every other `T` the read is unchanged: a nullable `T` reads
   `null` (C22), and a scalar, `Date`, enum, plain string-literal alias
   (index 0 is a member), wire-mapped alias with a member of wire value
   0, or `@ValueType` `T` whose fields are all such types reads its zero
   (C8). *(Corrected 2026-10-02
   after the Phase Review: the first text listed `string` here; its zero
   is a null handle, and the owner decided that it traps. It listed
   neither `FixedArray` of a reference nor a value class that holds
   one.)*
3. The trap reports the source position of the read, identically on the
   dev tier, the ship tier, and the reference interpreter (§19 trap
   parity).
4. A read while `done` is `false` is unchanged, and so is every
   `for...of` over a generator: its lowering never reads `value` after
   `done`.
5. `node` reads `undefined` at the same point and throws only at a later
   use, or never. A new collision record states this divergence; it
   lands with the implementation.

### 145.2 Acceptance

1. Red first: a trap corpus entry for each row of the Problem table (the
   member-read program and the escaping-store program), each with the
   expected trap kind and the read's position; each one does not trap
   with that kind at the contract pin.
2. Controls that do not trap: a finished `Generator<Box | null>` (`null`),
   a finished `Generator<i32>` (`0`), a `while (!r.done)` loop over
   `Generator<Box>`, and a `for...of` over `Generator<Box>`.
3. The trap kind has a direct unit test in the runtime and a stable
   code in the generated runtime header; the header regenerates
   byte-identically through its generator.
4. No `.expected` golden of an existing entry moves. A new generator
   entry grows the LIR text snapshot; the round regenerates it and
   verifies that only the new sections are added.
5. `runtime/src/ffi.rs` (7,888 lines) and any other file past 2,000
   lines that the change adds lines to are split first, in a commit of
   their own with no behaviour change (§5.y rule 2a).
