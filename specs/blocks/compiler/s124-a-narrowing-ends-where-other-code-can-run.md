<!-- §124 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 124. A narrowing ends where other code can run

*(Added 2026-09-28.)* Origin: the review of §123 (§123.1 rule 9). The
owner chose to fix it on 2026-09-28.

Problem: a null check narrows a path (C7, §46). The narrowing ends only
at an assignment to that path or an extension of it
(`compiler/src/check/expr/assign.rs`, "an assignment invalidates
narrowing for the path and its extensions"). Code that runs between the
check and the use can write the same location through another name, and
the use then reads null through the narrowed type. Measured at `3d140e2`
(dev JIT through `subscript run`; ship C through `subscript build`):

- A call: `if (h.c !== null) { clear(h); print(h.c.v); }` where `clear`
  sets `h.c = null` — signal 11 on the dev JIT, status 139 on ship C.
  The same for a function field `h.cb(3)`.
- A store through an alias: `const g = h; if (h.cb !== null) { g.cb =
  null; h.cb(3); }` — signal 11.
- A module global: `if (g !== null) { clear(); print(g.v); }` where
  `clear` sets `g = null` — signal 11.
- A suspension: `if (h.c !== null) { await tick(); print(h.c.v); }`
  where `tick` sets `h.c = null` — signal 11.

A local variable is not in this class: a lambda captures only `const`
locals, by value (S009), so no other frame writes a local. An array
element is not narrowed (S011 at `a[0].v` after `a[0] !== null`).

`tsc` accepts every probe above; its narrowing is not invalidated by a
call. This compiler is sound where `tsc` is not: the fix rejects
programs `tsc` accepts, a narrowing on top of `tsc` (collisions.md
header).

### 124.1 Rules

1. A shared location is a module global or a path through at least one
   field (`h.c`, `this.cb`, `S.opt`). Another frame can write a shared
   location; no other frame can write a local.
2. A narrowing of a shared location ends at every point where script
   code can run before the use. "Can run script code" is one HIR fact,
   derived by a match over every expression and statement kind with no
   wildcard arm. It holds for:
   - a call of a script function, a method, a function value, or a
     foreign function (a foreign function can call back);
   - a built-in call that runs a script callback;
   - `new C(...)` when `C` has a constructor or a field initializer
     that can run script code;
   - a generator resumption (`for…of` over a generator, `next()`);
   - the scope exit of a `using` binding (it calls `[Symbol.dispose]()`);
   - `await` and `yield`.
   A built-in that runs no script code (`slice`, `push`, `new Map()`)
   does not end a narrowing. *(Corrected 2026-09-28: the first version
   read the fact from the `Call` trap site, which misses the last four
   forms and includes built-ins that run no script code.)*
3. A store to a field `f` ends the narrowing of every shared path that
   has `f` as a field segment, whatever the receiver: the stored object
   can be reached through another name. A store to a module global ends
   the narrowing of that global and its extensions. The existing rule
   for a store to a local path stays.
3a. Each statement subtree has a summary: whether it can run script
   code, and the field names and module globals it can store to. Every
   join point applies it: a loop head removes, before the first
   iteration, each fact that the body, the condition, or the step can
   end (a narrowing never survives a back edge that ends it); a `try`
   handler starts without the facts the `try` block can end; the point
   after a `try` statement keeps only the facts neither block can end.
   A join never rebuilds facts from a saved set and a syntactic
   prescan alone.
3b. A location is shared by its binding, not its name: a local that
   shadows a module global name is a local.
3c. A `switch` is a join: each case starts from the facts at the
   dispatch (the facts before the `switch`, after the discriminant; no
   case summary applies there, because no back edge exists), plus the
   facts that fall through from the case before it; the point after the `switch` keeps only the facts that hold on
   every `break` edge and at the end of the last case, after the case
   summaries apply. This holds for a local as well (a narrowing made in
   one case never reaches another case or the exit through a case that
   did not make it).
3d. *(Added 2026-09-28 after the second review, owner decision.)* The
   static kills of rules 2 to 3c are the diagnostic, not the soundness
   argument. Two reviews found hand-placed kill sites that missed
   script code the checker builds itself (an accessor in a compound
   assignment, an optional chain, a destructuring pattern). So every
   read through a shared location whose declared storage type is
   `T | null` and whose expression type is the narrowed `T` carries a
   `NullNarrowing` trap site (§20.2). The rule compares two types that
   the checker derives separately (the declared type of the field or
   global, and the type of the read), so it covers every read the
   static kills miss. Every engine tests the site: the dev JIT, ship C,
   and the interpreter trap `null-narrowing` at the read position
   instead of reading through null. A local read carries no site
   (rule 4).
   The site belongs to one LIR operation: the conversion of a
   `T | null` value to `T`. Every path that uses a narrowed shared
   location, as a value or as a place (a field store, a compound
   assignment, a boundary box), goes through that conversion first.
   The conversion is its own LIR instruction, `NarrowNonNull(origin)`,
   and it carries where the narrowed value comes from: a shared read,
   or a local (a `const` copy included). A type-changing `as` cast
   keeps its own instruction (`Cast`) with the `NullNarrowing` and
   `ClassMismatch` sites; the language has no same-type `as` from
   `T | null` to `T` (S100), so no third origin exists. The lowering
   sets the origin from the HIR expression kind, by a match with no
   wildcard arm; the origin has no default. The LIR verifier checks, on
   every build: a `SharedRead` conversion carries the
   `SharedNullNarrowing` site, and no other conversion instruction
   converts a `T | null` value to the same `T`. The verifier decides
   "is a conversion" from the instruction kind, by a match over every
   kind with no wildcard arm, not from operand and result types: a call
   that takes `T | null` and returns `T` is not a conversion. The HIR
   site derivation classifies shared and local reads by a match with no
   wildcard arm too, so a new expression kind states its class in both
   places. The origin and the site come from two
   separate derivations (the expression kind, and `Expr::trap_sites`),
   so the check compares two facts (core principle 9). *(Corrected
   2026-09-28, owner decision: two verifier rules that read only
   operand shapes rejected legitimate `const`-local copies, because SSA
   makes the local the load itself and passes tested values through
   block arguments.)* The trap has its
   own kind and message ("a narrowed shared location is null"); the
   `as` narrowing trap keeps its kind and message.
3e. A checker-synthesized helper (§119) runs no script code, so a call
   of one does not end a narrowing (`JSON.stringify`, `JSON.parse`).
4. A narrowing of a local does not change.
5. After a narrowing ends, the use gets the diagnostic of an
   unnarrowed use (S011 for a member read, the §122/§123 diagnostic for
   a call). The program narrows again after the call, or copies the
   value to a `const` local before the call and uses the local.
6. A new collision entry records the divergence: `tsc` keeps a
   narrowing across a call and a suspension; this compiler ends it for
   a shared location.

### 124.2 Acceptance

1. Measure first (CLAUDE.md, step 0): prototype rules 2 and 3 and count
   the corpus entries (accept, warn, trap, examples) that the checker
   now rejects. If any entry is rejected, stop and report the list with
   each diagnostic; the owner decides before the rule lands.
2. Red first (core principle 10): reject entries pin a call, an alias
   store, a module global across a call, and an `await`, each Red at
   `3d140e2` (accepted there). Each header states what `tsc` does,
   measured, and cites the new collision id.
2a. Red reject entries also pin, each accepted at `7bc395d` and
   crashing at run time: a kill in a loop body reaching the next
   iteration (a call, an alias store, a module global, an `await`), a
   kill in a `try` block read after the statement, a class with no
   constructor whose field initializer calls a function, a generator
   body, and a `using` scope exit.
3. An accept entry pins the two spellings of rule 5 (narrow again; copy
   to a `const` local first), on all three engines.
4. A test pins rule 4: a narrowed local survives a call.
5. A test pins that a built-in with no script code (`slice`, `push`,
   `new Map()`) keeps a narrowing, and that a shadowing local keeps its
   narrowing across a call.
6. A diagnostic carries the C17 note only when a §124 kill ended the
   narrowing it reports.
7. No existing `.expected` golden moves. The LIR text snapshot moves
   only where a `Coerce` becomes `NarrowNonNull` (4 lines at the
   measurement); the round reports the lines.
7a. An accept entry pins a call that takes `T | null` and returns `T`
   (a function, a method, and a function value), on all three
   engines. The corpus had no such call when the verifier first
   rejected it.
8. Rule 3d: trap entries pin the shapes the second review measured
   (an accessor in a compound assignment, a static accessor, an
   optional-chain getter, a destructuring getter): each is accepted and
   traps `null-narrowing` at the read on every engine, where
   `7bc395d` read through null. A `switch` reject entry pins rule 3c.
9. The dev-JIT and ship C cost of rule 3d is measured on the benchmark
   set (paired, the same run count) and recorded. The cost of any
   second checker pass is measured and stated, or the pass is removed.
10. The language surface does not grow: `do…while` stays outside it.
