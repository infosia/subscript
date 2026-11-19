<!-- §339 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 339. A literal const, a worker entry, and an optional leaf follow `tsc`

*(Added 2026-11-19.)* Origin: rank 14 of `specs/tracking/backlog.md`
(§240.4 item 6, §185.4 item 1, §258.4 item 2). A `tsc` divergence
with a fix in sight (standing approval of 2026-11-06). On 2026-11-19
the owner selected rank 14.

Problem, measured at `eaaecee4` with `tsc` 5.9.2, the libraries of
the project, and the debug CLI. `tsc` accepts each form with no error.
`Name` is `type Name = "a.txt" | "b.txt"`, `f(n: Name): void`,
`g(n: i32): void`, `b` is a `boolean`, and `c` is a `C | null` with
`m(): void`.

| Form | Checker |
|---|---|
| `const A = "a.txt"; const n: Name = A;` and `f(A)` | S100 "type mismatch: the initializer expects `Name`, got `string`" |
| `` f(`a.txt`) `` and `` const n: Name = `b.txt`; `` | the same |
| `const e = "utf8"; readFile(p, e)`, `` readFile(p, `utf8`) `` | S100 "accepted forms: …" with a divergence block |
| `readFile(n, "utf8")`, `writeFile(n, s)` with `n: Name` | S100 "accepted forms: …", why "an integer tag at run time" |
| `Worker.spawn<M, M>(work)` with `work(i, o): i32` | S100 "`Worker.spawn` entry must have the exact synchronous shape" |
| `Worker.spawn<M, M>(entry)` with `entry(i: Inbox<M>): void`, and with `none(): void` | the same |
| `b ? c?.m() : g(1);`, `b \|\| c?.m();`, `c !== null && c?.m();` | S012 "an optional chain has type `void \| undefined` …" |

Also measured at `eaaecee4`:

- `const s: string = n` and `` `${n}` `` with `n: Name` print the
  member string. So the reason of §185 rule 5a for a path ("no string
  for the host") does not hold today (core principle 14).
- `let e2: string = "utf8"; readFile(p, e2)` gets a divergence block,
  but `tsc` rejects it with TS2345. §185 rule 5a gives a block only to
  an argument that `tsc` accepts.
- A function value with fewer parameters, or with a value result, is
  accepted in a callback place: `const cb: (a: i32, b: i32) => void =
  one;` runs (§240).
- These stay rejected, and are out of scope: a lambda entry (r106,
  §40.1), an async entry (r107), and `let e = "utf8"` as an encoding
  (`tsc` widens it to `string` and gives TS2345).

### 339.1 Rules

1. **A literal const keeps its literal in a literal place.** `tsc`
   gives an unannotated `const` the literal type of its initializer.
   If the initializer is a string literal, a template with no
   substitution, or the name of another such `const`, the binding
   carries that literal string. In a place whose type is a
   string-literal union alias, or the encoding parameter of `readFile`,
   a reference to the binding is that literal. The binding keeps the
   type `string` in every other place, as `let x = A` gives `string`
   in `tsc`. This applies to a local `const` and to a module `const`,
   also through an import.
2. **A plain template is a literal in each literal place.** A
   template with no substitution is the string literal of its cooked
   value in each place of rule 1. §336 rule 2 gave this to the
   `normalize` form only.
3. **A literal that is not a member stays rejected.** A literal of
   rule 1 or 2 that is not a member of the alias keeps the type
   mismatch diagnostic. An encoding other than `"utf8"` keeps the
   "accepted forms" diagnostic.
4. **A literal-alias path converts to its string.** A `path` argument
   of a string-literal union alias type is accepted. The call passes
   the member string, as `const s: string = n` gives it. §185 rule 5a
   and `collisions.md` C25 item 4 change.
5. **A divergence block needs a `tsc` acceptance.** A file-module
   argument of the type `string` at the encoding parameter gets no
   divergence block, because `tsc` rejects it (§185 rule 5a).
6. **A worker entry follows the function place rule.** The `spawn`
   argument is still a directly named module-level synchronous
   non-capturing function (§40.1). Its type must be assignable to
   `(inbox: Inbox<In>, outbox: Outbox<Out>) => void` by the rule of a
   callback place: it can have fewer parameters, and its result can be
   a value. The spawned worker calls it with the parameters it
   declares, and drops the result. A counted result is released, as in
   an expression statement (§300 rule 3). A parameter of another type
   keeps the S100 of the entry shape.
7. **An optional chain is a statement at a statement leaf.** At a
   leaf that §258 and §300 check as a statement, an optional chain
   call is a statement. S012 does not apply there, as it does not
   apply to the expression statement `c?.m();`. A leaf that is not a
   statement place (an operand of `??`, an initializer, an argument)
   keeps S012.
8. **Nothing else changes.** No program that `tsc` rejects changes
   from rejected to accepted. The output of each accepted program does
   not change.

### 339.2 Acceptance

1. Red first. Each new accept entry is rejected by a CLI built at the
   pin, and the report shows that output:
   - a literal-context entry for rules 1–3 (local, module, and
     imported `const`, a chain of two `const`s, and a plain template,
     at an initializer, an argument, and a return);
   - a file-module entry for rules 1, 2, and 4 (`interpreter: no`,
     `js-comparable: no` with C25);
   - a worker entry for rule 6 (a value result, a counted result, one
     parameter, and no parameter);
   - an optional-leaf entry for rule 7 (each form of the table, and
     the place in a lambda body with a `void` context).
   An entry that `node` can run is `js-comparable: yes` and equals
   `node`.
2. Unit tests, each with a control in the same test:
   - rule 3: a non-member literal through a `const` keeps the type
     mismatch; `let e = "utf8"` keeps the "accepted forms" diagnostic;
   - rule 1: `let x = A; const n: Name = x;` stays rejected;
   - rule 5: `let e2: string` gives the diagnostic with no divergence
     block, and `const e = "utf16"` gives no block either;
   - rule 6: an entry whose first parameter is `i32` keeps the S100;
     r106 and r107 do not change;
   - rule 7: an optional chain call of a `void` method as an
     initializer, or as an argument, keeps S012.
3. The development tier, the ship tier, and (where the entry allows
   it) the interpreter give the same output. The CLI `check` output of
   each other corpus entry is the same at the pin and after.

### 339.3 Sections this one amends

- §185 rule 5a; §185.4 item 1 closes. `collisions.md` C25 item 4.
- §40.1 (the worker entry shape); §240.4 item 6 closes.
- §258.4 item 2: the optional-chain leaf closes.

### 339.4 Phase Review

Form. The checker reads the literal of an unannotated `const` through
its initializer, also through an import and a chain of `const`s, only
where the context is a string-literal union alias or the `readFile`
encoding (`compiler/src/check/expr/entry.rs`,
`compiler/src/check/file_module.rs`). The path converts with the
operation of `const s: string = n`. A worker entry that is not of the
exact shape gets an adapter in each tier
(`codegen/src/lower/func.rs`, `codegen/src/cemit/emitter.rs`): it
calls the declared endpoint prefix and releases a counted result if no
trap is pending. An entry with trailing default parameters gets a
synthesized function. An optional chain at a statement leaf lowers to
the guarded call of `c?.m();`.

Measured, the pin `eaaecee4` and after. Each new entry is rejected at
the pin by `check`, the development run, and the ship build. After,
the development tier, the ship tier, the interpreter where the header
allows it, and `node` for `a488` and `a491` give the golden:

| Entry | Pin | After |
|---|---|---|
| `a488-literal-const-context` (two modules) | 13 type mismatch errors | 13 lines |
| `a489-file-literal-path` | 4 "accepted forms" errors | `alpha`, `beta` |
| `a490-worker-function-place` | 4 entry shape errors | `11`, `22`, `one`, `none`, `44` |
| `a491-optional-statement-leaf` | 3 S012 errors | 6 lines |

The CLI `check` output of 1,102 other corpus entries is the same at
the pin and after. `codegen/tests/worker_entry_release.rs` fails in
the development tier and in the ship tier when the adapter release is
removed: the released generator reports an unobserved failed handle.

One fresh review found no CRITICAL or MAJOR finding. It found six
MINOR findings. Four are fixed: the release test above; the entry
diagnostic states the rule 6 shape, and a generator, an async result,
a default parameter, and a missing message class each have their own
reason; a comment of the encoding branch; and a run of the default
parameter path in `a490`. The finding on §185 and §40.1 text is fixed
by §339.3. One is open (§339.5 item 1).

### 339.5 Open

1. Contrived. An entry whose only parameter is an `Outbox<M>`, with
   `Worker.spawn<M, M>(entry)`, gives "type arguments must exactly
   match its entry's Inbox/Outbox message classes". The type arguments
   match, and the cause is the parameter kind. `tsc` also rejects the
   program, so only the message is wrong.
2. A lambda entry stays rejected (r106, §40.1). A non-capturing lambda
   has no environment, so the reason of §40.1 does not apply to it.
   `tsc` accepts it. It is not selected.
