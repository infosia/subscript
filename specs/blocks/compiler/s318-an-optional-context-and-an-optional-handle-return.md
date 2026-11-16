<!-- §318 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 318. An optional context and an optional handle return

*(Added 2026-11-16.)* Origin: §251.4 item 1 and §237.4 item 4. The
owner selected the small `tsc` gaps of the open-item triage on
2026-11-16.

Problem, measured at `d1f9bc02`. `tsc` accepts both programs; the
checker rejects both.

```ts
type CB = (x: i32) => i32;
function r(): CB | undefined { return (x) => x + 1; }
```

gives S100 "parameters require a type annotation" at `x`.

```ts
class S { async am(): Promise<void> { print("am"); } }
async function g(s: S | null): Promise<void> { return s?.am(); }
```

gives S100 "a `void` function cannot return a value". With `g(new S())`
and `g(null)` awaited in turn, then `print("end")`, `node` prints `am`
and `end`.

### 318.1 Rules

1. A contextual type `F | undefined` or `F | null`, where `F` is a
   function type, gives a lambda the context of `F`: its parameter
   types and its result type. The lambda value has the type `F`, and
   the place converts it as it converts a value of `F` (§251, §195).
2. In an async function with the result `Promise<void>`, `return e`
   where `e` has the type `Promise<void> | undefined` (an optional
   call, for example `s?.am()`) adopts the handle when `e` is a handle,
   by the rules of §236, and completes the function with no value when
   `e` is `undefined`. `return e` where `e` is `undefined` alone keeps
   §250.
3. The development tier, the ship tier, and the interpreter give the
   same output. The accept entries carry `js-comparable` and equal
   `node`.
4. No program that `tsc` rejects becomes accepted.

### 318.2 Acceptance

1. Red first: an accept entry for each program, with output; each is
   rejected at the pin.
2. For rule 1: `null` and `undefined` forms, a lambda result checked
   against the result of `F`, and an annotated parameter that does not
   fit `F` (rejected).
3. For rule 2: the handle path rejects with an exception that the
   caller catches; the handle is released once on each path (the LIR
   release facts of §236 hold).

### 318.3 Phase Review

*(Filled in after the review.)*

### 318.4 Open

None.
