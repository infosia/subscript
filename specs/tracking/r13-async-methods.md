# §37 — async instance methods on reference classes: evidence

Status: **landed and verified 2026-08-02** against `compiler.md` §37.
Origin: downstream request R13 (blocking its P5 slice 1 — the
JS-shaped API's `await gpu.requestAdapter()` surface).

Grounding recorded before contracting: stock `tsc` accepts async
class methods and permits a floating async method call (both probed
with `node_modules/.bin/tsc` against `prelude/lang.d.ts`), so r105
is a strictly-narrower `tsc`-clean pin; generic classes and
`@CStruct` value classes accept synchronous methods today, so r103
and r104 are explicit decisions, not pre-existing behavior; the
collect mark walk already roots live async frames, so receiver
survival needed no runtime change.

## §37.4 evidence (reviewer-run)

1. `a110-async-method-receiver` byte-identical under both tiers. The
   entry is stronger than the contract asked: the receiver is a
   temporary (`await receiver().run(argument())`), so during
   suspension the callee frame is its **only** reference; a second
   async root runs `Context.collect()` mid-suspension and the
   resumed prints show intact state. The golden also pins
   receiver-before-arguments order and receiver rooting during
   argument evaluation (the argument expression itself collects).
2. `a111-interop-async-method-poll` byte-identical under both tiers:
   the a95 foreign-poll loop with receiver-held `attempt`, same
   fixture counter (ready at attempt 2).
3. `r101`–`r105` pin code and line; `r105` verified `tsc`-clean
   standalone by the reviewer (exit 0).
4. Reviewer live probe: a nullable-reference variant of the
   downstream's blocking shape (`async requestAdapter():
   Promise<Adapter | null>` awaited through an object) checks and
   runs under the dev tier. The HANDOFF's literal probe text also
   trips pre-existing C7 (`i32 | null` is not a legal union, S011) —
   unrelated to R13 and noted in the report.
5. Gate 48 harnesses, 806 passed, 0 failed, exit 0 read directly;
   `tsc -p tsconfig.json` exit 0; no previously tracked golden
   moved; generated-docs byte-compare gates green with the Q34/R13
   reference block updated.

## Implementer decisions recorded

`ExprKind::AsyncCall` gained a structured callee
(`AsyncCallee::Function` / `AsyncCallee::Method` carrying the
receiver expression); the receiver is evaluated once before
arguments, temporarily rooted while arguments execute, then stored
as the callee frame's first payload slot, and `this` reloads from
the frame across resumptions. The S100 messages at the old
coroutine-method site were split so each rejected form names itself
(static / generator / value-class / generic-template async method).
No runtime, prelude, or C API change.
