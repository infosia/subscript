# Backlog: open items in priority order

Status: triage at `2551f81a` (2026-11-17), from the Open lists of the
contract sections, `specs/tracking/`, and `specs/blocks/collisions.md`.
The triage read the repository. It did not run the CLI, so re-measure
an item before a section starts it.

Rank follows invariant 6: an item ranks high only if a typical program
or a typical host header reaches it. The list leaves out each item that
the owner put on hold or rejected (see the end).

When a section closes an item, delete the item here in the
implementation commit of that section.

## 1. Wrong output, a stopped process, or a failed build on accepted code

3. A C cast from an embedded-header link to its extension reads wrong
   values (`10000`, C gives `10003`) or reads past the scratch copy
   (§187.3 item 13). Only headers that use inheritance by first member.
   Medium.
5. A struct with a `FixedArray` of structs that the call rebuilds is
   rejected only at code generation (§187.3 item 6). Small.
6. Invalid UTF-8 stays at a string-view parameter, a string field
   written through a pointer, and `subscript_rt_str_from_view`
   (§184.4 item 2). Effect not measured. Medium.

## 2. Forms that `tsc` accepts and subscript rejects (and the reverse)

7. A named function with fewer parameters at a C callback place
   (§274.4 item 1). Small to medium.
8. S013 false rejections: a `yield` of a handle, a parameter that is
   only read (`r417`), a method receiver (`(await many()).slice(0)`)
   (§188.4 items 1, 5, 6). Medium.
9. Numeric literal types: `const MODE = 1; MODE === 2` and
   `readonly size = 4;` with `this.size = n` are accepted; `tsc`
   rejects them (TS2367, TS2322) (§156.4). Medium to large.
10. A for-of binding or a `const` element copy that a lambda captures is
    rejected, with a false message (§175.4 item 2). Medium.
11. An indirect call follows every function value made so far, so an
    unrelated lambda causes a rejection (§137 Open). Medium.
12. A method that moves a fresh local array of the same element type
    (`out.push(this)`) (§221.4 item 3). Small to medium.
13. Variance gaps: `Array.from<F>`, spread, `Set` source, a `then`
    callback with a `C | null` parameter, generic inference from a
    callback, an interface method (§235.4 items 2, 3). Medium.
14. Small gaps: `Worker.spawn(work)` with a value result (§240.4 item
    6); `new Set([1, zero()])` (§212.4 item 6); `new Set<i32>([])`
    (§103.5 item 7); `const b = (a = new A())` (§159.4 item 1); a
    literal-alias path and an encoding in a variable (§185.4 item 1);
    `normalize` with a template literal (`text-module-measure.md` §9.7
    item 2); §258.4 item 2.
15. C24 rows with no decided lowering (§154.4 items 4–13): `x!`,
    `**`, labels, string `<`/`>`, `yield*`, type-parameter defaults,
    local class or enum, generic aliases, `typeof`, and others. Each
    needs a design decision.
16. Not selected yet: an async arrow that captures `let` through cells
    (C24 row 36); `map(async …)`; the `matchAll` fusion (§103.5 item
    2). Large.
17. Development tier: the first use of a new generic instance refuses
    the hot-reload swap (§119.4 item 2). Medium.

## 3. Gate and test infrastructure

18. Checks that cannot fail, or missing firing tests: §168.4 item 1,
    §169.4 items 3, 4, 10, §172.4 item 2, §174.4 item 3, §176.4 item
    3, §268.4 item 2, §270.4 item 1, §323.4 item 3.
19. Test cost not stated, or work done twice (core principle 15):
    §150.4 items 1, 2, §168.4 item 2, §169.4 item 7, §174.4 items 1,
    2, §302.4 item 1.
20. Platform runs not recorded: §298.4 item 1 (`cl`), §280.4 item 3
    (Windows), §226.4 items 1–3 (x86-64), §225.4 item 1, C4029 (§183).
21. `a355` has no `node` run (§185.4 item 2); `jit-runner-fork.md`
    Open MINOR items.

## 4. Diagnostic messages

22. `bind` drops a `static const` whose type is not a flag alias. A use
    gives S016.
23. §157.4 item 1, §166.4 item 1 (`Promise.race`), §167.4 items 1, 3,
    the S009 async-arrow example, §149.4 item 2, §186.4 items 1, 5,
    §321.4 item 1, §312.4 item 2, `text-module-measure.md` §9.7 item 1.
24. §143.4 item 2, §153.4 item 2, §170.4 items 3, 4, §174.4 item 4,
    §291.4 item 1, §303.4 items 1, 3, §313.4 item 1.

## 5. Measured cost

25. An adopting async return expands its epilogue at each exit (about
    138 lines of C per value exit; §236.4 item 3).
26. Extra retain and release pairs, 19–21 ns per call (§255.4 items
    1–3, §256.4 item 1, §314.4 item 1).
27. Checker cost 1.04–1.06 of the pin (§160.4 items 1, 2).
28. The a22 x86-64 ship ratio at 2.18; §323.4 item 1; §269.4 item 1;
    §169.4 item 9.

## 6. Contrived or MINOR (about 70)

- Narrowing and flow: §155, §158, §163, §164, §315–§318.
- Inference and cycles: §149, §156, §290, §306, §308, §312.
- Async and counted values: §166–§172, §176, §295, §323.4 item 2.
- Interop and runtime: §179, §187.3 items 3, 5, 7, 9–12, §259, §261,
  §263, §265, §267, §270, §271, `s178-host-completion-measurement.md`.
- Code hygiene: §174.4 items 1, 6, §166.4 item 3, §220.4 item 3,
  §321.4 item 2, callback arity in `cemit/collection.rs`;
  `runtime/src/arrops.rs` is past 2,000 lines.
- Tutorial: §323.4 item 4.

## 7. Process and tooling

29. No totality check of the `compiler.md` §0 index; nothing detects a
    superseded section; no gate reads `docs/`.
30. Context-size countermeasures 3, 4, 6, 7
    (`context-size-2026-09-12.md`) are undecided.

## Left out: owner hold or rejection

- Rare `tsc` forms on hold: §240.4 items 3, 7, §244.4 item 1, §245.4
  item 1, §233.4 item 3.
- Environment function value adapter forms B and C (form A chosen).
- Host async: `_Nullable` handle return, per-root wake (on a downstream
  request only).
- Host API review items 1, 3, 4 (until a host states the need).
- The memory safety study (stopped).
- Legacy string method renames.
- A `Context.free` at a wrong point (Q6, by design).
- Decided or withdrawn: §252.4 item 1, §301, §289.4 items 1, 2,
  §236.4 item 2, §238.4 item 3, §154.4 item 1, §232.4 item 1, §212.4
  item 7, §215.4 item 4, §221.4 item 5, §103.5 item 1, collisions.md
  §3.
