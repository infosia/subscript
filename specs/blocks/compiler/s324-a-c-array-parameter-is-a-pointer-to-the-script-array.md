<!-- §324 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 324. A C array parameter is a pointer to the script array

*(Added 2026-11-17.)* Origin: §187.3 item 2, rank 1 of
`specs/tracking/backlog.md`. The owner selected it on 2026-11-17.

Problem, measured at `7693e340`. In C, a parameter declared as an array
(`void fillLays(Lay out[2])`) is a pointer to the first element. The
caller's array gets each write of the callee. `subscript bind` gives the
parameter the type `FixedArray<Lay, 2>`, and `check` accepts a call.
Neither tier can run the call:

| C declaration | Mirror type | Development tier | Ship tier |
|---|---|---|---|
| `fillLays(Lay out[2])` | `FixedArray<Lay, 2>` | internal lowering error (aggregate argument) | C compiler error: incompatible parameter type (`Lay *`) |
| `readLays(const Lay in[2])` | `FixedArray<Lay, 2>` | same | same (`const Lay *`) |
| `fillInts(int32_t out[4])` | `FixedArray<i32, 4>` | same | same (`int32_t *`) |
| `readInts(const int32_t in[4])` | `FixedArray<i32, 4>` | same | same |
| `fillHandles(Handle hs[2])` | `FixedArray<Handle, 2>` | same | same |
| `fillStrings(LayS out[2])` (`LayS` holds a string view) | rejected by `bind` | — | — |

The compiler lowers the argument as a value, and the LIR copies a
`FixedArray` local before the call (`codegen/src/lower/func/instruction.rs`,
`codegen/src/cemit/graph.rs`). A prototype that changed only the
crossing plan and the emitters passed the address of that copy, so each
C write went to the copy and the script read the old values.

### 324.1 Rules

1. **The parameter is a pointer.** A foreign-function parameter that C
   declares as `T p[N]` keeps the mirror type `FixedArray<T, N>`. The
   call passes C a pointer to the first element of the argument's
   storage. It does not pass a copy of the value. If the argument is a
   place (a local, a field, an element), that place is the storage. If
   the argument is not a place, the call evaluates it into a temporary.
2. **C writes reach the script.** For an element type whose C layout is
   the script layout (scalars, enums, handles, `@ValueType` structs that
   the call passes as bytes), C reads and writes the script storage
   directly (zero copy). For an element type that the call rebuilds as a
   scratch struct (§187 rule 3, for example a struct with a string view
   or a count-first pair), the call builds `N` scratch elements and,
   after the call, writes each element back by the §187 rule 7
   copy-back. A string view in an element has the read lowering of §187
   rule 2 under this copy-back.
3. **`const` is not written back.** For `const T p[N]`, C must not
   write. The call writes nothing back. The mirror records the `const`
   fact of the parameter in its provenance comment, as §187 rule 3
   requires for a pointer parameter.
4. **One crossing plan.** The crossing plan of §187 rule 3 has a kind
   for this parameter. The binder, the checker, and the code generation
   of both tiers read that kind. The read-position scan of §187 rule 1
   classifies the elements of a non-`const` array parameter as a
   non-`const` pointer target. `bind` stops rejecting an array of a
   struct with a string view in this position.
5. **LIR carries the place.** A foreign call takes the address of the
   argument's storage as its operand for this parameter (core principle
   8). The LIR verifier rejects a foreign call that passes a
   `FixedArray` value for a parameter of this kind.
6. If one call passes the same storage to two such parameters, or to
   one such parameter and one pointer parameter, the result is the
   result of the last write-back (as §187.3 item 7 records for fills).

### 324.2 Acceptance

1. Red first: an interop accept entry, with the native fixture functions
   in `corpus/interop` and the mirror from `subscript bind`. It covers:
   - a non-`const` `int32_t[4]` that C increments element by element;
   - a `const int32_t[4]` that C reads;
   - a non-`const` struct array that the call passes as bytes;
   - a non-`const` struct array that the call rebuilds (`Lay`, with a
     count-first pair), where C writes a scalar member;
   - a handle array that C overwrites;
   - a struct array with a string view that C replaces;
   - an argument that is a class field (`box.values`) and an argument
     that is not a place (a call result), for which C returns the sum of
     the elements that it reads.

   Each line prints the values that C wrote or returned. At the pin, the
   entry fails in both tiers.
2. A `const` array argument: C reads each element and the script values
   stay the same. The fixture stops the process if an element is wrong.
3. A test builds a foreign call that passes a `FixedArray` value for an
   array parameter. The LIR verifier rejects it.
4. A check that the mirror records `const` for an array parameter, and
   that a hand-written mirror with no record counts as non-`const`.
5. The CLI `check` output of each corpus entry is the same at the pin
   and after the change. LIR text goldens move only for entries with a
   foreign array parameter.
6. The development tier and the ship tier print the same output.
