<!-- §313 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 313. A callback leaf and an asserted lambda get the diagnostic of their form

*(Added 2026-11-16.)* Origin: §246.4 item 1, §247.4 item 1, §249.4
items 1 and 2. Each is a rejected program whose diagnostic differs from
the diagnostic of the same form in another place. `tsc` accepts each
program below (measured 2026-11-16 at `d5e4246e`).

Problem, measured at `d5e4246e` with the debug CLI.

1. With a parameter `p: (x: i32) => void` and
   `both(x: i32, y: i32): void`, `[1].forEach(b ? p : both)` gives
   "conditional branches have no common type". `run(b ? p : both)`
   with `run(f: (x: i32, y: i32) => void)` gives the §246 rule 5 text:
   "function value `p` needs an adapter for this place, and only a
   const local or a module `const` takes one".
2. With `wf(x: f64): Promise<void>` and `work(x: i32): Promise<void>`,
   `[1].forEach(b ? wf : work)` gives one S013 on `work`. The user
   place `each(b ? wf : work)` gives a type mismatch on `wf` and the
   S013 on `work`.
3. `((x) => x + 1) as CB` gives "parameters require a type annotation"
   only. `one as CB` gives the `as` rejection of C24
   `IdentityAssertion`, with its example.
4. `[one, (x) => x + 1] as CB[]` gives "parameters require a type
   annotation" and the `as` rejection. `[one] as CB[]` gives the `as`
   rejection only.

### 313.1 Rules

1. A built-in callback place checks each leaf of a conditional as the
   user place of §246 rule 6 does. A leaf gets the diagnostic that the
   same leaf gets in a user place with the same parameter types: the
   §246 rule 5 text, a type mismatch, or S013. Each leaf that does not
   fit gets its own diagnostic.
2. The operand of `as` gives the asserted type to a lambda as its
   context, as a type annotation does, and as `tsc` does. This applies
   to a lambda operand and to a lambda element of an array literal
   operand. The `as` rejection of the operand does not change. A
   lambda that gets parameter types from this context gets no
   "parameters require a type annotation" diagnostic.
3. No program changes from accepted to rejected or back. Only the
   diagnostics of the four forms above change, and the diagnostics of
   forms that reach the same code.

### 313.2 Acceptance

1. Red first: a test for each of the four forms asserts the code, the
   message, and the position of each diagnostic of rules 1 and 2. Each
   fails at the pin.
2. The user place forms (`run(b ? p : both)`, `each(b ? wf : work)`,
   `one as CB`, `[one] as CB[]`) give the same diagnostics at the pin
   and after the change.
3. The CLI output of each file of `corpus/reject` and `corpus/accept`
   is the same at the pin and after the change, except the files of
   rule 3. Section 313.3 lists each file that changes.

### 313.3 Phase Review

*(Filled in after the review.)*

### 313.4 Open

None.
