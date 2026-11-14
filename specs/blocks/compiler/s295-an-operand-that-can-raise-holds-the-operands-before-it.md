<!-- §295 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 295. An operand that can raise holds the operands before it

*(Added 2026-11-15.)* Origin: §256.4 item 3. On 2026-11-15 the owner
told Claude to do each item of a list of open runtime defects, in an
order that Claude selects. This item is one of them.

Problem, read at `e145bb35`. §255 rule 1 holds a counted operand (an
async handle, a generator, or a container that holds one) if a later
operand runs user code (§255 rule 2). The hold owns one count, and each
exit from its span releases it, the exception edges included. An
operand that runs no user code by §255 rule 2 can still raise:

- A descriptor literal that omits a field with a default evaluates the
  field initializer. If the initializer throws, the literal raises.
  `runs_user_code` (`codegen/src/lir/array_ownership.rs`) does not read
  an omitted field initializer.
- A parameter default `d: D = {}` is the same literal.
- In a hot reload build, each default call has a raise edge.

So in `gx(gen(), {})`, and in `gx(gen())` with `gx(g: Generator<i32>,
d: D = {})`, the fresh generator has no owner at the raise edge of the
literal. §256.4 item 3 reads a leak of 40 bytes for each raise of a
generator, and 16 bytes for each raise of an async handle. This program
is accepted at `e145bb35`, and prints `1` and `caught 100` on the debug
CLI:

```ts
function boom(): i32 { throw new Error("boom"); }
let fire: boolean = false;
function initX(): i32 { if (fire) { return boom(); } return 1; }
@Descriptor
class D { x?: i32 = initX(); }
function* gen(): Generator<i32> { yield 1; }
function gx(g: Generator<i32>, d: D = {}): i32 { return d.x; }
function gy(g: Generator<i32>, d: D): i32 { return d.x; }
export function main(): void {
  print(`${gx(gen())}`);
  fire = true;
  let caught: i32 = 0;
  for (let i = 0; i < 50; i++) {
    try { gx(gen()); } catch (e) { caught++; }
    try { gy(gen(), {}); } catch (e) { caught++; }
  }
  print(`caught ${caught}`);
}
```

The class of the defect: the hold decision reads a list of expression
kinds, and the raise edges come from another fact
(`trap_sites_for_reload`). A raise site that the list does not name
leaks.

### 295.1 Rules

1. §255 rule 2 changes. An operand *can change or raise* if it runs
   user code as §255 rule 2 states, or if its evaluation has a site
   that raises to a handler. The raise part reads
   `Module::expression_can_raise` (`compiler/src/raise_sites.rs`,
   §115.6), and, in the hot reload build, the raise edge of each
   default call. A descriptor literal or a `new` that evaluates an
   omitted field initializer reads that initializer.
1a. The *later operands* of an operand of a call or a `new` are the
   expressions that the lowering evaluates after it, in the order of
   §57.1: the later arguments, then for a `new` each field
   initializer that runs inline, then the defaults of the omitted
   parameters. One function gives this order to the lowering, and one
   function gives it to the release model. A hold reads "a later
   operand can change or raise" from that order, not from a list in
   each consumer.
2. Each site of §255 rule 1 that reads "runs user code" reads "can
   change or raise". The other parts of §255 rule 1 and rule 3 do not
   change: a read of an immutable local is not held, and a hold
   releases its count on each exit of its span.
3. The lowering and the release model of the LIR test
   (`codegen/tests/support/lir_facts_release.rs`, §255 rule 4) read
   rule 1 separately from the HIR. The model does not read the
   lowering.
4. A trap that ends the program is not a raise site. No hold is added
   for it.
5. No accepted program changes its output. The development tier, the
   ship tier, and the interpreter give the same output.

### 295.2 Acceptance

1. Red first, at the contract commit: a test in `codegen/tests/` runs
   the program of the problem statement in the development tier, with
   the memory accounting of a reload session, and reads the live
   allocations after `main`. At the contract commit the count grows
   with the iterations. The tracking note records the two counts (for
   example 10 and 50 iterations).
2. After the change, the count equals the count of a control in the
   same test: the same shape where the argument is an immutable local
   (`const g = gen(); gx(g);`). The control gives the same count at the
   contract commit and after the change. Both counts grow with the
   iterations, because no collection runs in the program (invariant 2);
   the leak is the difference to the control.
3. The release model accepts the LIR of the program in the normal build
   and in the hot reload build. If the hot reload build has a default
   call raise edge that a test can drive, the test drives it;
   otherwise the release model of the hot reload build pins it.
4. A unit test pins rule 4: an operand with a trap that ends the
   program and no raise site adds no hold (the LIR has no retain for
   it). The HIR and the LIR have no integer overflow trap, so the test
   uses the trap of an integer division.
5. No corpus golden changes. If the LIR text snapshot changes, report
   the moved functions and stop.
6. The full gate passes.

### 295.3 Phase Review

Two fresh reviews. No CRITICAL finding.

The first review:

1. MAJOR, realistic, fixed: a `new` evaluates each field initializer
   between the explicit arguments and the defaults (§57.1), and the
   hold of an explicit argument did not read the initializers.
   `new C(gen())`, where a field initializer of `C` throws, leaked one
   generator for each raise (live allocations 35 and 155 at 10 and 50
   iterations, against 25 and 105 for the control). Each consumer had
   its own list of later operands. Rule 1a now gives the order from
   one function in the lowering and one in the release model.
2. MINOR, fixed: the raise part read `trap_sites_for_reload` at each
   node. It added no coverage and allocated at each call. Rule 1 now
   reads `Module::expression_can_raise`.
3. MINOR, fixed: the leak test asserted constants in two tests. One
   test now compares the program with its control.
4. MINOR, fixed: the rule 4 test had no firing control.
5. MINOR, contrived, recorded: §295.4 item 1.

The second review:

1. MAJOR, realistic, fixed: the release model gave no parameters to a
   named-lambda call (§288 defaults), so its order disagreed with the
   lowering. A correct program got a false finding. The model now reads
   the parameters of a lambda call from the HIR.
2. MINOR, fixed: the receiver hold of §255 rule 1 site 1 read a list of
   its own. It now reads the order of rule 1a.
3. MINOR, contrived, recorded: §295.4 item 2.
4. MINOR, fixed: the tracking note did not name rule 1a.

### 295.4 Open

1. The receiver hold of a method call reads the defaults of all
   parameters, the supplied ones included, in the hot reload build. No
   program reaches a difference: a counted receiver is a built-in
   receiver, and built-in parameters have no defaults.
2. An omitted descriptor field initializer adds no hold in two cases.
   In the hot reload build, an initializer call that cannot raise gets
   a raise edge (§121 rule 1) that `expression_can_raise` does not
   read. In each build, an initializer that runs user code with no
   raise site, and that changes an earlier argument, is not read as
   user code. The lowering and the release model agree, so the model
   cannot see it (core principle 12).
