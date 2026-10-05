<!-- §163 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 163. An indexed read is not narrowed

*(Added 2026-10-05.)* Origin: `compiler.md` §159.4 item 4 (an indexed
target). The owner decided on 2026-10-05 not to narrow an indexed read,
and to make the diagnostic show the form that narrows.

Problem: a null check of an indexed read does not narrow it, and the
S011 message tells the author to "narrow with a null check first". For
an indexed read that advice is wrong: the author wrote the null check.
The S011 site is `TscRejects`, but `tsc` accepts some of these programs.
Measured at `4286e511` with `tsc` 5.9.2 (`xs: (A | null)[]`):

| Program | `tsc` | This checker |
|---|---|---|
| `if (xs[0] !== null) { xs[0].x }` | accepts | S011 |
| `if ((xs[0] = mk()) !== null) { xs[0].x }` | accepts | S011 |
| `const i = 0; if (xs[i] !== null) { xs[i].x }` | accepts | S011 |
| `let i = 0;` (not assigned again), the same test | accepts | S011 |
| `for (let i = 0; …; i++) { if (xs[i] !== null) { xs[i].x } }` | TS2531 | S011 |
| `if (xs[0 + 0] !== null) { xs[0 + 0].x }` | TS2531 | S011 |
| `if (xs[0] !== null) { xs[1].x }` | TS2531 | S011 |
| `if (xs[0] !== null) { xs[0] = null; xs[0].x }` | TS2531 | S011 |
| `if (xs[0] !== null) { xs[j] = null; xs[0].x }` | accepts | S011 |
| `if (xs[0] !== null) { touch(); xs[0].x }` | accepts | S011 |
| `const v = xs[i]; if (v !== null) { v.x }` | accepts | accepts |

Reason for the decision. An array element is a shared location: another
name can hold the array and change the element (C17), so a narrowing of
an element ends at every call and every element store. `tsc` narrows an
element only for a literal key or a key that does not change, so the
common loop key (`xs[i]` with `i++`) is rejected by `tsc` too. A `const`
copy of the element narrows for every key, keeps its narrowing across
calls (§124 rule 4), and reads the element once. Narrowing an element
needs the C17 ends and the §124 rule 3d runtime check on every tier, for
a form that the copy already covers.

### 163.1 Rules

1. A null check of an indexed read (`e[k] !== null`, `e[k] === null`, and
   their negation and assignment forms) does not narrow the indexed
   read. `collisions.md` C24 row 32 states the reason.
2. A diagnostic of §159 rule 4 (S011 for a member read, S100 for a call
   of a nullable function value, S005 for a nullable argument or store)
   whose nullable value is an indexed read states the copy form in its
   message, not "narrow with a null check first":
   `` `A | null` may be null here; copy the element to a `const` local
   and test the local `` (with the type of the program). Its note shows
   the form: `const v = xs[i]; if (v !== null) { v.x }`.
3. The site splits on one fact (§154 rule 2): a null check of the same
   indexed path holds at the read. The path is the same when the
   receiver path is the same and the keys have the same identity, as
   `tsc` names them: an integer literal, and a `const` local whose
   initializer is an integer literal (or a `const` local initialized
   with such a local), have the identity of that integer value; every
   other `const` local has the identity of its declaration. So `xs[i]`
   with `const i = 0` and `xs[0]` are one path. The fact follows the flow of §124 and §162,
   but only these end it: a store to the receiver path, a store to the
   same indexed path, and a join where a path into it lacks the fact. A
   call and a store through another key do not end it (as in `tsc`).
   - When the fact holds, the site is `Diverges` (C24 row 32), with the
     divergence block.
   - Otherwise, the site is `TscRejects`.
4. No program changes from accepted to rejected or the reverse, and no
   diagnostic changes its rule code. Only the message, the note, and the
   class of the site change.

### 163.2 Residual policy

The fact of rule 3 approximates the `tsc` element narrowing. These get
no fact here, and `tsc` narrows them, so the site claims `TscRejects`
where `tsc` accepts: a `let` key or a parameter key that is not
assigned again, and a non-null store to the element
(`xs[0] = new A(); xs[0].x`). Such a case is a missing
fact: it is fixed when it is found and does not block a phase. A site
that claims `Diverges` where `tsc` rejects is MAJOR.

### 163.3 Acceptance

1. Red first: reject entries `r373` (a literal key after a null check;
   `tsc: accepts`, the C24 row 32 divergence) and `r374` (an indexed
   read with no null check; `tsc: rejects` with its measured code). At
   the contract pin, record each diagnostic: the message and the class
   differ from rules 2 and 3.
2. Unit tests for rule 3, each with a same-shape control that gives the
   other class: a literal key and a `const` key (`Diverges`); a different
   key, a `for` loop key, a store to the same indexed path, and a store
   to the receiver (`TscRejects`); a call and a store through another key
   between the check and the read (`Diverges`).
3. The §154 total test passes, and C24 row 32 has its divergence text in
   `compiler/src/divergence/`.
4. Every existing corpus entry keeps its accept or reject result. A
   reject entry whose expected text names the old message moves to the
   new message; list each one in the tracking note.
5. No existing `.expected` golden moves.

### 163.4 Open

1. A store that the program already rejects for another reason does not
   end the fact: a destructuring assignment `[xs[0]] = [null]` (S100,
   §107.3) and an enum-member key `xs[E.A] = null` (S100). The follow-on
   S011 claims `Diverges` where `tsc` gives TS2531. Contrived: the
   program is rejected in any case.
