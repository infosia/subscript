<!-- §331 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 331. A capture of an array element names the array

*(Added 2026-11-18.)* Origin: §175.4 item 2, rank 10 of
`specs/tracking/backlog.md`. On 2026-11-18 the owner selected the form:
the rejection stays, and its text names the true cause.

Problem, measured at `fb77186a`. §175 rule 3 rejects a lambda that
captures a counted binding when a local outside the binding's block
receives the lambda. In these two forms the binding holds an element of
an array that the function still holds:

```ts
const hs: Promise<i32>[] = [work(1), work(2)];
let f: () => Promise<i32> = () => work(0);
for (const h of hs) { f = () => h; }      // form A: a for-of binding
{ const h = hs[1]; f = () => h; }        // form B: a const copy of an element
```

Each gives S009 "captured binding `h` is released when its block exits;
local `f` is declared outside that block". The text is false for these
forms: the array holds the element, so the block exit does not free it.
The true cause is that the array can drop or replace the element (`pop`,
`hs[1] = …`) before a later call of the lambda, and the lambda borrows
the value (§116 rule 4c). `tsc` accepts both forms.

### 331.1 Rules

1. **The forms stay rejected.** §175 rule 3 applies to forms A and B
   with no change. The site stays `CaptureOutlivesBlock` (S009,
   collision C5).
2. **The text names the array.** If the captured binding is a for-of
   binding over an array, or a `const` whose initializer is an index
   read of an array, the text names the binding, the array, and the
   local: the lambda borrows an element of the array, and the array can
   drop that element before the lambda runs. It does not say that the
   binding is released when its block exits. The text names the array
   by the declared name of the binding or the field that holds it (an
   import alias gives the declared name; a pattern binding such as
   `const [h] = hs` gives the name of the array that its initializer
   reads). If the outer array of a nested index read is held so, the
   text says "an array". *(Amended 2026-11-18: round 1 stopped because
   the HIR does not keep the source spelling of the array expression.)*
2a. **Only an array that outlives the block.** Rule 2 applies only if a
   field, or a binding declared outside the block of the captured
   binding, holds the array. An array that is declared in that block,
   or that is a temporary (`for (const h of [work(1)])`, `hs.slice(0)`),
   is released when the block exits, so the §175 rule 4 text is true
   and stays. *(Added 2026-11-18 after the Phase Review: the rule 3
   example is also rejected for an array declared in the block.)*
3. **The alternative captures the array.** The "= subscript:" block of
   such a rejection (§311) is a fixed example program in which a lambda
   captures the array and reads the element in its body
   (`f = () => hs[1];`). The example passes `check` and `tsc`, as §311
   requires.
4. **Other forms keep their text.** A binding with another initializer
   (a call, an `await` result) keeps the §175 rule 4 text. The `r396`
   entries keep their output.

### 331.2 Acceptance

1. Red first: a reject entry for form A and one for form B. Each
   `expected-error` line has the new text. Each header states what
   `tsc` does, measured. At the pin, the text is the §175 text.
2. A unit test for rules 2 and 2a with forms A and B, a pattern
   binding, an array declared in the block, a temporary array, and the
   control of rule 4 (`const h = work(1)`) in the same shape, which
   keeps the §175 text.
3. The "= subscript:" example of rule 3 passes the existing check of
   §311 examples.
4. The CLI `check` output of each other corpus entry is the same at the
   pin and after.

### 331.3 Sections this one amends

- §175 rule 4: the text of a capture of an array element names the
  array. §175.4 item 2 closes.
