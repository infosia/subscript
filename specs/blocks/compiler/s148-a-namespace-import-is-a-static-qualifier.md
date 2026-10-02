<!-- §148 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 148. A namespace import is a static qualifier

*(Added 2026-10-02.)* Origin: a downstream proposal (`HANDOFF.md`,
2026-10-02, item 3). Owner decision of the same day: take it third.

Problem: `collisions.md` C18 admits named imports only. `import * as m
from "./math"` gives S100 "only named imports are in the decided
surface". Measured at `bd065fcd`: a `main.ts` that imports `./math` as
`math` and uses `math.add(1, 2)`, `math.bump()`, `math.counter`,
`const b: math.Box = new math.Box()`, `math.Color.Green`, and
`math.id<i32>(5)` gives that S100 at 1:8; `tsc` 5.9.2 accepts it.

### 148.1 Rules

1. `import * as ns from "<module>"` binds `ns` as a static qualifier of
   that module. `ns.x` resolves, at check time, to the declaration that
   the module exports as `x` (a declaration or a named re-export,
   §128), with the identity a named import of `x` gives (§126). It
   covers every export kind: a function (called), a module global (read
   live, as a named import reads it), a class (constructed, and named as
   a type `ns.C`, generic `ns.G<i32>` included), an enum (`ns.E.M`), and
   a string-literal alias (a type `ns.A`).
1a. The checker resolves every `ns.x` once, scope-aware, before it
   checks a body: each site then sees the declaration that `ns.x`
   names, as if a named import bound it. No site reads an unresolved
   `ns.x`. *(Added 2026-10-02 after the Phase Review: resolution ran at
   a few expression sites, so `await ns.f()` and `Worker.spawn(ns.echo)`
   were rejected as a value use of `ns`.)*
2. A namespace import creates no runtime value. `ns` alone is rejected
   with S100 wherever it is a value: passed, stored, returned, compared,
   interpolated, indexed (`ns[k]`), or used with `typeof`. A write
   through it (`ns.x = 1`, `ns.x++`) is rejected as a write to an import
   binding (§127). A call `ns.f()` supplies no receiver.
3. `ns.x` where the module exports no `x` gives S016 at `x`, naming the
   module.
4. A namespace import loads the module as a named import does; the
   initialization order of §137 does not change. Module cycles and
   re-export chains resolve as they do for named imports.
5. A local declaration named `ns` shadows the namespace in its scope, as
   it shadows a named import.
6. The entry-module rules of §129 do not change: a namespace import in
   the entry module exports nothing.
6a. A namespace import of a module that a discovery check lists as
   absent (§63) is poisoned as a named import of it is: every `ns.x`
   types as `Type::Error` with no diagnostic. The poisoned-import record
   carries the namespace as a field of its own, not as a sentinel name.
   §63 rule 3 is amended to match. *(Added 2026-10-02 after the Phase
   Review.)*
6b. A re-export of a namespace binding (`export { ns }`) is the
   `export * as ns` form of rule 7, and is rejected with S100 and C18,
   not with S016.
7. C18 is amended: `import * as ns` is in the surface; `export * as ns
   from`, `export *`, `import type * as ns`, and a namespace used as a
   value stay outside it.

### 148.2 Acceptance

1. Red first: a multi-module accept entry (laid out as
   `corpus/accept/a19-modules/`) that uses every rule 1 form through a
   namespace, beside the same forms through named imports, printing the
   same results; a re-export chain reached through a namespace; a live
   global read after a mutation. It is rejected at the contract pin;
   both tiers and the interpreter agree with the golden; `node` agrees
   where the entry is js-comparable.
2. Reject entries: `ns` passed as an argument, `ns` stored, `ns[k]`, a
   write `ns.x = 1`, a missing member `ns.nope`, and `export * as ns
   from`; each header records the measured `tsc` result.
3. A unit test shows that rule 5's shadowing resolves to the local.
4. No existing `.expected` golden moves.
