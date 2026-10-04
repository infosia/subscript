<!-- §157 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 157. A lambda captures `this` in a reference-class method

*(Added 2026-10-05.)* Origin: `compiler.md` §154.3 item 8 (`collisions.md`
C24 row 15). The owner selected it on 2026-10-04.

Problem: `this` in a lambda is rejected. The remedy that C24 row 15 gives
is `const self = this;`, which the checker accepts and lowers. The
rejection adds a line to the most common callback form in TypeScript and
protects nothing that the remedy does not. Measured at `f60fa6f9`:

| Program, in a method of a reference class | `subscript check` | `tsc` |
|---|---|---|
| `xs.forEach((v: i32): void => { this.total += v; });` | S100 "a lambda cannot capture `this`; capture a const local instead" | accepts |
| `const self = this; xs.forEach((v: i32): void => { self.total += v; });` | accepted; `run` prints the `node` value | accepts |

A `@ValueType` method is different. `const self = this; self.n = 5;`
writes to a copy (C2): `run` prints `0`, and `node` prints `5`. A
capture of `this` there has the same copy.

### 157.1 Rules

1. In an instance method, an accessor, or a constructor of a reference
   class, a lambda can read `this`. The lambda captures the receiver
   reference by value, as it captures a `const` local (C5). A write
   through it (`this.total += v`) writes to the object, because the
   capture is a reference.
2. The lambda is a capturing lambda for every C5 rule: it does not
   escape its defining function. A lambda that reads `this` and is
   returned, stored in a field or an array, or passed where a C callback
   is expected is rejected by the C5 site, as a lambda that captures a
   `const` local is. A nested lambda captures through the lambda that
   holds it, as a `const` local is captured.
3. In a constructor, a lambda that reads `this` follows §108: a call of
   the lambda, or a call that receives it, before a field holds a value
   is the §108 rejection, as `this.m()` is. A lambda that is created but
   not called before the field holds a value is accepted.
4. `this` in a lambda stays rejected in these places, each with its own
   site and reason:
   - a method of a `@ValueType` class: the capture is a copy (C2), so a
     write through it does not reach the receiver;
   - a field initializer or a static field initializer (§147 rule 2);
   - a default of a parameter: a lambda there cannot capture an earlier
     parameter (C5, S009, measured at `d6ba67f2`), and the receiver is
     captured as a parameter is;
   - a static method (C24 row 15);
   - a `function` expression, which binds its own `this` (C24 row 15).
5. C24 row 15 states the forms of rule 4 only.

### 157.2 Acceptance

1. Red first: one accept entry, `js-comparable: yes`, with `this` read
   and written in a `forEach`, `map`, `filter`, and `sort` callback, a
   nested lambda, a lambda in an async method across an `await`, a
   lambda in an accessor, and a lambda in a constructor after every
   field holds a value. An instance generator method is outside the
   subset (C24 row 11), so the entry has none. Its golden is the `node`
   output. At the contract pin, the checker rejects it; record the
   diagnostics.
2. Reject entries, each with its measured `tsc` header: a `this`
   lambda that escapes (returned, and stored in a field: C5, `tsc:
   accepts`); a `this` lambda in a `@ValueType` method (`tsc: accepts`);
   a `this` lambda called in a constructor before a field holds a value
   (§108, `tsc: accepts`).
3. Unit tests for each rule, each with a control in the same shape.
4. The §154 total test passes; the sites that rule 1 removes leave the
   site table, and the rule 4 sites keep a witness each.
5. No existing `.expected` golden moves, except the LIR snapshot record
   that a new async or generator entry adds.

### 157.3 Open

1. `this` in a lambda in a static method gives "`this` is only
   available in constructors and methods", the wording of a bare `this`
   in a static method at `f60fa6f9`. The context is a static method, so
   the message names the wrong reason; the `why` is true. Realistic,
   message only.
