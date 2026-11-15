<!-- §303 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 303. Diagnostic text fixes from open lists

*(Added 2026-11-15.)* Origin: the open lists of §164, §186, §241, §244,
and §284. On 2026-11-15 the owner told Claude to close the open items
that a small change closes first.

Each item below is a diagnostic, a divergence block, or a comment that
states a wrong or repeated fact. No program changes from accepted to
rejected, or from rejected to accepted. Only the text and the count of
diagnostics change.

### 303.1 Rules

1. §164.4 item 4: the note and the divergence `why` of the site of
   §164 rule 6 do not state the same fact. The `why` keeps the reason
   (a function type has no optional parameter, C7); the note states what
   to write. This amends the note text of §164 rule 6.
2. §186.4 item 2: `then(f, null)` gives a message and an example that
   show `then(f)`, the accepted form, not `catch(r)`.
3. §186.4 item 3: the divergence block of `PromiseReactionSpread` shows
   a reaction form (`then`, `catch`, or `finally` with a spread
   argument), not the spread example of §14.4.
4. §186.4 item 4: the field comment in `compiler/src/check/mod.rs` that
   cites §186 rule 6 for the literal hint cites rule 4.
5. §244.4 item 3: the reason of `CallbackParameterShape` is true for
   the parameter it names. "A parameter for the container itself is not
   passed" stays for a parameter at the position where JavaScript passes
   the container (the third parameter of `Map.forEach` and of
   `Set.forEach`, and the position of the array in an array callback),
   and `Set.forEach` with two parameters keeps its own reason. `tsc`
   accepts these forms, so they stay divergences.
5a. A callback parameter past the parameters of the callback type that
   the prelude declares (a third parameter of the `sort` comparator, a
   fourth of `map`, a fifth of `reduce`, a fourth of `Map.forEach`, a
   second of `Map.groupBy`) is a form that `tsc` rejects too (TS2345;
   TS2769 for `reduce`; measured). It takes a site of its own, of the class that `tsc`
   rejects, with no "TypeScript accepts" block. Its message names the
   number of parameters that the callback type declares. A site of its
   own keeps one example for each site (§182 rule 2).
6. §244.4 item 4: a conditional diagnostic names the declared type of
   each leaf, not the type with padded parameters: `one` is
   `(i32) => void`.
7. §284.4 item 5: a warning names the callee of a self-call by its name,
   not `…`.
8. §241.4 item 1: a read or a write of a static field before its class
   gives one S100 at the site: the diagnostic of §241 rule 1. The C14
   diagnostic does not repeat it at the same site. The decision compares
   the site, not the text of a message. `tsc` gives one TS2449.

### 303.2 Acceptance

1. A unit test pins each rule with the full text of the diagnostic, the
   block, or the warning. Each test fails before its change. For rule
   4, the comment change is the evidence. A comment and a block cite the
   rule that the code keeps (for example §186 rule 4, C8, or Q27), not
   this section.
2. The rejection-site total check and the generic `tsc` matrix pass. If
   a witness text or a recorded block changes, the round reports it.
3. No corpus golden changes. If the expected diagnostics of a reject
   entry change, the round reports each one.
4. The full gate passes.

### 303.3 Phase Review

One fresh review of the implementation, by measurement with the CLI,
`tsc`, and `node`. No CRITICAL finding.

1. MAJOR, realistic, fixed: the new reason of rule 5 said that
   `Map.groupBy` passes no value at its second callback parameter;
   `node` passes the index there.
2. MAJOR, realistic, fixed: the block of an extra callback parameter
   said "TypeScript accepts:" over a `sort` call that `tsc` rejects
   (TS2345), and the site was a divergence for each such form. Rule 5a
   now gives these forms the site `CallbackExcessParameters` (S014),
   of the class that `tsc` rejects, with no block. Its message is
   "`map` callback type declares 3 parameter(s); got 4". The witness
   `s038` moved to it.
3. MINOR, fixed: a comment cited this section, not §186 rule 4.
4. MINOR, fixed: the `collision:` fields of four new blocks cited this
   section, not the rule that rejects the form.
5. MINOR, contrived, recorded: §303.4 item 1.
6. MINOR, contrived, recorded: §303.4 item 2.
7. MINOR, fixed: rule 8 compared the text of a message; it compares
   the site now.
8. MINOR, fixed: test costs, repeated assertions, unpinned messages,
   and a control for `then(null, r)`.
9. MINOR, fixed: rule 1 amends the note of §164 rule 6, and rule 5a
   keeps one example for each site (§182 rule 2).

The first full gate failed in two integration tests that pinned the
texts before rules 1 and 5a: the `sort` message of
`built_in_callback_fewer_parameters.rs`, and the C7 reason in the note
in `exit_predicate_and_arguments.rs`. Each now pins the new text; the
second reads the reason in the `why`.

### 303.4 Open

1. A nested conditional, `[1].forEach(b ? one : (c ? one : three))`,
   still names the padded type of a leaf.
2. A static field read through an immediately called lambda before its
   class, `const s: i32 = ((): i32 => B.z)();`, gives the S100 of §241
   rule 1 and the C14 diagnostic "through an indirect call".
3. The message of rule 5a for `sort` no longer cites Q22; the message
   before §303 did.
