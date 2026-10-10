<!-- §188 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 188. A handle inside a value has an origin

*(Added 2026-10-11.)* Origin: the §186 Phase Review (§186.4 item 6).
On 2026-10-10 the owner selected this section and chose option
"A+store" of the measurement round at `85c2fbcf`
(`specs/tracking/s188-handle-inside-a-value.md`).

Problem: S013 (§70.1 decision 2) rejects a handle that no `await`
observes, but its origins are a call result of `Promise<T>` or
`Promise<T>[]` and a function or method parameter of those types. An
`await` result, a lambda or `then` callback parameter, a constructor
parameter, and a call result of a deeper counted type have no origin.
So `(await many()).length`, with `many(): Promise<Promise<i32>[]>`,
drops two handles with no S013 (measurement part 1.1). The same rule
rejects six programs that await every handle: a field store or a
global store that is awaited later, a `FixedArray` return, a
destructured call result, a `yield`, and an array parameter that is
only read (part 1.3). `tsc` accepts every program of the measurement
except one that it types differently (part 4).

### 188.1 Rules

1. **A type that holds a handle.** `Promise<T>`, and a dynamic array,
   a `FixedArray`, or an `IterResult` whose element type holds a
   handle, at any depth.
2. **Origins.** A value of a type that holds a handle is an origin
   when it is: the result of a call of a script function or a foreign
   function; an `await` result; a
   parameter of a function, a method, a constructor, or a lambda
   (a `then`, `catch`, or `finally` callback included). A pattern
   binding carries the origins of its initializer.
3. **Discharge.** An origin is discharged by an `await` through the
   value; an argument whose parameter type holds a handle or is an
   array; a `return` of a type that holds a handle; the operand of
   `Promise.all`; the receiver of `then`, `catch`, or `finally`; and a
   store into a field or a module global. One `await` through a value
   discharges the whole value (the length of a dynamic array is a
   run-time value).
4. **S013.** An origin that is not discharged at the end of its body
   is S013, at the site of the origin, with the §182 example.
5. **Runtime cases (policy).** These forms are not S013, and the
   runtime reports a failed task that no `await` observes with trap 29
   at its release (§171 rule 10, §172 rule 4, §176 rule 5, §186
   measurement part 6): a value that is discharged at an argument or
   a store and then dropped by its holder (`Map.set`, `push`, a field,
   a helper parameter); an array with one element awaited and another
   dropped. A completed task that is dropped is silent, as in `node`.
6. **Tiers.** The rule is a checker rule; LIR, codegen, the runtime,
   and the interpreter do not change.

### 188.2 Acceptance

1. Red first, at the contract pin: reject entries for an `await`
   result used in place, a destructured `await` result with one
   element dropped, a `then` callback parameter of a counted type, a
   constructor parameter, and a call result of `Promise<i32>[][]`;
   accept entries for the pin false rejections that rule 3 closes (a
   field store awaited later, a global store awaited later, a
   `FixedArray` return, a destructured call result with both elements
   awaited). Each accept entry is `js-comparable: yes` where `node`
   prints the same bytes.
2. Unit tests: each origin kind of rule 2 and each discharge of rule 3,
   with a control in the same shape (core principle 9).
3. Cost: the checker cost of all accept entries and of the dense
   program of the measurement, against the pin.
4. No accept, warn, or trap entry changes; the reject corpus output
   does not change except for the new entries.

### 188.3 Open

1. A `yield` of a handle and an array parameter that is only read stay
   S013 (measurement h03, k10), although each program awaits every
   handle.
2. Rule 5's runtime cases.
3. The result of a built-in method (`pop`, `shift`, a generator's
   `next`, `Map.get`) is not an origin, so a handle that it returns
   and that the program drops is a runtime case (rule 5). The measured
   prototype has the same scope; it keeps `a339` accepted.
4. A call result that holds a handle in a field initializer or a
   module-global initializer is not reported: rule 4 reports at the
   end of a body, and an initializer has none (`class C { n: i32 =
   makeAll().length; }`).
5. A method receiver is not a discharge:
   `const ps = (await many()).slice(0); await Promise.all(ps);` gives
   S013 at the `await many()`.
6. A constructor parameter that is only read is S013, as item 1 states
   for a function parameter (`r417`).

### 188.4 Sections this one amends

- §70.1 decision 2: the origins and discharges of rules 2 and 3.
- §186.4 item 6 closes.
- Collision C8: a handle inside a value is S013 by rule 2.
