<!-- §106 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 106. The reference interpreter stores a generator

Origin: §103.5's `a217` item. The language accepts a `Generator<T>`
in storage, both production tiers run it, and the reference
interpreter answers "expected runtime handle, found Coroutine". Core
principle 12 makes the interpreter the witness no tier shares, so an
exclusion that hides a whole value kind costs more than the repair.

### 106.1 The defect is wider than `a217` records

`a217`'s header says the interpreter "cannot pack it into a class
field". Measured 2026-09-10: `pack_into` is the **general** store
path. `codegen/src/interpreter.rs` calls it for an array element at
one site, for a field or offset at another, and for a pointer store at
two more. `Type::Generator(_)` sits in its handle arm, and
`as_handle()` cannot answer for a `Value::Coroutine`.

So every packed location fails, not one. `a217`'s reason is narrower
than the defect, and the round corrects it.

### 106.2 What the language surface requires

Measured 2026-09-10 at `3d03f80` on the dev tier. These are the facts
the interpreter must reproduce; none of them is new.

1. **A stored generator is the same generator.** A local and a field
   that name one generator share its progress:

       const g = upTo(4); const h = new Holder(g);
       g.next() → 1;  h.g.next() → 2;  g.next() → 3

   Identity, not a copy of the frame.
2. **Replacing a field replaces the generator.** The new one starts at
   its own beginning, and the old one is unaffected.
3. **A generator is storable in an array, and as a `Map` value**, not
   only in a class field. `Map<i32, Generator<i32>>` checks clean, and
   each of the three is a distinct packed location.
4. **`Generator<T> | null` is rejected**, S011 "unions are limited to
   `Ref | null`". A program cannot declare nullable generator storage,
   so §106.3 rule 4's null case is unreachable from a corpus program.
   That is why §106.7 routes it to a unit test.

   A consequence, measured: `Map<K, Generator<V>>.get(key)` is
   rejected, correctly, because its result would need that union.
   **Its stated reason does not fit the form** — it says "A scalar
   value type has no null miss value", and a `Generator<V>` is not a
   scalar. `getOr` is the total spelling and it **works**:
   `m.getOr(1, one()).next().value` runs on both tiers. This is §103's
   class in a message §103 did not reach. Recorded, open, and not
   part of §106.
5. **`g1 === g2` is rejected at the language surface**, S100 "operator
   not defined". So the interpreter's `as_handle()` equality arm for a
   generator is unreachable from a program. **No rule decides
   generator equality here**, and a later round must not read one into
   this section.

### 106.3 The rule

1. **A generator registers at creation**, in a registry of its own,
   keyed as the async registry keys a handle.
2. **`Type::Generator(_)` gains its own pack and unpack arms**, which
   write and read that key. They do not fall into the handle arm.
3. **Unpack restores the same coroutine**, so §106.2 rule 1 holds
   through every packed location.
4. A zero key unpacks to null and a null packs to a zero key, as the
   handle arm already did through `as_handle`. An unknown key is an
   invalid-LIR error, as the async arm gives. *(The pack direction was
   added 2026-09-10: the rule named only unpack, and a test pinned
   both.)*
5. **The registry retains a generator until the interpreter tears
   down.** This is an interpreter resource choice, not a language
   rule, and it is stated because the alternative is unsound: a
   generator reachable only through packed bytes has no live `Rc`,
   so a reference-count sweep would free a frame a valid field still
   names.
6. **The async owner-count path is not reused.** §70's count governs
   an async handle, a generator's is zero at creation, and copying
   the release path would remove an entry a field still names.

### 106.4 What the round measures before it closes

Rule 5 trades memory for correctness, and one thing can observe it:
a program that reads `subscript_rt_ctx_live_allocations` or calls
`Context.collect()` around a generator. **The round measures whether
any accept entry does**, and reports the result. If one does, this
section gains a release path, and the round reports that instead of
inventing one.

**Measured 2026-09-10: none does.** `subscript_rt_ctx_live_allocations`
has no language-surface spelling, and no entry that names
`Generator<` calls `Context.collect()`.

**The cost is measured and open.** The registry retains about 1.3 to
1.5 KiB for **every generator ever created**: 200,000 generators hold
290 MiB against 43 MiB for the same program with none, and 1,000,000
hold 1,498 MiB. Rule 1 registers at **creation**, and rule 5's
soundness argument needs only generators that were **packed** — a
`for (const v of gen())` is retained and can never be unpacked.
**Registering at pack time bounds the cost and keeps the argument.**
Nothing observes the retention today, and the 168-entry sweep is green
in 204 s. Recorded as open; it constrains what a generator-heavy entry
can be.

### 106.5 Found by the repaired witness

*(Added 2026-09-10 by §106's Phase Review, which measured it.)* This
section exists because an exclusion that hides a whole value kind
costs more than the repair. The repaired witness found a defect on its
first round, and the record belongs here.

**A re-entrant `.next()` on a stored generator kills both production
tiers with a signal.** A `tsc`-clean program the checker accepts
reaches itself through storage: a generator holds a reference to its
own handle and steps it.

| form | measured 2026-09-10 |
|---|---|
| reference interpreter | prints `a 1`, then `invalid LIR: coroutine frame is already executing` |
| dev tier | prints nothing, `program terminated abnormally (dev-JIT child signal 4)` |
| ship tier | prints `a 1`, then exits 1 |
| node v24.18.0 | prints `a 1`, then `TypeError: Generator is already running` |

The emitted C carries no re-entry guard: `frame->state` is a
resume-point index, so the second entry re-enters one frame and
overwrites its locals. node raises a **catchable** error; this
language crashes.

**At `19fa6fe` no witness could see it**, because the interpreter
answered "expected runtime handle, found Coroutine" for the same
program. The repair exposed it.

Three facts belong with the record. The divergence has no id —
`collisions.md` names no re-entrancy rule. The interpreter reports
"invalid LIR" for valid LIR, which is §103's class. And the two tiers
disagree with each other in how they die.

**This section does not fix it.** A re-entry guard is a change to both
tiers and is a section of its own. §106 records the measurement.

### 106.6 Sites

- `codegen/src/interpreter.rs`: the registry, the two `Generator`
  arms, and the creation site that registers only async functions.
- `corpus/accept/a217-generator-in-a-class-field.ts`: the exclusion
  header goes when all three forms agree.
- `specs/blocks/compiler.md` §103.5, the `a217` item.

### 106.7 Corpus and gate (pre-registered exit criteria)

**Accept.** `a217` loses `// interpreter: no`. New entries pin what
§106.2 states and no entry pins today: two references to one
generator, alternating `.next()` through both; two distinct
generators, each keeping its own suspended state; a field replaced
after partial consumption; a generator in an **array** element,
driven from the array; a generator as a **`Map` value**, driven from
the map; and an exhausted generator read again from storage.

**Gate.**

1. All three execution forms — dev tier, ship tier, reference
   interpreter — match each entry's golden byte for byte, `a217`
   included.
2. `a216` keeps its three witnesses throughout.
3. Direct pack and unpack unit tests cover identity preservation, a
   zero key, an unknown key, and **the registry's drain**, because no
   corpus program can express any of the last three. *(The drain was
   added 2026-09-10: deleting rule 5's branch failed no test, because
   the map drops with the interpreter anyway and the branch matters
   only for a cycle through a generator frame's own locals.)*
4. The async lifecycle tests run, because the adjacent machinery
   shares the coroutine value.
5. The interpreter sweep reports the exclusion count, and the round
   states it before and after.
6. The round reports which goldens and counted totals moved, and why
   (§103.8 rule 2). *(An earlier draft of this rule added "every new
   entry declares a generator, so `corpus.txt` moves". That predicts
   the answer, which rule 2 forbids, in the section written right
   after the rule. The round reads the selection test and reports
   what it measured.)*
7. `tools/gate.sh full` green in both profiles.
