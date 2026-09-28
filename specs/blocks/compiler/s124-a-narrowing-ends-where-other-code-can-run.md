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
2. A narrowing of a shared location ends at every point where other
   code can run before the use: an expression that carries a `Call`
   site (§20.2; this covers script calls, indirect calls, and built-in
   calls that run a script callback), an `await`, and a `yield`. The
   narrowing does not survive past that point in evaluation order.
3. A store to a field `f` ends the narrowing of every shared path that
   has `f` as a field segment, whatever the receiver: the stored object
   can be reached through another name. A store to a module global ends
   the narrowing of that global and its extensions. The existing rule
   for a store to a local path stays.
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
3. An accept entry pins the two spellings of rule 5 (narrow again; copy
   to a `const` local first), on all three engines.
4. A test pins rule 4: a narrowed local survives a call.
5. No existing `.expected` golden moves.
