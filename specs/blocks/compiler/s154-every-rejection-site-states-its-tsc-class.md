<!-- §154 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 154. Every rejection site states its tsc class

*(Added 2026-10-03.)* Origin: §153.3 item 1. The measurement is
`specs/tracking/s154-rejection-classes-a.md`, `-b.md`, and `-c.md`.

Problem: §79 rule 2 requires a variant at every site that rejects a
program `tsc` accepts. §153 made this total for S014 only. A site of
another code carries a variant only if a reject entry pins it (§79
rule 4), so nothing reports the other sites.

Measured at `ee67931d`, with one or more witnesses per production
diagnostic site of every code other than S014 in `compiler/src`:

| Group | Sites | Reached | Reached by a `tsc`-accepted program | Of those, block present | Needs variant | Unreachable |
|---|---:|---:|---:|---:|---:|---:|
| a: `check/expr.rs`, `check/expr/**` | 237 | 229 | 172 | 49 | 123 | 8 |
| b: `class_shape`, `declarations`, `stmt`, `signatures`, `exports`, `bodies`, `exception` | 177 | 163 | 146 | 43 | 103 | 14 |
| c: every other file | 116 | 110 | 106 | 36 | 70 | 6 |
| Total | 530 | 502 | 424 | 128 | 296 | 28 |

78 reached sites are reached only by `tsc`-rejected programs (an
unbound name, a type mismatch). 262 of the 296 sites are S100.
Examples: `if (n)` with `n: i32` ("condition must be boolean");
`apply(id, 3)` with a generic `id`; `yield* [1]`; a method overload
`f(v: i32): void; f(v: i32): void {}`; a class declared in a function
body. One `tsc` project per group costs 0.21–0.27 s; the checker over
each group's programs in one process costs 0.04–0.12 s.

### 154.1 Rules

1. **Every rejection site is named.** Every diagnostic that rejects a
   program goes through one checker function that takes a site name
   from one closed enum. The enum is the §153 `RejectionSite` enum,
   extended to every code. The parser's rejection sites are in it. A
   common builder that takes a code as a value is not a site; each of
   its callers passes a site. A site is the branch that decides the
   rejection, because the `tsc` class belongs to that branch. A form
   that carries a failure from that branch to the code that builds the
   diagnostic carries the site, not only text (for example a
   `Result<_, String>` from a regex, JSON helper, or alignment check).
   A rejection that a stage stores for later emission stores its site,
   or the whole `Diagnostic` that the site function built; a forwarder
   does not rebuild it from its code, message, and position. A branch
   that rejects forms with different reasons is one site per form: a
   catch-all over a syntax kind, an operator, a receiver type, or a
   callee origin splits by that fact.
2. **The site states its code and its class.** One exhaustive `match`
   maps each site to its `RuleCode` and to one of two classes:
   - `Diverges(Divergence)`: a `tsc`-accepted program reaches the
     site, or the site is S014 (§153 rule 1). The diagnostic carries
     the variant.
   - `TscRejects`: only `tsc`-rejected programs reach the site. The
     diagnostic carries no variant.
   `RuleCode::` appears in no checker or parser file other than the
   one that holds this `match`, except `diag.rs` and the renderer.
   If a fact that the checker already has separates the
   `tsc`-accepted programs of a site from its `tsc`-rejected programs,
   the site splits on that fact, so a `tsc`-rejected mistake gets no
   block. Example: a nullable member access on a local whose
   initializer is non-null (`tsc` narrows it) is one site; the same
   access on a call result or a parameter is another.
   A site name and a variant name state the guard. No name holds a
   source line number.
3. **Every site has a witness.** The §153 rule 5 table covers every
   site. A `Diverges` site that is not S014 has at least one
   `tsc`-accepted witness. A `TscRejects` site has at least one
   witness, and every witness is `tsc`-rejected. A site that no
   program reaches is "unreachable" with the guard that excludes it.
4. **The check is total.** The §153 rule 6 test covers every site: it
   asserts each witness's target message, the target's variant or the
   absence of a block, and, in one `tsc` run, each witness's label.
5. **A fragment is true.** §153 rule 7 covers every variant that a site
   carries.
6. **A message states no `tsc` outcome.** The diagnostic text of a
   `Diverges` site does not say that TypeScript rejects the form.
7. **Consequence for the corpus.** §153 rule 8 and acceptance 3 apply
   to every code: a reject entry whose header says `tsc: rejects` at a
   site that becomes `Diverges` is rewritten to a `tsc`-accepted
   program with the header `tsc: accepts`, or it retires, and its old
   program stays as a witness.
8. **Scope.** Diagnostics in `compiler/src`. Warnings are not
   rejections. Diagnostics that another crate builds are out of scope.
9. **A rejected declaration poisons its name.** A declaration that the
   checker rejects in a function body, or as a mirror `declare module`,
   binds its name as poisoned, as a rejected module-level declaration
   does. A later use of the name gives no second diagnostic.
10. **A source declaration shadows a builtin type name.** Every arm of
    the type-name resolution that names a builtin (`Promise`,
    `FixedArray`, `Generator`, and the others) first resolves a source
    declaration of that name in scope, as the `Worker` and `Map` arms
    do. `Array` stays a builtin (`stdlib.md` §9.0). A program that this
    checker accepts with another meaning than `tsc` gives it breaks C14.
11. **A type-parameter default is rejected at its declaration.** The
    checker does not read a default (C24 row 28), so the declaration
    `<T = i32>` is the site, not each use that omits the argument.
12. **C24 records the forms that no other section decides.** A
    `Diverges` site whose restriction no other section decides cites
    C24, and the variant's `why` states the part of its C24 row's rule
    that fits every program the site rejects.
13. **The first diagnostic carries the block.** When this checker
    rejects a `tsc`-accepted program, its first diagnostic in source
    order carries a divergence block. A later diagnostic that an earlier
    rejection causes is a follow-on. Rule 9 removes a follow-on where a
    poisoned name can; a follow-on with no block does not break rule 2.
    A witness of a `TscRejects` site is a program whose first diagnostic
    is at that site.

### 154.2 Acceptance

1. Red first: the rule 4 test, run against the pin's sites, reports
   every measured `needs variant` site at once.
2. A `Diverges` site uses an existing `Divergence` variant when its
   `why` states the reason the site rejects. Otherwise it uses a new
   variant. The new variant's `collision` names the `collisions.md`
   heading, or the `compiler.md` or `stdlib.md` section, that decides
   the restriction. One variant states one restriction: two sites
   share a variant only when one `ts` fragment shows both.
3. If neither C24 nor another section decides a site's restriction,
   the round does not invent a `collision`. The tracking note lists
   each such site with its witness, and the round stops after the list.
4. The rule 4 test states its measured cost in its doc comment. Its
   `tsc` run stays one process.
5. No accept golden moves. A reject entry's pinned message does not
   change, except where rule 6, rule 7, or acceptance 7 changes it.
6. §153.3 items 1 and 3 close.
7. A message that does not fit every form its site rejects (§103), or
   that names a remedy this language rejects, changes. Measured: the
   generic-alias site says "string-literal union aliases cannot be
   generic" for every generic alias; the generic-function-value site
   tells the program to write `id<i32>`, which is rejected.

### 154.3 Open

C24 rows that say "no lowering is decided" are decisions with no
reason. Each is a candidate to accept, because its lowering looks
mechanical. The evidence is in `specs/tracking/s154-undecided-sites.md`.
None is decided.

1. Truth tests on non-`boolean` values (row 1). Decided 2026-10-04: the
   form stays rejected; row 1 states the reason.
2. Inference of a literal-initialized module variable, field, or
   defaulted parameter, and of a block-lambda result (row 4). Closed by §156.
3. Definite assignment for a local with no initializer (row 3), as
   §108 does for fields.
4. A static method and a generator function as values (row 13).
5. `yield*` over a stdlib §14.1 source, and an instance generator
   method (rows 11, 16).
6. `#name` members (row 11); transparent and generic type aliases, and
   a repeated literal member (row 7); parameter properties and
   identifier-spelled string enum members (row 8).
7. `x!` as a checked narrowing; `&&=` and `||=` (row 18).
8. `this` captured in a lambda, and a `function` expression that does
   not read `this` (row 15). `this` in a lambda in a reference-class method is
   closed by §157.
9. Nullable-parameter contravariance of function types (row 25).
10. Narrowing by a non-null initializer or assignment (row 26).
11. Order-independent yield-type inference (row 27).
12. Type-parameter defaults (row 28).
13. A `string` index, a quoted descriptor key, a descriptor spread, an
    array in a template, a `switch` on `boolean`, an assigning
    `for…of` head (rows 20–24).

A `TscRejects` class is a measured claim: no witness set proves that no
`tsc`-accepted program reaches the site, and this checker approximates
the `tsc` acceptance of a program. Three Phase Reviews found 7, 6, and
15 new MAJOR instances of the class, so no finite review closes it. A
new instance is a missing split or a missing fact. It is fixed when it
is found, by a split on a fact the checker has (rule 2), and it does
not block a phase. The instances below were found by the third review
and were present before §154; each is realistic.

1. A nullable local assigned non-null in every `switch` case and read
   after the `switch`, or after `while (n === null) { n = new N(); }`:
   S011 with no block.
2. A `switch` with grouped case labels that all return
   (`case 1: case 2: return 1; default: return 0;`): "not all paths
   return a value" with no block, in functions, lambdas, and methods.
3. `xs.length = 0` on an array: no block, and the message lists
   `length` as outside the surface.
4. `e.stack` and `e.cause` on an `Error`: S018 with no block.
5. `new Map<string, i32>([["a", 1]])`: the element mismatch fires
   first with no block, before the `new Map(iterable)` site.
6. Structurally identical classes inside arrays and function types
   (`const ps: P[] = qs;`): type mismatch with no block.
7. An `abstract` property: the missing-initializer site with no block.
8. An optional parameter in a function type
   (`cb: (a: i32, b?: i32) => void` called with one argument): the
   arity site with no block. Closed by §155 rule 5.
9. `this` in an arrow inside a static method: no block.
10. Reverse cost at C6, C21, and C14 sites: `e.message` on an untyped
    catch binding (TS18046), `const x: i32 = f()` with `f(): void`
    (TS2322), and a read before its `const` in one block (TS2448)
    carry a block.

Outside this section: four acceptance gaps that broke invariant 5. Closed
by §155.
