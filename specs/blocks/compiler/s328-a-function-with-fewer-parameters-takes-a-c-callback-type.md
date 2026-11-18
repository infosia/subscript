<!-- §328 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 328. A function with fewer parameters takes a C callback type

*(Added 2026-11-18.)* Origin: §274.4 item 1, rank 7 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06).

Problem, measured at `1ee6c0f9` with the interop mirror
(`type SubLogCallback = (message: string, userdata1: object | null,
userdata2: object | null) => void`):

| Value at a C callback place (`new SubCallbackInfo(v, null, null)`) | `tsc` | subscript |
|---|---|---|
| `(message: string) => { print(message); }` | accepts | the same S100 text |
| `function lf(message: string): void` | accepts | S100: "a value that is not a named function or a capture-free lambda cannot take the C callback type `SubLogCallback`" |

The text is false: `lf` is a named function, and the lambda captures
nothing. A capture-free lambda with all three parameters is accepted.
The check of a C callback type requires an equal parameter count
before it applies the parameter rules. At an ordinary function type
place, a named function with fewer parameters runs
(`const g: (a: i32, b: i32) => i32 = one` and `[1, 2, 3].map(one)` give
`10` and `2,4,6`). A named function cannot declare an `object`
parameter (S011), so at a C callback type with userdata parameters it
can only take fewer parameters.

### 328.1 Rules

1. **The prefix rule holds at a C callback place.** A named function
   or a capture-free lambda takes a C callback type if its parameters
   are a prefix of the callback type's parameters, and each of its
   parameter types and its return type relate to the callback type as
   at an ordinary function type place.
2. **The call passes the leading arguments.** When C calls the
   callback, the function receives the first `k` arguments, `k` its
   parameter count. Both tiers run it with the same output.
3. **A rejection names its cause.** A function value that does not
   take the callback type gets a text that names the parameter or the
   return type that does not match. No text calls a named function
   "not a named function".

### 328.2 Acceptance

1. Red first: an interop accept entry. A named function and a
   capture-free lambda, each with one parameter and with no parameter,
   take `SubLogCallback`; C calls
   each; each prints what it receives. At the pin, `check` rejects the
   entry.
2. A reject entry: a named function whose first parameter type does
   not match the callback type. The text names the parameter. Its
   header states what `tsc` does, measured.
3. The development tier and the ship tier print the same output. The
   CLI `check` output of each other corpus entry is the same at the pin
   and after.

### 328.3 Sections this one amends

- §274.4 item 1 closes for a named function and a lambda. A conditional of two
  values (`flag ? f : g`) at a C callback place stays as §274.4
  records it.
