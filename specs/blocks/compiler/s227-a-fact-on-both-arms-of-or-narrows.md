<!-- §227 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 227. A fact on both arms of `||` narrows

*(Added 2026-11-06.)* Origin: §219.4 item 2. The owner selected this
section on 2026-11-06.

Problem, measured at `cae86f9d` with `tsc` 5.9.2 and the debug CLI:

| Form, `a: string \| null` unless stated | subscript | `tsc` |
|---|---|---|
| `if (a === "x" \|\| a === "y") a.length` | S011 | accepts |
| `if (a !== "x" && a !== "y") return; a.length` | S011 | accepts |
| `if (!(a === "x" \|\| a === "y")) return; a.length` | S011 | accepts |
| three arms, `a === "x" \|\| a === "y" \|\| a === "z"` | S011 | accepts |
| `(a === "x" \|\| a === "y") ? a.length : 0` | S011 | accepts |
| `if (a !== null \|\| a === "y") a.length` | S011 | accepts |
| `a: C \| null`, `b: C`: `if (a === b \|\| a !== null) a.v` | S011 | accepts |
| `if (a === "x" \|\| b === "y") a.length` | S011 | TS18047 |
| `if (a === "x" \|\| a === null) a.length` | S011 | TS18047 |

`narrowing_paths` in `compiler/src/check/narrowing.rs` gives no fact
when `a || b` is true, and no fact when `a && b` is false. Each fact of
the checker states that its path is present, so a fact that both arms
give holds after either arm.

### 227.1 Rules

1. When `a || b` is true, a path has a fact if the true result of `a`
   gives it, and the true result of `b` gives it.
2. When `a && b` is false, a path has a fact if the false result of `a`
   gives it, and the false result of `b` gives it.
3. Two facts match if they have the same path and the same kind. The
   result is shared if either fact is shared, so a call ends it (§159,
   §162).
4. If `a` gives the result, `b` does not run, so the fact of `a` holds.
   A fact of `b` holds when `b` stops: a kill in `b` after its test ends
   it, as §124 states. For example, `a === "x" || (a === "y" && g())`
   gives no fact if `g` ends a shared `a`.
5. Rules 1 and 2 apply at each level. `!`, a nested `||` or `&&`, a
   conditional expression, `if`, `while`, and each other user of
   `narrowing_paths` read the result.
6. A fact on one arm only gives no narrowing. The last two rows of the
   table stay rejected with S011, as `tsc` rejects them.
7. No run-time cost. The rule changes the checker only.

### 227.2 Acceptance

1. Red first, at the pin: the accept entry `a416-or-narrows-on-both-arms`
   is rejected with S011. The tracking note records the output.
2. `a416` passes `tsc`. Its output under `node` equals its golden. It has
   each accepted row of the table, an `ElementCheck` fact on both arms,
   and a run of each arm.
3. `r484-or-fact-on-one-arm` rejects the form `a === "x" || b === "y"`
   with S011. Its header states the `tsc` result, measured.
4. A unit test pins rules 1 to 4. Controls: a fact on one arm gives no
   narrowing, two facts of different kind give no narrowing, and the
   example of rule 4 gives no fact.
5. If a fact of the checker does not state presence, report it and stop.
   Rule 3 then needs a contract change.
6. The rejection-site total check passes. §219.4 item 2 cites this
   section. No other golden moves.
