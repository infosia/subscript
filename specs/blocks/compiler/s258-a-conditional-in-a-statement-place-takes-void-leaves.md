<!-- §258 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 258. A conditional in a statement place takes `void` leaves

*(Added 2026-11-12.)* Origin: §239.4 item 1. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-12.

Problem, measured at `908c35b7` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `g(n: i32)` and `print` have the result
`void`, `k(): i32` has a value result, and `b` is a `boolean`. `tsc`
accepts each form, with no error. The checker gives S100 "a `void`
expression is only allowed as an expression statement" at the first
`void` leaf.

| Form | Place |
|---|---|
| `b && g(1);` | expression statement |
| `b \|\| g(2);` | expression statement |
| `h.cb !== null && h.cb(1);` | expression statement |
| `b ? g(1) : k();` | expression statement |
| `n > 1 ? g(1) : n > 0 ? g(2) : print("z");` | expression statement |
| `(n > 0 ? g(1) : g(2));` | expression statement |
| `const f = (n: i32) => n > 0 ? g(n) : print("e");` | lambda body, no context |
| `run((n: i32) => n > 0 && g(n))`, `run(cb: (n: i32) => void)` | lambda body, `void` context |

These forms run at the pin, as controls:

| Form | Output |
|---|---|
| `const f = (n: i32): void => n > 0 ? g(n) : print("e");` | the leaf that runs |
| `run((n: i32) => n > 0 ? g(n) : print("e"))`, `run(cb: (n: i32) => void)` | the leaf that runs |
| `return n > 0 ? g(n) : print("e");` in a function with the result `void` | the leaf that runs |

A place with the result `void` already checks each leaf of a
conditional as a statement (§237). An expression statement does not:
it checks the whole expression as a value, so a `void` leaf is a `void`
value (C21).

### 258.1 Rules

1. A statement place is an expression statement, and the expression
   body of a lambda with no result annotation whose context is a
   function type with the result `void` (§237 rule 4, §238 rule 2).
   `tsc` drops the value in both places. A parenthesized expression
   keeps the place.
2. In a statement place, each branch of a conditional `c ? a : b`, and
   the right operand of `a && b` and of `a || b`, is a statement place.
   A `void` leaf is legal there. If a `void` leaf is present, each value
   leaf is evaluated and its value is dropped, as in an expression
   statement.
3. The expression body of a lambda with no result annotation and no
   context has the result `void` if it is a conditional whose leaves
   are each `void` (§238 rule 1). A conditional with a `void` leaf and
   a value leaf, and `a && g()` with a `void` `g()`, stay rejected in
   that place: `tsc` gives the result `void | T` or `false | void`, and
   this language has no such type (C21).
4. The operand of a `return` in a function with the result `void`, and
   the expression body of a lambda with the annotation `void`, keep
   §237: each leaf must be `void`. `return b && g(1);` stays rejected:
   `tsc` gives TS2322 "Type 'false | void' is not assignable to type
   'void'".
5. The order of evaluation and the short circuit do not change.
6. No program that `tsc` rejects changes from rejected to accepted. The
   development tier, the ship tier, and the interpreter give the same
   output. The accept entry carries `js-comparable` and equals `node`.

### 258.2 Acceptance

1. Red first, at the pin: the accept entry
   `a442-a-conditional-in-a-statement-place-takes-void-leaves` with
   each form of the first table is rejected. The tracking note records
   the output.
2. After the change, `a442` passes `tsc`, the three tiers, and `node`.
   Each leaf that runs prints a line, and each leaf that does not run
   would print one.
3. A unit test pins rules 1 to 4: each form of the first table, the
   controls, `run((n: i32) => n > 0 && g(n))`, and the rejected forms
   of rules 3 and 4, each with its diagnostic.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.

### 258.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review checked about 75 forms against `tsc`, and
the checker rejects each form that `tsc` rejects. The development tier,
the ship tier, and `node` gave the same output for the order of
evaluation, the short circuit, an `await` leaf, and a `yield` leaf.

1. MINOR, realistic, fixed: rule 2 stated that a value leaf is dropped
   in each statement place. The checker drops it only beside a `void`
   leaf. Rule 2 now states that condition, and §258.4 item 1 records
   the forms.
2. MINOR, fixed: C21 had no revision for this section.
3. MINOR, recorded: §258.4 item 2.

### 258.4 Open

1. A statement with no `void` leaf keeps the checks of a value. `tsc`
   accepts each form. `b ? 1 : "s";` gives S100 "conditional branches
   have no common type". `b && 5;`, `b && k();`, and `b && (n = 3);`
   give S100 "logical operators require booleans, got `i32`".
2. These forms keep their rejection, and `tsc` accepts each:
   `(b && g(1)) || g(2);` (the left operand is not a statement place),
   `b ? c?.m() : g(1);` (S012), `gr((n) => n > 0 && g(n))` with a
   generic context, `const f = async (n: i32) => n > 0 && g(n);`, and
   `[b ? g(1) : g(2)];`.
