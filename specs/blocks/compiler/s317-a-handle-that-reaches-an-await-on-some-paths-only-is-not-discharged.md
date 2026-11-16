<!-- §317 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 317. A handle that reaches an await on some paths only is not discharged

*(Added 2026-11-16.)* Origin: §257.4 item 2. The owner selected it in
the open-item triage on 2026-11-16.

Problem, measured at `9c015c24`. §188 rule 3 discharges every origin
that a value carries when the value is awaited. A value that comes from
a conditional carries the origins of each branch, but at run time it
holds one of them. A default that reads an earlier parameter
discharges that parameter (§257 rule 1), but the default runs only when
the call omits the argument. The checker accepts each of these forms
with no S013:

```ts
async function f(p: Promise<i32>, q: Promise<i32> = p): Promise<i32> { return await q; }
async function g(c: boolean): Promise<i32> { const p = v(1); const q = v(2); const r = c ? p : q; return await r; }
async function h(c: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { return await (c ? p : q); }
```

If a call of `f` passes `q`, nothing awaits `p`. In `g` and `h`, one
handle is not awaited. If that handle rejects, the program traps with
`uncaught-exception` (C8); `node` reports an unhandled rejection. §188
rule 4 exists to report this drop at check time.

### 317.1 Rules

1. A value that a conditional gives (`c ? a : b`, `a ?? b`, `a || b`,
   `a && b`), directly or through a `const` binding or a pattern
   binding, carries the origins of its operands. Each origin in it is
   one of two kinds:
   1. *Made in the operand*: the operand makes it (a call, an `await`
      result). It exists only on the path of that operand, so a
      discharge through the value discharges it, as §188 rule 3 does.
      `await (c ? v(1) : v(2))` discharges both.
   2. *Selected*: the operand reads a binding or a parameter that holds
      it, and the origin exists before the conditional. A discharge
      through the value discharges it only if each operand that can be
      the value reads the same origin. `await (c ? p : q)` discharges
      neither `p` nor `q`; `await (c ? p : p)` discharges `p`.
   A selected origin that no other path discharges is S013.
   *(Amended 2026-11-16: round 1 stopped because the origin set merges
   both operands and keeps no condition; it also found that the first
   text of this rule rejected `await (c ? v(1) : v(2))`. The kind of
   rule 1 is a fact of each operand, not a path condition.)*
2. A default of a parameter that reads an earlier parameter does not
   discharge the earlier parameter. §257 rule 1 keeps the discharge of
   each other origin of the default (a call result, for example).
3. A form of §188 rule 5 (the runtime cases) stays a runtime case.
4. A program that this section rejects is accepted by `tsc`. The S013
   site is `Diverges`, as the S013 sites of §188 are, with a block of
   §79 rule 3 and an example of §311 for each new form: an `await` of
   each handle, or a call that does not make the handle that it does
   not await.
5. No accepted program that awaits each handle on each path changes.
   The corpus entries that change from accepted to rejected are
   listed in the tracking note, with the reason for each.

### 317.2 Acceptance

1. Red first: reject entries for `f`, `g`, and `h` (headers with the
   measured `tsc` result), each accepted at the pin.
2. An accept entry with output and these forms: `await p; await q;`
   where both handles exist; `c ? await v(1) : await v(2)`, where each
   handle is made on its own path only; and
   `async function f(p: Promise<i32>, q: Promise<i32> = v(2))` with
   `await p` and `await q` in the body.
3. The CLI `check` of each corpus entry at the pin and after the
   change; the tracking note `specs/tracking/s317-conditional-discharge.md`
   lists each entry that changes.
4. The total checks of §154, §307, §309, §311, and §312 pass.

### 317.3 Phase Review

Round 1 stopped: the origin set merged both operands and kept no
condition, and the first text of rule 1 rejected
`await (c ? v(1) : v(2))`. Rule 1 is amended to a kind of each origin
(made in the operand, or selected). The corpus entry a423 returned one
of two parameter handles (`return b ? h1 : h2`) and gets S013 under
rule 1; it now awaits both before it returns the selected one, and its
output does not change.

One fresh review ran 70 probes and compared 1,061 corpus entries with
a pin build: only r533, r534, and r535 differ. It found no CRITICAL or
MAJOR finding. A sync helper that returns one of two parameter handles
(`function pick(c, p, q): Promise<i32> { return c ? p : q; }`) is
rejected. This follows from §188 rule 2: a parameter is an origin, and
`return p` with an unused `q` is S013 at the pin too. Three MINOR
findings: the example of a sync body showed `await`, and the text of
two sites named an await for forms with no await (both fixed: each
example now transfers one handle, which fits an async and a sync body
and each discharge form); and two contrived correct programs are rejected
(317.4).

### 317.4 Open

1. `const t = c ? p : q; const u = c ? q : p; await t; await u;` and
   `[c ? p : q, c ? q : p]` discharge each handle on each path, and get
   S013: rule 1 reads each conditional alone. Contrived.
