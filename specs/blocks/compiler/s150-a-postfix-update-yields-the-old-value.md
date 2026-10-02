<!-- §150 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 150. A postfix update yields the old value

*(Added 2026-10-02.)* Origin: the §149 Phase Review. A defect, not a
surface change: the language already accepts every form below.

Problem: a postfix `x++` or `x--` used as a value yields the updated
value; JavaScript yields the value before the update. The checker never
reads whether an update is prefix or postfix. Both tiers agree, so the
differential gate cannot see it (core principle 12). Measured at
`9e18bb3e` on the dev tier, against `node`:

| Program | dev | `node` |
|---|---|---|
| `let k = 0; const a = k++;` print `a k` | `1 1` | `0 1` |
| `let j = 5; const b = j--;` | `4 4` | `5 4` |
| `const c = o.x++;` (field) | `1 1` | `0 1` |
| `const e = xs[0]++;` with `xs = [10]` | `11 11` | `10 11` |
| `let i = 0; let n = 0; while (i++ < 3) { n = n + 1; }` print `n i` | `2 3` | `3 4` |
| `let q: f64 = 1.5; const g = q++;` | `2.5 2.5` | `1.5 2.5` |
| `const f = ++p;` (prefix, control) | `1 1` | `1 1` |

`tsc` 5.9.2 accepts every program.

### 150.1 Rules

1. A postfix update `x++` or `x--` used as a value yields the value of
   `x` before the update; the update happens once. A prefix update
   yields the value after it. A statement-level update (whose value is
   unused) is unchanged.
2. The target is evaluated once: a field target's object, an index
   target's array and index, and a namespace-qualified or captured
   target each evaluate once, before the read, as they do for the
   prefix form.
3. The numeric rules of the update do not change: the width, the wrap
   or trap of the target type, and C3's rules for `f64` and the sized
   integers.
4. Every place that lowers an update follows rules 1–3 on the dev tier,
   the ship tier, and the reference interpreter: one form in the HIR
   carries prefix or postfix to every consumer.

### 150.2 Acceptance

1. Red first: an accept entry, `js-comparable: yes`, with each row of
   the Problem table and a postfix update on a module global, a field of
   a captured `const` local in a lambda (C5 rejects an update of a
   captured `let`, S009), a `u8` at its maximum (wrap), and an `i64`. Its
   golden is the `node` output; at the contract pin, the dev tier
   output differs from it (record both).
2. A side-effect witness shows that an index target with a side-effecting
   index expression evaluates the index once.
3. Unit tests in the checker and in each lowering for prefix and postfix
   in value position.
4. No existing `.expected` golden moves, unless an existing entry uses a
   postfix update in value position; each such golden that moves is
   listed with its old and new line and the `node` output that confirms
   the new line.

### 150.3 Open

These items are MINOR under CLAUDE.md invariant 6 (gate cost only).

1. `codegen/src/postfix_update_tests.rs` `postfix_corpus_matches_all_three_forms`
   runs `a326` again on all three forms; the golden and interpreter
   corpus tests already run it (measured 0.35 s per gate profile).
2. The same file runs the C compiler and states no cost (measured
   0.49 s for its four tests).
