<!-- §307 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 307. Each divergence shows an accepted program, or states that none exists

*(Added 2026-11-15.)* Origin: an owner request of 2026-11-15. A form
that `tsc` accepts and this compiler rejects shows the form to write
instead, if one exists. The owner decided on 2026-11-15 that the
alternative stays in the `= subscript:` block of §79 rule 3, and that
this section is the first of five work units.

Problem, measured at `23e6f607` by a survey of each rejection site and
each `Divergence` entry (`specs/tracking/divergence-alternatives.md`).
512 variants exist; 5 of them no site emits. The `subscript` field of
an entry has two forms (§79 rule 1): a program fragment, which the §154
total check compiles, or the sentence "no equivalent; <what to do
instead>", which no check reads.

| `subscript` field | Variants |
|---|---|
| a program that the checker accepts | 170 |
| "no equivalent; …" with an instruction that names an accepted form | 202 |
| "no equivalent; …" where no form exists | 54 |
| "no equivalent; …" that is vague, shared by unrelated forms, or names the form of another variant | 84 live, 2 dead |

Examples of the last row, measured on the debug CLI:

- `2 ** 3` shows "no equivalent; use Math.pow, an explicit comparison,
  or a null check". The same sentence serves 13 variants.
  `Math.pow(2, 3)` is accepted.
- `E.A + 1` shows "no equivalent; convert to the integer or use a
  template", and the `why` "an assertion does not check membership".
  `(E.A as i32) + 1` is accepted.

### 307.1 Rules

1. §79 rule 1 changes. The `subscript` field of an entry is one of two
   forms:
   a. a program fragment that the checker accepts, with the intent of
      the `ts` fragment of the same entry;
   b. the sentence `no equivalent: <reason>`, only when no program that
      the checker accepts has that intent. The reason states why no form
      exists. It can name a step outside the source, such as "generate
      the mirror again from its C header".
   The form "no equivalent; <what to do instead>" is removed.
2. Each entry whose instruction names an accepted form takes the
   program of that form (form a). An entry whose text fits another
   variant takes a program of its own `ts` intent.
3. The `why` of an entry gives the reason of its own form. A `why` that
   more than one variant shares is true for each of them.
4. The 5 variants that no site emits are deleted:
   `AsyncReturnMissingArgument`, `AsyncReturnArgumentCount`,
   `ReferenceSearchMiss`, `GeneratorDoneValue`, `EntryParameterType`.
5. The §154 fragment check (`fragment_checker_failures` in
   `compiler/src/check/rejection_total.rs`) compiles each `subscript`
   field that does not start with `no equivalent: `. A field that starts
   with `no equivalent; ` fails the check. So an instruction in prose
   cannot pass the check unless it is stated as a form b entry.
5a. The fragment check holds the list of the variants whose field is
   form b. A variant that is form b and not in the list, or in the list
   and not form b, fails the check. So each new form b entry changes a
   list that a review reads. Whether a form a program has the intent of
   its `ts` fragment stays a review item. *(Added 2026-11-15 after the
   Phase Review, which found two form a programs of another intent and
   three form b entries where a form exists.)*
6. The rendered block of §79 rule 3 does not change its layout. A `ts`
   or `subscript` fragment does not start with a line break, so the
   block has no empty first line.

### 307.2 Acceptance

1. At the contract pin, a count of the `subscript` fields by form, from
   the table: the tracking note records it.
2. After the change, each entry is form a or form b. The tracking note
   lists each form b entry with its reason, so that the Phase Review can
   read each one.
3. The §154 total test passes. A unit test builds an entry with a
   `no equivalent; ` field and shows that the check fails, and a form b
   entry passes.
4. Pinned texts that move (`= subscript:` lines in reject expectations
   and tests) are updated. No message line and no position moves.
5. The cost of the fragment check is measured before and after, with
   `--test-threads=1`, and stated in the test.

### 307.3 Phase Review

### 307.4 Open
