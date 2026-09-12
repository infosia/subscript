<!-- §83 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 83. The operation-signature table is a total function of the HIR

*(Owner decision 2026-09-03. Origin: review round 3 of §82 found the
defect outside that diff.)*

Measured at `088acac`, this host. A generator consumed only through
`for…of`:

```ts
function* values(): Generator<i32> { yield 3; yield 5; yield 8; }
export function main(): void {
  for (const v of values()) { print(`${v}`); }
}
```

`subscript check`: no errors. `subscript run`: exit 2, "internal
lowering error: LIR construction failed: … call disagrees with the
signature table: BuiltinMethod.GeneratorNext declares , got
[Data(Generator(I32))] -> Some(Data(IterResult(I32)))". `subscript
build --source … --run`: the same finding under the prefix "internal
error:". `a79` passes because it also spells `generator.next()`.

Cause. `hir::Module.operation_signatures` is filled by
`register_operation_signature`, which `check_expr` calls on every
expression it checks. A HIR call that the checker synthesizes does not
pass through `check_expr`. `check_for_of` builds the `next` call by
hand and registers nothing; eight other sites (`check/json.rs`, the
§82.3 chain code, an initializer path) remember to call the
registration by hand. The LIR verifier is total and reports the
missing entry, which is the wanted behaviour of a verifier and the
wrong behaviour of a checker: a program that checks clean must lower.
The table is a record written beside the form (core principles 8 and
9), and every synthesized node is a site that can forget it.

### 83.1 Rule

1. **One owner iterator, one walk.** `hir::Module` exposes one
   iterator over every module-level expression owner it holds, in a
   shared form (`&self`) and a mutable form (`&mut self`): each
   function, method, and constructor body and each of their parameter
   defaults; each field initializer, static initializer, descriptor
   default, and module initializer; the top-level statements. A
   lambda body and its parameter defaults are not module-level
   owners: `children()` reaches them under the owner that holds the
   lambda, and a consumer recurses through `children()`. Every
   module-level pass that reads expressions uses the iterator: the
   §83 walk, `trap_sites`, `warn`, and the LIR fact check in
   `codegen/tests/support`. Two passes are exempt because they need
   the owner's identity (its function name or class index), which the
   iterator erases: `check/layout.rs`, which runs on the checker's
   pre-module state, and the lowering in `codegen/src/lir.rs`. No
   other pass enumerates owners by hand. After the module is checked,
   the walk visits every `Call` node under every owner through
   `children()` (§72) and derives `operation_signatures` from the
   nodes alone: the target from the callee and the receiver type, the
   parameter types from the receiver and the arguments after
   `normalize_operation_parameter_types`, the return type from the
   node's type. Two nodes with one signature give one entry. The walk
   is total over the form, not over the programs the language accepts
   today: a lambda parameter default is in the form and is visited,
   although no accepted program evaluates one (a call through a
   function value supplies every argument). *(Amended 2026-09-03
   after review round 1: the first walk listed owners by hand and
   skipped parameter defaults, which `trap_sites` three lines later
   did visit; `factor: f64 = Math.max(2.0, 3.0)` lowered at the pin
   and failed at `35fe70e`. Amended again after review round 2: the
   iterator was `&mut self` only, so `warn` and the LIR fact check
   kept their hand lists; the foreign-function arm was unreachable,
   because a `declare function` has no body and no default.)*
2. **No site registers a signature as a side effect.**
   `register_operation_signature` and the checker's `RefCell` table
   are deleted, and so is every call of it. A synthesized node is
   registered because it is in the HIR, not because a site remembered
   it.
3. **One target mapping, in the compiler crate.** The callee-to-target
   mapping is one function in `hir.rs`. The walk, the `hir.rs` lookup,
   and the LIR lowering in `codegen/src/lir.rs` all call it; the
   lowering maps the `hir::BuiltinMethod` it returns to the LIR
   builtin through the existing bridge and holds no receiver-and-name
   match of its own. *(Amended 2026-09-03: the lowering held a third
   copy.)*
4. **One stated exception.** The lowering of `Arr::Map` and
   `Arr::Filter` synthesizes an `ArrayPush` that no HIR node spells,
   and appends that signature at lowering. This is the only entry a
   HIR walk cannot see; a second such entry needs a rule here first.
5. **The HIR JSON does not change shape.** `operation_signatures` is
   the same field with the same entry form; the fix adds the entries
   the walk finds that the side effect missed.

### 83.2 Corpus and gate (pre-registered exit criteria)

1. `corpus/accept/a180-for-of-generator-only.ts` + `.expected`: a
   generator consumed only through `for…of`, with no `.next()` in the
   program; one loop that runs to the end, one that `break`s at the
   second value, one that `continue`s past a value, and one generator
   whose element type is a `@CStruct` value class read through a
   field. Red at `088acac`: dev exit 2 with the text above. `tsc:
   accepts`; `js-comparable` measured.
2. `corpus/accept/a181-operation-in-every-owner.ts` + `.expected`: a
   program whose only operation-table calls sit one per owner kind of
   rule 1 (a free-function parameter default, a method parameter
   default, a constructor parameter default, a lambda body, a field
   initializer, a static initializer, a descriptor default, a module
   initializer, a generic instance body, an async body). Red at
   `35fe70e` for the parameter-default forms (dev exit 2, `Math.Max
   declares ,`). `tsc: accepts`; `js-comparable` measured.
3. Unit test, principle 9: the table the walk derives for a180 and for
   a181 equals a **hand-written** expected list in the test, entry for
   entry. The test derives nothing with production code.
4. A total test over every entry under `corpus/accept/`, `corpus/warn/`,
   and the top-level `examples/e*.ts`: every `Call` node whose callee
   is an operation-table target has a matching table entry, every
   violation reported at once. The three examples that need a mirror
   are gated by `examples/tests/gate.rs` at the LIR level. **Positive
   control:** the test takes a checked module, inserts a synthesized
   `Call` node with a new signature into an owner (a parameter default
   and a field initializer), and asserts the check reports that node
   and nothing else. The interop token list the test needs lives in
   one shared test module beside `corpus_warn.rs`, not in a copy.
5. The owner iterator has its own test: a program with one `Call` in
   each owner kind, and a hand-counted number of calls the iterator
   must yield. a181 holds a top-level statement with an operation
   call, so the hand-written table of item 3 covers the top-level
   owner. The total test of item 4 shares the iterator with the walk
   and cannot see an owner the iterator skips; the hand-written tables
   and this test are the controls for that.
6. **A totality gate gains no per-entry exception.** The LIR
   execution-fact helper resolves a method call's operand count from
   the method's declared parameters, as it does for a free function
   and a constructor, so a method call that omits a defaulted argument
   is one more shape, not a named entry. *(Added after review round
   2: the first fix round special-cased a181 in
   `codegen/tests/lir.rs`.)*
7. `a79` and every pre-existing golden stay byte-identical. The record
   lists the HIR entries the walk added (measured on this host with a
   binary from the pin): `Ambient(Unreachable)` in a116, a162, a163;
   order only in a136, a146, a69–a72, a79.
8. Gates: both profiles, zero-warning build, fmt, `tsc`, hygiene,
   clippy at 7 / 18 / 13.

### 83.3 Recorded, not changed

The dev command reports the finding under "internal lowering error:"
and the ship command under "internal error:". After this section no
program that checks clean reaches either text. The prefix parity is a
CLI record (`cli.md`), not a checker rule.

### 83.4 Review round 1, 2026-09-03

Fresh review of `7f46a3a..35fe70e`. CRITICAL: the walk listed owners
by hand and skipped parameter defaults (rule 1 amended: one owner
iterator on `hir::Module`, shared with `trap_sites`); the two tests
re-derived the table with copies of the production code and could
not fail (§83.2 items 3–5 rewritten: hand-written expected tables, a
positive control, an iterator test). MAJOR: no positive control
(item 4); a third copy of the callee mapping in `codegen/src/lir.rs`
(rule 3 amended). MINOR: the lowering appends an `ArrayPush` entry for
`Arr::Map`/`Filter` (rule 4 states the exception); the examples list
(item 4 states the scope); no HIR JSON diff test exists because no HIR
serializer exists (item 6 records the measurement instead); the
interop token list was a copy (item 4); the lookup is order-sensitive
and the order moved for seven entries with no golden change
(recorded); the tracking note was untracked; `subscript build` writes
beside its source (a probe hazard, recorded in the tracking note).

### 83.5 Review round 2, 2026-09-03

Focused review of `90be0c2..efdc31d`. MAJOR: the LIR execution-fact
helper gained an `if id == "a181…"` exception instead of resolving a
method's parameter count (item 6); the iterator was `&mut self` only,
so `warn.rs` and `codegen/tests/support/lir_facts.rs` kept hand lists
(rule 1, second round on the class: both borrow forms, exempt passes
named). MINOR: rule 1 said the iterator yields lambdas while the code
reaches them through `children()` (rule 1 states it); an unreachable
`foreign_fns` arm (removed); the lambda-default arm in `children()`
changes no accepted program (rule 1 states the form-totality); the
total test shares the iterator (item 5 states the controls); the
iterator test counts calls, not arms (its comment says so); the
compiler-side interop token list lacks three tokens of the
codegen-side list (synchronized); a `next` receiver match at the
reload trap site is outside rule 3 (recorded); the tracking note's
"independent walk" wording (corrected); a stale `top_level` comment
and a missing `#[non_exhaustive]` reason (fixed).
