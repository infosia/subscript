<!-- §256 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 256. A default call takes its own count of each argument

*(Added 2026-11-12.)* Origin: §255.4 item 4. Selected by the owner on
2026-11-12.

Problem, measured at `1c2a6a78` with the debug CLI. `v(): Promise<i32>`
is async, `gen(): Generator<i32>` is a generator, and
`w(p: Promise<i32>, k: i32 = 2)` awaits `p`. `tsc` accepts each form,
with no error. `subscript run` and `subscript emit` stop each form of
the first table with "internal lowering error: LIR construction failed:
… stores counted operand 0 (value N) without a fresh single-use owner or
a preceding retain".

| Form | Called as |
|---|---|
| `w(p: Promise<i32>, k: i32 = 2)` | `await w(v())` |
| the method `C.w(p: Promise<i32>, k: i32 = 2)` | `await c.w(v())` |
| `w` | `ps.push(w(v()))` |
| `all(xs: Promise<i32>[] = [], k: i32 = 0)` | `await all()` |
| `all` | `await all([])` |
| `all` | `await all([v()])` |
| `take(g: Generator<i32>, k: i32 = 1)` | `take(gen())` |
| `take(g: Generator<i32>, k: i32 = 1, m: i32 = 2)` | `take(gen(), 5)` |
| `cnt(xs: Promise<i32>[], n: i32 = xs.length)` | `await cnt([v(), v()])` |

These forms run, as controls:

| Form | Called as | Output |
|---|---|---|
| `w` | `await w(v(), 3)` | `4` |
| `w` | `const h = v(); await w(h)`, and the same with `let h` | `3` |
| `w2(p: Promise<i32>, k: i32 = two())`, `two()` a call | `await w2(v())` | `3` |
| `take2(g: Generator<i32> = gen())` | `take2()` | `1` |
| `mk(b: Box, k: i32 = 3)`, `Box` a class | `mk(new Box())` | `7` |

The cause is in `codegen/src/lir/defaults.rs`. The default of an omitted
parameter is a function of its own, and the call passes it each earlier
argument (§57.1 step 4). The call then passes the same values to the
callee. A fresh counted value (an async handle, a generator, an array
that holds a counted type) has one owner. The default call takes it, so
the callee call stores it with no count of its own. If a later default
runs user code, the operand hold of §255 rule 1 site 2 retains the value
first, so the `two()` control runs. A read of a local takes a count of
its own, so the `h` controls run.

### 256.1 Rules

1. If a call passes a counted value both to a default function and to
   the callee, each pass has a count of its own. If the value has one
   owner only, the lowering retains it twice before each default call
   and then releases one count. The default call and the callee each
   move one retained count. The LIR verifier accepts a fresh owner at a
   store only if the value has one use, so the owner is not passed.
2. The counts balance on each path. The memory accounting of the
   development tier finds no handle that stays live and no handle
   released twice.
3. The release model of the LIR test (§255 rule 4) accepts the LIR with
   no new rule. If it does not, report and stop.
4. No accepted program changes its output. The development tier, the
   ship tier, and the interpreter give the same output.

### 256.2 Acceptance

1. Red first, at the pin: the accept entry
   `a440-a-default-call-takes-its-own-count-of-each-argument` stops the
   lowering. It has each form of the first table, prints each result,
   and is `js-comparable`. The tracking note records the output.
2. After the change, `a440` passes the three tiers, the release model,
   and `node`.
3. A unit test of the lowering lowers `w(v())`, `take(gen())`, and
   `cnt([v(), v()])`, and the LIR verifier accepts each. A control:
   `const h = v(); w(h)` keeps its LIR.
4. `main` of `a440` is async, so the LIR text snapshot grows by the
   `a440` block. No other golden moves. If another corpus entry or
   golden changes, report it and stop.

### 256.4 Open

1. Rule 1 does one retain and one release more for each omitted
   default than one retain needs. A single retain fails the LIR verifier: the owner then
   has three uses, and the verifier accepts a fresh owner at a store only
   at one use. A verifier that counts the retains before a store accepts
   the single retain. The cost is not measured.
2. An async handle that a parameter default creates gets S013.
   `one(p: Promise<i32> = v(), k: i32 = 2)` with `return (await p) + k;`
   gives "an async handle is dropped without any await of its
   completion" at `v()`. `tsc` accepts it.
