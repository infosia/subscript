<!-- §300 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 300. A statement place drops each value leaf

*(Added 2026-11-15.)* Origin: §258.4 item 1. On 2026-11-15 the owner
selected this item.

Problem, measured at `464c4c9d` with `tsc` 5.9.2 and the debug CLI. `k(): i32` prints `k` and returns 1, `b` is a
`boolean`, and `n` is an `i32` local. `tsc` accepts each form with no
error (§258.4 item 1). The checker rejects each one.

| Form | Checker |
|---|---|
| `b && k();` | S100 "logical operators require booleans, got `i32`" |
| `b \|\| k();` | the same |
| `b && 5;` | the same |
| `b && (n = 3);` | the same |
| `b ? 1 : "s";` | S100 "conditional branches have no common type: `i32` and `string`" |

§258 rule 2 makes each branch of a conditional, and the right operand of
`&&` and `||`, a statement place, but drops a value leaf only beside a
`void` leaf. With no `void` leaf, the checker checks the whole
expression as a value. These controls run at `464c4c9d`: `b ? k() : 2;`
(a common type), `k();`, and `5;`.

### 300.1 Rules

1. §258 rule 2 changes. In a statement place, each branch of a
   conditional, and the right operand of `&&` and of `||`, is a
   statement place, also when no leaf is `void`. Each value leaf is
   evaluated and its value is dropped, as in an expression statement.
   The branches need no common type.
2. The left operand of `&&` and of `||`, and the condition of a
   conditional, keep their check: each is a `boolean` (C24 row 1: no
   truth test on a value that is not `boolean`).
3. A leaf that a statement cannot hold keeps its diagnostic: a handle
   of an async call that nothing awaits keeps S013, and a counted value
   keeps its release (§70.3 rule 2) as in an expression statement.
4. The places of §258 rules 3 and 4 do not change: a lambda body with
   no context and the operand of a `return` keep their checks.
5a. The flow analysis reads a statement `a && b;` as `if (a) b;`, and a
   statement `a || b;` as `if (!a) b;`. So
   `s === null && (s = "z"); print(s);` narrows `s` to `string`, as the
   `if` form does and as `tsc` does.
5. The order of evaluation and the short circuit do not change. No
   program that `tsc` rejects changes from rejected to accepted. The
   development tier, the ship tier, and the interpreter give the same
   output. The accept entry carries `js-comparable` and equals `node`.

### 300.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a459-a-statement-place-drops-each-value-leaf` holds each form of
   the table, each leaf prints when it runs, and the entry is rejected.
   The tracking note records the output.
2. After the change, `a459` passes `tsc`, the three tiers, and `node`.
3. A unit test pins rules 2 and 3: `5 && k();` keeps the diagnostic of
   the left operand, and `b && v();` with an async `v()` keeps S013. A
   control in the same test: `b && k();` is accepted.
4. No golden moves. If a corpus entry or a golden changes its type or
   output, report it and stop.
5. The full gate passes.

### 300.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review measured the scope of rule 1: a value place
keeps the common type and the literal context of its branches. It
measured release parity on the development tier: the statement form
and the `if` form have the same maximum RSS over 300,000 iterations for
an array, a string, a class, and a generator leaf.

1. MINOR, realistic, fixed: a statement `s === null && (s = "z");` did
   not narrow `s` after it, and `tsc` narrows it. Rule 5a now reads the
   statement as the `if` form.
2. MINOR, realistic, fixed: the test of the literal context had no
   control for a value place.

The second round added a test that ran `a459` on the three tiers. The
corpus sweeps already do that (core principle 15), so it was removed.

### 300.4 Open

None.
