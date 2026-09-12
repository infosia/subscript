<!-- §37 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 37. R13 — async instance methods on reference classes

Owner decision 2026-08-02 (downstream request R13, blocking its P5
slice 1). The Q34 model gains a receiver; nothing else in §26
moves. Probed before contracting: stock `tsc` accepts
`async name(): Promise<T>` as a class method and permits a floating
async method call, so the floating rejection below is a
strictly-narrower pin (r105 `tsc`-clean). Checker probes: generic
classes and `@CStruct` value classes accept synchronous methods
today, so their async variants are explicit rejections here, not
pre-existing behavior.

### 37.1 Checker

`async name(args): Promise<T>` is accepted as an **instance method
of a plain, non-generic reference class**, with the explicit
`Promise<T>` return annotation §26.1 requires; `await` legality
inside the body is §26.1 unchanged. The await grammar gains exactly
one form: `await recv.m(...)` — a direct call of an async method
through a receiver expression of the declaring class's type
(including `this`). The receiver is evaluated once, before the
arguments.

Rejected, with corpus pins: an async static method (r101), an async
generator method (r102), an async method on a `@CStruct` value
class (r103), an async method on a generic class template (r104),
and a non-awaited async method call — floating statement or value
position (r105, S013, `tsc`-clean). *(§64, 2026-08-23: the generic-class
rejection is withdrawn; r104 retires.)* Async methods on `@Descriptor`
classes stay covered by the existing methods-on-descriptors
rejection (r94). Awaiting a synchronous method is an error,
symmetric with §26.1's synchronous-function case. Async methods are
not first-class values, exactly as async functions are not.

### 37.2 Lowering (both tiers)

HIR already carries a method as a function whose first argument is
the receiver; an async method lowers as a §26.2 async function
whose frame's first slot holds the receiver reference.
`await recv.m(...)` evaluates the receiver, allocates the callee
frame with the receiver and arguments stored, and proceeds per
§26.2. The receiver persists across suspensions because the frame
does: the collect mark walk already treats live async frames as
roots, so a receiver held by a suspended frame survives an explicit
`Context.collect()`. No runtime C API change; methods are not
module exports, so the §26.3 standard-runner convention is
untouched.

### 37.3 Corpus

`a110-async-method-receiver` (accept): a reference class whose
async method awaits `Context.suspend()` and awaits a sibling async
method through `this`, mutating receiver fields across suspensions;
async `main` awaits it through an object; a second exported async
root runs `Context.collect()` while the first chain is suspended
(the a94 two-root kick), so the resumed method's prints pin
receiver survival under explicit collection.
`a111-interop-async-method-poll` (accept): the a95 foreign-poll
shape with a receiver — a class wrapping the fixture's
`subDevicePoll`, its async method polling to readiness. Rejects
r101–r105 as listed in §37.1.

### 37.4 Exit criteria (pre-registered)

1. `a110`/`a111` byte-identical under both tiers, driven by the
   standard §26.3 pump; `a111` reaches readiness through the same
   fixture counter `a95` uses.
2. `r101`–`r105` pin (code, line); `r105` type-checks under stock
   `tsc`, recorded in its header.
3. `a110`'s golden shows receiver state intact after a
   `Context.collect()` issued while the method was suspended.
4. `tsc` gate green with the new accepts included; no existing
   golden moves; full gate green; zero-warning sweep unaffected.
