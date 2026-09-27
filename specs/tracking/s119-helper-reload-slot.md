# §119 A synthesized helper has no reload slot — evidence

Contract: `specs/blocks/compiler/s119-a-synthesized-helper-has-no-reload-slot.md`.
Date: 2026-09-27. Host: arm64 macOS.

## Before the change

At `9605ba8`, a reload whose new source added `JSON.parse`,
`JSON.stringify`, `decodeURI`, and `Error.toString` calls inside an
existing body was refused at the first differing declaration,
`[[json.parse#1.validate#0]]` (codex round, dev JIT).

## First round

The HIR function carries `synthesized_helper`, set by every helper
constructor (`hir::Function::new_synthesized_helper`, six sites in
`compiler/src/check/json.rs` and `text.rs`), carried to LIR as
`FunctionKind::SynthesizedHelper`. The dev JIT gives a helper no table
slot and calls it directly (`codegen/src/lower/func/call.rs`); the hash
leaves it out. A helper with a carrier parameter fails lowering. The
ship C and the interpreter call helpers as before. The same edit is then
an accepted swap, and the new calls run. The LIR text snapshot did not
move.

`gate full 9605ba8 dirty:13 debug 1736/0/2 release 1733/0/2 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.

## Review

A fresh review verified every call path to a helper (user bodies,
methods, constructors, generic instances, lambdas, the initializer,
coroutine resumes, which trap on a stale epoch first), the slot
numbering, the hash, the rule-4 test, and both ship tiers; a live
`--watch` probe swapped with a retained lambda across a helper
renumbering. MAJOR: the Error class is created on first use, so a
module's first `JSON.parse`, `decodeURI`, or `new Error` added `class
Error` to the hash and refused the swap (probe: "refused: class Error").
§119.1 rule 6 closes it.

## Rule 6, first form, and its review

The first form declared the class after every user class and rotated it
to the last id with `put_error_last` (`compiler/src/check/exception/class_order.rs`,
223 lines), which rewrote every `ClassId` it knew in the HIR. Eight tests
that pinned the class count, a metadata fixture, and C function numbers
moved on purpose; the dev-JIT user slots did not move; first
`JSON.parse`, `decodeURI`, and `new Error` each became an accepted swap.
`gate full cd3f564 dirty:25 debug 1740/0/2 release 1737/0/2 skips 2/0 clippy 5/18/13 goldens-moved 2 exit 0`.

The review found no CRITICAL or MAJOR, and one MINOR that is a form
defect: the rewrite enumerated the HIR by hand with `..` arms, so a
`ClassId` field added later escapes it with no compiler error. Rule 6
now declares the class first (id 0), and the rewrite goes.

## Rule 6, landed form

The Error class is declared before Pass A (`compiler/src/check/mod.rs`),
so it is id 0 and every user class and generic instance follows in
declaration order; `class_order.rs` and `put_error_last` are deleted. A
user class named `Error` still makes the built-in `[[Error]]`. Tests that
pinned the last-id form now pin id 0 (class ids in the compiler tests,
the dev-JIT slot test: Error constructor 4, user constructor 5, method
6, `Init` 7; the allocation metadata fixture through its capture path).
The LIR text snapshot moved in its class table and the ids derived from
it; with ids normalized, every function body in the 48 snapshots is
equal. No `.expected` golden moved. The reload tests (30) pass,
including first `JSON.parse`, first `decodeURI`, first `new Error`, a
retained lambda, and a generic function instance's first use refused as
before.
`gate full 1c60babd721ebfc8b3410edcd9766a7012cc6b27 dirty:29 debug 1741/0/2 release 1738/0/2 skips 2/0 clippy 5/18/13 goldens-moved 2 exit 0`.
