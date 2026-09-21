<!-- §112 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 112. Position id 0 is "no script site"

*(Added 2026-09-19.)* Origin: the §111 Phase Review, finding m2.
Evidence: `specs/tracking/s111-callback-registration.md`, "Open:
position id 0 names an unrelated site".

Problem: a trap record carries a position id, and each tier resolves
it through its own position table. Id 0 is a valid index. The runtime
also records 0 where no script site exists: the quota refusal of a
callback binding (`Context::bind_callback`), of a registration
(`Context::register_callback`), and of one array operation
(`runtime/src/arrops.rs`), and the fire-time userdata check when the
freed allocation carries no position. Each tier then reports entry 0 of
its table, which is an unrelated site.

Measured under the sandbox profile, with a quota that the callback-info
crossing is the first charge to pass: both tiers print
`AllocationQuota at prog.ts:1:17`, the position of `main`. The two
tables are built apart and in different orders, so the tiers agree only
when their first entries are equal. In §111's second trap entry they
differed: the dev JIT reported `12:5` and the ship tier `18:29`. No
golden pinned either case (core principle 12). §111 closed the class
for one trap kind by name. A kind cannot close it in general, because
`allocation-quota` carries a real site from most paths and none from
the three above.

The form is the defect: a position id cannot say "no position".

### 112.1 The rule

1. **Position id 0 means that no script site exists.** Every position
   table holds one reserved entry at index 0: an empty file name, line
   0, column 0. The ids that a lowering gives to script sites start
   at 1. This holds for the dev-JIT table, the ship-C table, and the
   table that `program.alloc.h` publishes to the host, so a host still
   reads `subscript_alloc_positions[pos_id]` with no arithmetic.
2. **The reservation is a fact of the table, not of a consumer.** The
   table is a type, not a bare vector. Its one constructor puts the
   reserved entry in place, and its one way to add a site answers the
   id. `Default` is that constructor under a second name, because a
   lowering takes its table out with `std::mem::take`. No code, and no
   test, can build a table without the reserved entry. No
   consumer tests the kind of a trap to find the position, and
   `trap_has_no_script_site` goes. §111 rule 14's last sentence, that
   each tier maps the kind by name, is superseded by this rule; the
   behaviour that §111's second trap entry pins does not change. The
   interpreter's map from a trap kind to a LIR trap site is not a
   position table. It stays, and it answers "no script site" for
   `callback-registration-ended` by name, not through its fallback arm.
   That answer is apart from "no site matches": for the first the
   interpreter reports the empty position of rule 3, and for the
   second it reports the instruction that ran, as it does today.
3. **Every tier reports id 0 as the empty position**, with the text it
   gives §111's second trap entry today. An id outside the table keeps
   its present report on each tier.
4. **A runtime path records 0 only when no script site exists.**
   *(Amended 2026-09-21, §113: the quota refusals of a binding, a
   registration, and a sort are removed with the quota, and the
   position parameter that each call took for them goes.)* The two
   paths that keep 0 state in one comment why no site exists: the
   fire-time userdata check with no header for the address, and the
   refused fire of §111 rule 14. Every other literal 0 in the runtime
   is outside this section: rule 1 already stops it from naming an
   unrelated site, and a better site for it is a diagnostic question.
   The tracking note lists those paths by group.
5. **The fire-time userdata check keeps its rule** (§14.4b (A)): the
   position is the site of the freed allocation when the runtime holds
   it, and 0 when it does not. With rule 1, that 0 is now the empty
   position on every tier.
6. **The host tutorial states the meaning of id 0** where it shows the
   host how to resolve a position.

### 112.2 Corpus

*(Amended 2026-09-21, §113: the two quota trap entries, `t61` to
`t63`, are removed with the quota. §111's second trap entry pins
rule 3.)*

No `.expected` file of an existing entry moves. A listing that prints a
position id moves by one for each id; with the ids normalised it does
not move. If a position, as file, line, and column, moves in any
existing test table, stop and report.

### 112.3 Exit criteria

1. For each tier, a test reads the finished table and finds the
   reserved entry at index 0, and finds that the first script site of a
   one-line program has id 1. The test reads the table that the tier
   built, not the constructor's expression.
2. A test compares the entry at index 0 of the dev-JIT table, the
   ship-C table, and the published `program.alloc.h` table for one
   program: all three are the reserved entry.
3. *(Deleted 2026-09-21, §113.)*
4. §111's second trap entry passes with no edit of its expectation,
   and no code names a trap kind to decide a position.
5. *(Deleted 2026-09-21, §113.)*
6. The standing gate is green. Each new test states its measured cost
   (core principle 15).
