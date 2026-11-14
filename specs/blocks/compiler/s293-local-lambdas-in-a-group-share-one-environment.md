<!-- §293 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 293. Local lambdas in a group share one environment

*(Added 2026-11-14.)* Origin: §284.4 items 1 and 2. On 2026-11-14 the
owner selected the form of this section: the lambdas of a group share
one environment, a call of a member is a direct call with that
environment, and a read of a member as a value makes a new function
value of the same code and the same environment.

Problem, measured at `7a0ddf2f` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. Each form is in a function body.

| Form | subscript | `tsc` |
|---|---|---|
| `const first = (n: i32): void => { if (n > 0) second(n - 1); }; const second = (n: i32): void => { first(n); }; first(1);` (`r510`) | S100 "`second` is read before its declaration in this block" | accepts |
| `const local = (n: i32): void => { const other = local; other(n); }; local(0);` (`r509`) | S100 "`local` is read before its declaration in this block" | accepts |

§284 accepts a call of a local lambda by its own name, because the call
needs no capture of the name: it calls the running closure. A call of
another lambda of the same block, declared later, needs the value of
that lambda in the environment of the first, and the second lambda
needs the first: a cycle of two environments. A read of the lambda's
own name as a value needs the running closure as a value.

### 293.1 Rules

1. A *run* is a sequence of one or more `const` declarations in one
   block, with no other statement between them, where each initializer
   is a lambda literal and each `const` has no type annotation. In the
   body of a lambda of a run, a call or a read of the name of a lambda
   of the run that is declared after it links the two lambdas. A read
   of a lambda declared before it is a capture of a `const`, as at the
   pin, and makes no link. A *lambda group* is a set of lambdas of a
   run that links connect, and a lambda whose body reads its own name
   as a value is a group of one. In a member body, a call or a read of
   the name of a member declared before it is a group call or a group
   value (rules 2 and 3). A member is a lambda of a
   group. A lambda of a run that is in no group keeps the lowering that
   it had at the pin, and a lambda whose body only calls itself keeps
   §284. *(Amended 2026-11-15: the first round of the coding agent
   measured that a group of each run changes the lowering of `a149`,
   `a342`, `a432`, and `a449`, and that an async lambda made the
   environment of a lambda next to it owned, with no link in the
   source. That is a hidden cost (core principle 16). The second round
   measured that a link by a read of an earlier lambda changes `a149`
   and `a342`.)*
2. In the body of a member, a call whose callee is the name of a
   member is a *group call*, also before the declaration of that
   member. A group call calls the code of that member with the
   environment of the group. A self-call of §284 is a group call.
3. In the body of a member, a read of the name of a member that is
   not the callee of a call is a *group value*. It makes a function
   value with the code of that member and the environment of the
   group. A group value has the type of the member. If the environment
   of the group is the stack environment of C5, the group value copies
   the capture values into a stack environment of the same layout. The
   captures are `const` locals, so the copy holds the same values.
   *(Amended 2026-11-15: the LIR has no operand for the running stack
   environment, measured by the first round of the coding agent.)*
4. The members of a group share one environment: one field for each
   `const` that a member captures (§181 rules 1 to 3; §291 rule 1 for
   a default). A member does not capture the name of a member. A
   group with no capture has the null environment.
5. The environment of a group is owned (§274 rule 1) if a value of a
   member can leave the function, or if a group value can leave the
   function, or if a member is async (§181 rule 2). Otherwise it is
   the stack environment of C5. A member value made outside the group
   (a read of the name after the group) holds the same environment.
6. The type of a member is decided before a body of the group is
   checked: a result annotation, §238 rule 1, or §286. A member whose
   result needs its body and that a group call or a group value of
   another member reads gives the S100 of §290 rule 1. A cycle of
   expression bodies stays S100 (§156 rule 3; `tsc` gives TS7023).
7. These forms stay rejected with the present site and diagnostic:
   a group call or a group value in a lambda that is nested in a
   member (§284 rule 5); a read of a member's name before its
   declaration outside a body of the group; two lambda `const`
   declarations with another statement between them, where the first
   calls the second. They are not one group, so the read of the second
   before its declaration stays S100 (C14).
8. A member with an environment is a capturing lambda for C5 and
   §274 rule 5: it never reaches a C callback.
9. Hot reload follows §181 rule 7: a member value keeps the body and
   the environment layout of the module that created it.
10. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 293.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a457-local-lambdas-in-a-group-share-one-environment` is rejected
   with the S100 of the table. The tracking note records the output.
2. `a457` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds: `isEven` and `isOdd` as a
   group with a capture; the two rows of the table; a group value
   pushed into an array and called after the group; a group returned
   from its function and called by the caller (an owned environment);
   a group of three members; an async member that awaits a group call;
   a member with a counted capture (a string and a class reference).
3. `r509` and `r510` change to forms that rule 7 rejects: a group call
   in a nested lambda (`r509-group-call-in-a-nested-lambda`), and a
   statement between two members.
4. A unit test pins: the lowering of a group that does not escape has
   no allocation (core principle 9: a control in the same shape that
   escapes allocates); the ownership of a counted capture of an owned
   group environment over 50 iterations, with no growth.
5. The full gate passes. `a457` has an async entry, so the LIR text
   snapshot grows by its functions. No other golden moves; the
   sections of `a149`, `a342`, `a432`, and `a449` stay the same.

### 293.3 Phase Review

One fresh review. No CRITICAL finding. Rules 1 to 6, 8, and 9 agreed on
the three tiers and `node`: a run with a group and a lambda in no
group, a chain of three, two groups in one run, group values in an
array, a `Map`, a class field, and a return, a default capture, an
async member, and 50 iterations with `Context.collect()` with no
growth. A run with no group has the LIR of the pin.

1. MAJOR, realistic, fixed: a parameter default of a lambda that calls
   or reads a lambda declared after it was accepted, and the lowering
   stopped with an internal error. Rule 7 keeps the S100 of the pin: a
   default is not a body of the group.
2. MAJOR, realistic, fixed: a lambda that captures another lambda whose
   omitted default reads a capture (§291 rule 1), when that lambda owns
   its environment (§274), stopped with "call target identity or
   signature is invalid". The verifier did not follow a closure that
   the caller loads from a field of an owned environment. The defect
   came from §291, not from this section.
3. MINOR, realistic, fixed: a test cost line stated the wrong count.
4. MINOR, realistic, fixed: the name of `r509` did not match its
   content. It is `r509-group-call-in-a-nested-lambda`.

### 293.4 Open

1. Function values have no equality (S100 `ErasedAssignableEquality`),
   so the identity of a group value is not observable. A group value of
   a stack group is a copy (rule 3).
