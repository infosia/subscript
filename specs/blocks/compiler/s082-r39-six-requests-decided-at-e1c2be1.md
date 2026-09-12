<!-- §82 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 82. R39 — six requests decided at `e1c2be1`

Origin: downstream request R39, 2026-09-02, at pin `e1c2be1`. The
request holds nine items. R39.2 (operators on value classes) is
withdrawn by the request: stock `tsc` rejects `a + b` on two class
operands (TS2365), so invariant 5 excludes every spelling. R39.1 and
R39.9 are not decided here; §82.7 records the cost facts that the
owner's decision needs. The six items below are decided.

Measurements at the pin, on this host (`subscript check`, `subscript
run`, and `tsc` 5.9.2 under `prelude/lang.d.ts` with the corpus
options):

1. R39.3. `c.$ += 1` through an accessor: S100 "`x.$ += v` is not
   supported for a class accessor". `g[0] += 3` through a class index
   signature: S100 "`a[i] += v` is not supported for a class index
   signature". A compound write and `++` on a plain field, an array
   element, and a local check and run today: `f.f += 2; f.f++;
   a[0] += 3; n++` prints `4 4 3 3`. `tsc` accepts every form,
   prefix `--` and a static accessor included.
2. R39.4. S015 is retired (§33.4). The next free codes are S016,
   S017, and S018. The reject entries whose first diagnostic carries
   one of the three message families: r149, r150, r151 ("duplicate
   declaration of `value` in one scope" / "in one switch body") and
   r169 ("unknown type name `SubChainExtA`"). No entry pins "unknown
   name" or "has no method".
3. R39.5. `maybe(false) ?? fallback`: S100 "operator outside the
   decided surface" (`check_binary`, `B::NullishCoalescing`).
   `maybe(true)?.next`: S100 "expression form outside the decided
   surface" (`ast::Expr::OptChain` reaches the catch-all arm). `tsc`
   accepts `a ?? b`. **`tsc` rejects `const a: Box | null =
   maybe(true)?.next` with TS2322**: an optional chain has the type
   `T | undefined`, and `undefined` is not assignable to `Box | null`.
   The same TS2322 fires for a numeric member and for a method call.
   The request's claim that `tsc` accepts both operators holds for
   `??`, and for `?.` only where `undefined` never binds a name: as
   the left operand of `??`, and as an expression statement.
4. R39.6. `identity<T>(value: T): T` on a reference class: S100
   "unknown type name `T`" twice (`tyres.rs`). `static create<T>` on
   a class: the same, and S100 "static method `Buf.create` is not
   generic" at the call. `tsc` accepts both programs.
5. R39.8. `static get total(): i32` with no `set`: S100 "static read
   accessor `total` requires a write accessor with the same name",
   with the `NamedAccessor` divergence block. `tsc` accepts. The
   instance form is legal (§65 rule 6).
6. R39.9. The request's probe passes `tsc`. This compiler stops at
   collection: S100 "duplicate function name `abs` in the program",
   S100 "duplicate top-level name `abs`", S100 "a method cannot share
   the member name `mul` with a method", then S011 at the union.
7. R39.7. The `.cargo/config.toml` patch recipe is the downstream's,
   verified there with `cargo metadata --offline`. No measurement on
   this side.

### 82.1 R39.3 — compound assignment through an accessor or an index signature

1. `x.name op= v`, where `x.name` reads an accessor (instance, or
   static as `C.name`), checks to the HIR of `x.name = x.name op v`.
   `a[i] op= v`, where `a` has a class index signature, checks to
   the HIR of `a[i] = a[i] op v`, which §58 rewrites to
   `a.set(i, a.get(i) op v)`. `op` is every operator that
   `assign_op` accepts today.
2. `x.name++`, `x.name--`, `++x.name`, `--x.name`, and the four
   index forms, in statement position, check to the HIR of the rule
   1 form with `+` or `-` and the literal `1`. Statement position is
   an expression statement, or the update clause of a `for`
   statement. *(Defined 2026-09-02 after the review: the update
   clause reported the value-position rejection, while a plain
   field in the same clause ran.)*
3. The receiver `x` and the index `i` evaluate once. When the
   receiver or the index is a place, the HIR is exactly the HIR of
   the rule 1 spelling; a test asserts the identity. When it is not
   a place, the checker binds it to a synthetic local (a `[[` name;
   warnings.md §2 already excludes such a name from W004) and the
   rewritten statement reads the local. The local is declared in
   the statement list that holds the statement. An initializer that
   has no statement list (a module-level binding initializer, a
   field initializer, a static field initializer, a parameter
   default, a descriptor default) cannot hold the local: there a
   non-place receiver of `??` or `?.` fails with S100 and a
   divergence block (`tsc` accepts; C7). A compound write in such an
   initializer is value position and fails under rule 5 first. The
   checker wraps nothing in a lambda, and
   `ModuleEffectScanner` (§67) does not change. *(Added 2026-09-02
   after the review: the first implementation wrapped such an
   initializer in an immediately-invoked lambda and taught the
   scanner to walk a called lambda literal, which accepted a C14
   shape the pin rejected.)*
4. The rewritten form obeys the rules of the spelling it rewrites
   to: the write accessor must exist (§65 rule 6, S100 otherwise), a
   `readonly` index signature fails as today (§58 rule 6), and the
   operator checks through `bin_result` under
   `BinUse::CompoundAssignment`. A rejection names the spelling the
   program wrote (`x.v += 1`, `C.x++`), not the rewritten form; a
   plain assignment renders its value as `v` (`x.v = v`), as the
   pre-existing messages do.
5. The write used as a value stays rejected: `y = (x.name op= v)`,
   `y = x.name++`, and the index forms fail with S100 and a
   divergence block (`tsc` accepts).
6. Plain fields, array elements, `FixedArray` elements, locals, and
   globals do not change.

§58.1 rule 7 and §65.1 rule 7 are amended in place: the rejected
spelling is the write used as a value. C10 and C12 in
`collisions.md` drop compound assignment, increment, and decrement
from their narrowing lists. The `why` texts of
`Divergence::ClassIndexSignature` and `Divergence::NamedAccessor`
name the remaining rejections only.

Sites: `compiler/src/check/expr.rs` `check_assign` (the
`Place::Accessor` arm with a receiver, the static accessor arm, and
the `Place::IndexSignature` arm) and `check_update` (both arms);
`compiler/src/divergence.rs` (two `why` texts). No HIR form, no LIR
form, and no tier changes.

Corpus:

- `corpus/accept/a176-compound-through-accessor.ts` + `.expected`
  (golden from the dev JIT; ship and interpreter byte-identical): an
  instance accessor with `+=`, `-=`, `*=`, postfix `++`, and prefix
  `--`; a static accessor with `+=` and `++`; a class index
  signature with `+=` and `--`; a string accessor with `+=`; a
  receiver that is a call and an index that is a call, each call
  counted and the count printed after each statement. `tsc:
  accepts`; `js-comparable` measured against `node`.
- `corpus/reject/r173-compound-write-as-value.ts`: `const y: i32 =
  (c.v += 1);` S100 at the write; `tsc: accepts`, with a block.
- r130, r143, and r144 retire: the files, the harness rows, and the
  `language_reference.rs` rows are removed, as r104 in §64.

Unit tests in the same commit: HIR identity for the accessor form,
the static accessor form, and the index form with place operands;
`c.v++` in value position fails; a `readonly` index signature with
`+=` fails; a read-only accessor with `+=` fails with the §65 rule 6
message; a call receiver runs once (the a176 pin on both tiers).

### 82.2 R39.4 — three stable codes

1. **S016, unknown name.** A name that no declaration binds: a type
   name, a value name, a callee name, a class name after `new`, or
   an import that names an export the module does not declare. The
   code follows the fact, not the syntactic position. *(Clarified
   2026-09-02 after the review: `zz()` and `new Nope()` kept S100.)*
2. **S017, duplicate declaration.** A second declaration of one name
   in one namespace: a block scope, a `switch` body, a module's top
   level, the program's function set, a class's instance member
   namespace, or a class's static member namespace (§67.1 rule 3a,
   §71 rule 1, §65 rule 3).
3. **S018, unknown member.** A field, method, accessor, static
   member, or enum member that the receiver type does not declare,
   where the receiver type is a class, a value class, a mirror type,
   or an enum. A standard-library receiver (`string`, an array,
   `Map`, `Set`, `Math`, and the other stdlib.md types) keeps its
   subset code: its message states the accepted subset (Q24), and
   the checker cannot tell an absent member from one outside the
   subset. *(Clarified 2026-09-02 after the review.)*
4. Messages and positions do not change. Every other S100 message
   stays S100. S015 stays retired. `RuleCode::ALL` lists 18 codes,
   and each new code has an `explanation`.
5. A site that passes a `Divergence` keeps it. The three codes are
   ordinary type errors that `tsc` also reports, so a new site
   passes `None`, except the C14 sites that already pass
   `DeclarationScope`.
6. Every reject entry whose pinned diagnostic carries a moved
   message moves its `expected-error` header line and its
   `corpus_reject.rs` row to the new code. The tracking note lists
   every moved entry and every moved site.

§6's code table gains three rows. Sites: `compiler/src/diag.rs`;
the message sites in `compiler/src/check/{mod,expr,tyres}.rs` and
`compiler/src/lib.rs`; `compiler/tests/corpus_reject.rs`;
`compiler/src/language_reference.rs` where it names a code.

Corpus:

- `corpus/reject/r174-unknown-name.ts`: `const n: i32 = zz;` S016;
  `tsc: rejects TS2304`.
- `corpus/reject/r175-unknown-type-name.ts`: `const t: Q = new
  S();` S016; `tsc: rejects TS2304`.
- `corpus/reject/r176-unknown-member.ts`: `s.store(1)` on a class
  with no such method; S018; `tsc: rejects TS2339`.
- r146, r149, r150, r151, r161–r164 move to S017. r169 stays S100:
  the harness supplies its mirror, and its pinned diagnostic is the
  embedded-header copy. *(Corrected 2026-09-02: measurement 2 above
  ran `check` without the mirror.)*

### 82.3 R39.5 — `??`, and `?.` where `undefined` never binds

`??`:

1. The left operand has a C7 nullable type `Ref | null`. Any other
   left type fails with S100 "the left operand of `??` has type `T`,
   which is not nullable", with a divergence block (`tsc` accepts;
   C7).
2. The right operand checks with the context `Ref`. When its type is
   `Ref`, the result type is `Ref`. When its type is `Ref | null`,
   the result type is `Ref | null`. Any other type fails as the
   spelled assignment fails.
3. The left operand evaluates once. The right operand evaluates
   only when the left is `null`.
4. `??=` keeps its S100, with a divergence block (`tsc` accepts;
   C7). *(Block added 2026-09-02 after the review.)*
5. `??` establishes no narrowing outside itself.

`?.`:

6. A chain is a receiver followed by steps. A tested step, `?.name`
   or `?.name(args)`, tests its receiver. A plain step, `.name` or
   `.name(args)`, does not. An index step `?.[i]` fails with S100
   and a divergence block (`tsc` accepts; C7).
7. A tested receiver has a C7 nullable type; any other type fails
   with S100 and a divergence block whose fragments show `?.`, not
   `??`. A receiver whose type is already an error reports nothing
   more. After the test the receiver has the non-null type, and the
   step checks as the spelled member access or call on that type.
   *(Amended 2026-09-02 after the review: the first implementation
   reused the `??` variant and reported a second diagnostic on an
   error receiver.)*
8. A chain is legal in two positions: as the whole left operand of
   `??`, and in statement position (§82.1 rule 2: an expression
   statement, or the update clause of a `for`) when its last step is
   a call. *(Amended 2026-09-02 after review round 2: the update
   clause follows the one definition of statement position.)*
   In every other position the chain fails with S012 "an optional
   chain has type `T | undefined` in TypeScript; give it a fallback
   with `??` or use it as a statement", with a divergence block
   (`tsc` accepts `print(\`${x?.v}\`)`; C7).
9. `chain ?? y`, with `T` the type of the last step: when `T` is
   `Ref | null`, rules 1 and 2 apply with `T` as the left type. When
   `T` is any other type, `y` checks with the context `T`, and the
   result type is `T` (`x?.v ?? 0` has type `i32`).
10. Each tested receiver evaluates once. When a tested receiver is
    `null`, no later step evaluates; the `??` right operand
    evaluates, or, in statement position, nothing further happens.
11. No new HIR form and no new LIR form. `x ?? y` for a place `x`
    checks to the HIR of `x !== null ? x : y`; `x?.m();` for a place
    `x` checks to the HIR of `if (x !== null) { x.m(); }`. A test
    asserts each identity. A receiver that is not a place binds to a
    synthetic local (rule 3 of §82.1) whose declaration carries no
    side effect; the evaluation order of the statement does not
    change. In an initializer that has no statement list, a
    non-place receiver fails as §82.1 rule 3 states.

`collisions.md` C7 gains the two operators.

Sites: `compiler/src/check/expr.rs` `check_binary`
(`B::NullishCoalescing`), the `ast::Expr::OptChain` arm of
`check_expr`, and the expression-statement path in
`compiler/src/check/stmt.rs`; `compiler/src/divergence.rs` gains the
variants the three rejections need. No change in `codegen/`.

Corpus:

- `corpus/accept/a177-nullish.ts` + `.expected`: `??` with a `Ref`
  right operand, with a `Ref | null` right operand, and `a ?? b ??
  c`; a counted call as the left operand, with both outcomes; a
  tested field step and a tested method step; a two-step chain
  `a?.next?.v ?? 0`; a numeric member with `?? 0`; a string method
  with `?? ""`; `x?.m()` as a statement with `x` null and non-null,
  counted. `tsc: accepts`; `js-comparable` measured.
- `corpus/reject/r177-nullish-non-nullable-left.ts`: `const b: Box =
  a ?? c;` with `a: Box`; S100; `tsc: accepts`, with a block.
- `corpus/reject/r178-optional-chain-unbound.ts`: `print(\`${x?.v}\`);`
  S012 at the chain; `tsc: accepts`, with a block.

Unit tests in the same commit: the two rule 11 identities; `??=`
stays S100; a value-class receiver fails as rule 1; a chain in
argument position fails as rule 8; a right operand of the wrong type
fails; `?.[i]` fails; the right operand of `??` runs only on `null`
(the a177 pin on both tiers).

### 82.4 R39.6 — method type parameters, instance and static

1. A method on a non-generic reference class or a non-generic
   `@CStruct` value class, instance or static, declares type
   parameters as a free function does (§64). The parameters are in
   scope in the signature and the body.
1a. A template carries a body. A bodiless template, method or free
   function, fails with S100 "function bodies are required" at
   collection (`tsc` TS2391). Two type parameters of one name fail
   with S017 at collection (`tsc` TS2300). A bodiless generic method
   in a `declare class` written in a `.ts` source is the same S100
   with a divergence block, because `tsc` accepts that form; the
   site separates the `declare` case from the plain-class case. A
   template rejected at collection reports nothing more at a call
   that instantiates it, and a duplicate type parameter name counts
   once. A template that no call instantiates is otherwise not
   checked, as §64 has it. *(Added
   2026-09-02 after the review, CRITICAL: `class C { m<T>(v: T): T; }`
   checked clean and ran; the free-function form had the same hole
   at the pin.)*
2. A call supplies explicit type arguments: `recv.m<A>(args)` and
   `C.m<A>(args)`. A call without them fails with S100 "generic
   method `m` requires explicit type arguments", with a divergence
   block (`tsc` infers; the variant cites §64). A wrong count fails
   as `instantiate_fn` reports.
3. Each distinct type-argument list yields one instance, named
   `m<A>` in the class's instance or static method table, checked
   once at its first call as a method of that class (§64 rule 4).
   The HIR holds the instance as an ordinary method; no template
   reaches the HIR. Every consumer of a method name sees `m<A>`. The
   ship tier names a method by its LIR function id (`sub_f<id>`), so
   two instances are distinct by construction; the test asserts two
   HIR names and two LIR ids. *(Corrected 2026-09-02 after the
   review: the first text cited §65 rule 10, which applies to no
   method name.)*
4. The declared name `m` owns the member name in its namespace
   (§67.1 rule 3a). An instance name holds `<`, so it collides with
   no declared member.
5. Excluded, each S100 with a divergence block (`tsc` accepts): a
   generic method on a generic class ("the checker holds one
   substitution"; the variant cites §64), and an `async` generic
   method (the await grammar gains no form; §64 rule 3). A type
   parameter list on an accessor or a constructor keeps today's
   rejection. A mirror class and a `@Descriptor` class keep theirs.
   *(§93, 2026-09-08: the `async` half is superseded. An async
   method with type parameters on a non-generic reference class is
   accepted. The generic-class half stands.)*
6. `this`, a value-class receiver, `private`, and method-as-value
   keep their rules on an instance.
7. Lowering: an instance is an ordinary method; a static instance
   is an ordinary static method (§71 rule 7). No tier changes.

Sites: `compiler/src/check/mod.rs` (`ClassSig` gains a template map
for generic methods in each namespace; `resolve_class_shape`
collects a method with `type_params` as a template and reports rule
5; `check_class_body` skips templates; a new `instantiate_method`
beside `instantiate_fn`); `compiler/src/check/expr.rs` (the
instance-method call path, the static call path at "static method
`C.m` is not generic", and the await path at "method `m` is not
generic"); `compiler/src/divergence.rs`;
`compiler/src/language_reference.rs`.

Corpus:

- `corpus/accept/a178-generic-method.ts` + `.expected`: a reference
  class with `identity<T>(v: T): T` called at `i32`, at `string`,
  and at a `@CStruct` value class; a static `create<T>(v: T): Box`;
  a method with two type parameters; a generic method that calls a
  generic free function; one instance called twice; a generic
  instance method on a `@CStruct` class. `tsc: accepts`;
  `js-comparable` measured.
- `corpus/reject/r179-generic-method-without-type-args.ts`: S100;
  `tsc: accepts`, with a block.
- `corpus/reject/r180-generic-method-on-generic-class.ts`: S100;
  `tsc: accepts`, with a block.
- `corpus/reject/r181-async-generic-method.ts`: S100; `tsc:
  accepts`, with a block.

Unit tests in the same commit: two calls at one type list yield one
instance; the HIR method name is `identity<i32>`; a static instance
lowers through the static symbol; a wrong type-argument count fails;
the emitted C for `m<i32>` beside `m<u32>` holds two distinct
identifiers and compiles.

### 82.5 R39.8 — a static read accessor without a write accessor

1. A static read accessor with no write accessor is legal, as the
   instance form is (§65 rule 6). The write spelling `C.name = v`
   then fails with S100 at the assignment.
2. §71 rule 4 is amended in place. `Divergence::NamedAccessor`'s
   `why` names no static accessor.
3. r147 retires: the file, the harness row, and the
   `language_reference.rs` row.
4. `corpus/accept/a179-static-read-accessor.ts` + `.expected`: a
   static read accessor over a static field, read twice around a
   write to the field. `tsc: accepts`; `js-comparable` measured.
5. Unit test: `C.name = v` through a static read-only accessor fails
   with S100.

Site: `compiler/src/check/mod.rs` `resolve_class_shape`, the
`read_accessors` loop.

### 82.6 R39.7 — `tools/downstream.sh`

A shell script that runs the downstream gate against this working
tree.

1. The downstream checkout comes from the environment variable
   `SUBSCRIPT_DOWNSTREAM_DIR`. The script has no default path
   (CLAUDE.md: no path outside this repository in a committed file).
   If the variable is unset, or the directory holds no
   `tools/gate.sh`, the script prints one usage line and exits 2.
2. If `.cargo/config.toml` exists in the downstream checkout, the
   script prints one line that names it and exits 2. It never
   overwrites a file it did not write.
3. The script writes `.cargo/config.toml` with a `[patch]` section
   for `https://github.com/infosia/subscript.git` that maps
   `subscript-compiler`, `subscript-codegen`, and `subscript-bindgen`
   to this repository's crate directories, resolved from the
   script's own location. It prints this repository's `HEAD` hash
   and whether the working tree is clean.
4. It runs `tools/gate.sh` in the downstream checkout and passes
   every argument through (`--require-backend` included).
5. On exit, by every path, it deletes the file it wrote and runs
   `git checkout -- Cargo.lock` in the downstream checkout. It exits
   with the gate's status.
6. The script is not a CI gate. It runs once before a pin candidate
   is named; the result is recorded with the candidate in the
   tracking note. A red run yields the smallest failing program as a
   `corpus/` entry before the pin moves.
7. `tools/hygiene.sh` passes with the script in the tree.

Verification: one run on this host against the working tree after
§82.1–§82.5 land, recorded in the tracking note with the counts the
downstream gate prints.

### 82.7 Not decided here: R39.1 and R39.9

Recorded for the owner. Neither has a corpus entry. *(Owner decision
2026-09-02: both deferred. The narrowed overload shape is recorded in
`specs/tracking/r39-overloads-deferred.md`.)*

**R39.1, `Ref<T>` parameters (option A).** Zero downstream sites
today. Option B is closed by §81.1 (a by-value parameter crosses the
C ABI by value). Option A adds a parameter type that binds to the
caller's storage. The cost: a HIR type form, an argument rule (the
argument must be a place), and one by-address parameter form in each
of the three LIR consumers (dev JIT, C emitter, interpreter); the
form exists for `this` on a value class (§68, `LoadAddress`) but not
for a parameter. The entry signature for a host-called function that
takes `Ref<T>` becomes a pointer parameter.

**R39.9, overloads by parameter type.** The request's rule 4 states
that `tsc` and this compiler pick the same signature on every
accepted program. Measured against the aliases in `prelude/lang.d.ts`:
`i32` and `f32` are both `number` to `tsc`, so `tsc` picks the first
numeric signature in declaration order for every numeric argument,
and this compiler picks by the nominal sized type. `abs(x)` with
`x: i32` resolves to `abs(f32)` under `tsc` and to `abs(i32)` here.
Rule 4 therefore holds only for overload sets whose signatures
differ by a class type; two numeric signatures are a divergence that
no accepted program can show. Other costs: a method table keyed by
one name per signature needs a third reserved-name family
(`mul(f32)`, after `name=` and `m<A>`); the downstream kernel
generator keys on the method name and the argument count and must
move to the instance name; `instanceof` and `typeof` are S100 today
and would gain a fold-only meaning inside an instance; the collector
must accept a bodiless signature; C7 gains an exception for a union
in a signature position.

### 82.8 Exit criteria (pre-registered)

1. Red at `e1c2be1`: every measurement above, recorded (this host).
2. `a176`–`a179` byte-identical across dev JIT, ship C-AOT, the
   interpreter, and the golden. `tsc` accepts each under the corpus
   gate's options. Each `js-comparable` header is measured.
3. r173–r181 pin the code and the line; every `tsc: accepts` entry
   renders a divergence block, every `tsc: rejects` entry renders
   none (§79 rule 4).
4. r130, r143, r144, and r147 are removed with their rows.
5. No pre-existing golden or `.expected` moves. The zero-warning
   sweep stays green.
6. `cargo test --offline --workspace` in both profiles; zero-warning
   build; `cargo fmt --check`; the `tsc` gate; `tools/hygiene.sh`;
   clippy at the baseline 7 / 18 / 13. The record quotes the test
   count and the wall time.
7. `tools/downstream.sh` runs green against the working tree, or
   its red run yields a corpus entry (§82.6 item 6).
8. `generated-docs/` regenerates.

### 82.9 Review round 1, 2026-09-02

Fresh no-context review of `25c9437..ec41d65`, execution-verified.
CRITICAL: a bodiless generic method template checked clean and ran
(rule 1a). MAJOR: S016 and S018 followed the syntactic position
(rules 1 and 3 of §82.2 clarified); an initializer with no statement
list was wrapped in an immediately-invoked lambda and
`ModuleEffectScanner` walked a called lambda literal, which accepted
`const n: i32 = ((): i32 => 3)()` where the pin rejected it under C14
(§82.1 rule 3); a `?.` on an error-typed receiver reported a second
diagnostic with a block (§82.3 rule 7); no tracking note existed at
review time. MINOR: r169 mis-measured (§82.2 corrected); the `for`
update clause (§82.1 rule 2); `??=` without a block (§82.3 rule 4);
the emitted-C test could not fail (§82.4 rule 3); two assertion
messages omit a176; a Rust constant duplicated the `retired:` marker
of `collisions.md`; the `??` variant rendered for `?.`; rejection
messages lost the written spelling (§82.1 rule 4).

Fix round obligations, beyond the rules above: revert the lambda
wrap and the scanner change; `compiler/tests/js_corpus.rs` reads the
`retired:` markers and holds no retired list; the two assertion
messages name a176; the C14 shape above is a unit test that fails at
the pin's message; a bodiless generic free function and a bodiless
generic method are unit tests; `for (…; …; c.v++)` is in a176.

### 82.10 The synthetic prefix is scoped, and a boundary check is total

*(Owner decision 2026-09-02, after review round 2. Two rounds raised
one class: where the statements that bind a synthetic local go. Round
1 found them wrapped in a lambda; round 2 found a per-function stash
that a `for` initializer, a `for` condition, and an arrow body drain
into the wrong list, so `for (…; i < 3 && (maybe() ?? fb).v > 0; …)`
and `(maybe() ?? fb).v + ((): i32 => 2)()` check clean and fail at
lowering with "unknown local [[compound#0.nullish]]". The class is a
defect of the form, so this section states the form.)*

1. **A prefix belongs to one owner.** The statements that bind the
   synthetic locals of an expression (§82.1 rule 3, §82.3 rule 11)
   are collected by the innermost owner under check. The owners are:
   a statement in a statement list; a `for` initializer; a `for`
   condition; a `for` update; an arrow body; an initializer that has
   no statement list; a `switch` case test; a declarator of a
   declaration with two or more declarators *(added 2026-09-05,
   §87.1 rule 2)*. Entering an owner starts an empty collection;
   leaving it drains the collection. An outer owner's collection is
   saved on entry and restored on exit, as `subst` is around an
   instantiation.
2. **Each owner drains into a list it owns.** A statement drains
   before itself in its list. A `for` initializer drains before the
   loop. A `for` condition drains at the head of the loop body in the
   `while` form the update rewrite already uses, so it runs on every
   iteration before the test. A `for` update drains into the step. An
   arrow body drains at the head of the body. An initializer that has
   no statement list rejects (§82.1 rule 3). A `switch` case test
   rejects the same way, with the same block: the tests run in order
   until one matches, so no list before the `switch` can hold the
   local without a change of evaluation order. *(Added 2026-09-02
   after review round 3: the boundary check reported the shape.)*
3. **The boundary check is total.** *(Superseded 2026-09-05 by §87.1
   rule 3: in the scoped form the drain is the boundary, and the
   S100 report below has no input that can fire it.)* On leaving
   every owner the collection is empty, or the checker reports S100
   "internal: synthetic prefix escaped its owner" at the expression's
   position.
   The check compares the collection against the owner boundary, not
   against the site that filled it. No test writes into the
   collection: the check is an internal invariant, and its witness is
   the `switch` case shape of review round 3, which it reported
   before rule 1 named that owner. *(Amended 2026-09-02: the first
   text asked for a test hook, which changed the record the check
   reads, against core principle 9.)*
3a. **Closed, measured 2026-09-06.** The §87 review claimed that a
   `while` condition's prefix drains before the loop, so
   `while ((maybe() ?? fb).v > 0)` would call `maybe()` once. Measured
   on the dev JIT and under `node` with a receiver that returns a
   new object twice and then `null`: `3,2` on the dev JIT, the ship
   tier, and `node`, for `??` and for `?.v ?? 0` — the call runs on
   every iteration. The prefix of §82.3
   is `let [[c0]] = null;` and the assignment stays inside the
   condition expression, so where the `Let` lands does not move the
   call (the same fact as §87.1 rule 2's note). No defect; no owner
   is added for `while`.
4. **No lowering failure is the report.** A program that checks
   clean lowers on both tiers. The four probes above are in a177, and
   the empty-body `for` with a prefix in its condition (green at
   `ec41d65`, red at `61c31e9`) is in a176; the golden gate runs both
   on the dev JIT, the ship tier, and the interpreter, in both
   profiles. No test under `compiler/tests/` runs a `target/<profile>`
   binary by path. *(Amended 2026-09-02 after review round 3: the
   first probes hard-coded `target/debug/subscript`.)*

### 82.11 Review round 3, 2026-09-02

Focused review of `e1b0b90..8c93b7d`. MAJOR: a `switch` case test was
not an owner, and the rule 3 check reported the shape (rule 1 and
rule 2 amended: the test rejects); the rule 4 probes ran
`target/debug/subscript` by path in both profiles (rule 4 amended:
the probes are corpus content). MINOR: the boundary test wrote into
the collection it checked (rule 3 amended: no hook); no owner-exit
enforcement exists, and no exit path skips the check today (recorded,
not changed); a plain `=` renders `v` (§82.1 rule 4 states it); an
unexported import reported twice (the imported name is poisoned after
the first report); the a176 header omits the nullish shape (fixed).
Outside this diff: a generator used only through `for…of` does not
lower on either tier, and the two tiers report the failure with
different text and exit codes; `a79` passes because it also calls
`next()` by hand. That defect needs its own request and its own
corpus entry.
