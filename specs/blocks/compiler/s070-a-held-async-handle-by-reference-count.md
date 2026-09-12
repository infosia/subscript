<!-- §70 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 70. A held async handle, by reference count

Origin: the owner asked on 2026-08-27 whether a `@Shared`-style
decorator with a reference-counted handle relaxes the async
restrictions. It does. **No `Promise` object appears, and no
scheduler.** C8's model is unchanged; this section changes who may
hold the frame.

### 70.0 What this relaxes

C8 accepts `async`/`await` as poll-driven sugar over Context-owned
frames, and `r100` and `r105` reject a **floating async call**: the
result must be awaited at the call site. So a program cannot start
work, do something else, and await later.

    const t = doWork();   // r100 today
    stepRenderer();
    const v = await t;

The reason is ownership. The frame is Context-owned and its lifetime
is tied to the await; a held handle has no owner. A reference count
answers that.

**The relaxed surface is already `tsc`-clean.** `Promise<T>` is the
`tsc` view of an async function's value (C8), so holding one and
awaiting it later is valid TypeScript. This section accepts more of
what `tsc` already accepts, which invariant 5 permits without a gate
change.

### 70.1 Owner decisions, 2026-08-27

1. **Scope: the async handle only.** The reference count applies to a
   coroutine frame handle. A user-facing `@Shared` decorator on an
   arbitrary reference class is **not** in this section. It is the
   general form of the same mechanism and it waits for evidence.
2. **At least one `await` is required.** Holding a handle, storing it,
   and passing it remain legal. Dropping it without awaiting remains
   rejected. Under §94, an accepted call progresses independently of
   its holder's await. The restriction requires result observation;
   it does not control execution or cancellation. The checker keeps
   its current approximation and diagnostics (`r100`, `r105`, `r157`).

### 70.2 Where the count lives

*(Added 2026-09-08 by §94.)* The runtime scheduler also reads the
async resume pointer at frame offset 8. The count stays at offset 4.
The Context metadata stores the reload epoch and fulfilled-value size.
Generated code derives that size from the LIR function's return type.

**Measured 2026-08-27.** The allocation header is 16 bytes, fully
packed: an 8-byte state word at `-16`, a class id at `-8`, and a
position id at `-4`. Generated code reads the first two directly.

**The header does not move.** A coroutine frame already begins with

    typedef struct { int32_t state; uint32_t reserved; SubAsyncResume resume; }

and the count goes at byte offset 4. **The frame does not grow, the
header does not change, and no emitted offset moves.**

*(Corrected 2026-08-27. This section first called offset 4 "four bytes
of alignment padding that nothing reads", from reading the struct
declaration in `cemit.rs` and not looking for a writer.* `runtime/src/
context.rs` documented it as the **reload epoch**, and as an "ABI
contract with generated code". One `grep` in `runtime/` would have
found it. The round took the wrong premise and handled it correctly:
an async frame's epoch moved to Context metadata, and a generator
still uses offset 4.)*

*(The owner allowed the allocation header's offsets to move, because
no user depends on binary compatibility yet. That allowance is
recorded and unused here. A user-facing `@Shared` would need it,
because an arbitrary class's allocation has no spare word.)*

### 70.3 The rules

1. **A handle's count starts at one**, held by the value the call
   returns.
2. **A copy increments; a scope exit decrements.** The compiler emits
   both. There is no user-visible operation.

   **2a. Every store of a counted value is one path, and the verifier
   checks it.** *(Added 2026-08-28 after the Fable phase review of
   §69–§70, finding C1.)* Rule 2 says "a copy increments" and the
   lowering acquired at five sites: a local declaration, a local
   assignment, an array-literal element, a call argument, and a
   `return`. A store into a global, a class field, an array element,
   and a spread literal did not acquire. `release_scopes_from` released
   the local regardless, so the count reached zero while the other
   copy still named the frame. Measured at `7bf2559`, each accepted by
   the checker, each followed by four more awaits so the freed frame
   is reused:

       g = t          (module-level `let g: Promise<i32>`)   v=100, expected v=3
       this.h = h     (a class field)                        v=100, expected v=5
       hs[0] = t      (an index store)                       v=100, expected v=7
       [...hs]        (a spread literal)                     o=101 o1=100, expected o=1 o1=2

   Both tiers print the wrong value. Without the reuse step the dev
   tier dies with SIGSEGV and the ship tier runs the frame twice. The
   interpreter reports `unknown packed async handle` for the first
   three and agrees with the wrong output for the fourth, so the
   spread form is a defect all three share (core principle 12).

   This is a class, not four sites, and CLAUDE.md's two-round rule
   applies. The fix is two things:

   - **One path.** Every instruction the lowering emits that stores a
     counted operand into a location that outlives the expression —
     a local, a global, a field, an element, a literal, a spread, an
     argument, a return — is emitted by one function, and that
     function acquires. No store site calls the emitter directly.
   - **A total check.** The LIR verifier walks every function and,
     for every store of an operand whose type is counted, requires
     that the operand is a fresh owner used exactly once, or that its
     retain precedes the store. A violation names the instruction. A
     unit test builds the violating LIR by hand and reads the message
     (core principle 9), and `a161` is Red against the pin.

   **2b. One analysis owns "which expression copies a handle".**
   *(Same review, finding M4.)* The checker's must-await analysis
   (`expr_async_origins`) and the lowering's ownership analysis
   (`acquire_owner`) walk two different site sets. Where they
   disagree the outcome is a false rejection or the use-after-free
   above. Measured false rejections: a `for...of` over a handle array
   (S013 on every element); an arrow lambda that returns a handle;
   `hs[0] = t` awaited only through `hs[0]`. Measured leaks with no
   diagnostic: `flag ? quiet(1) : quiet(2)` retains a count the arm
   already gave (`live_bytes` 24, expected 0); `hs.pop();` as a
   statement never releases the popped element.

   The set of copy sites is one fact. Both analyses read it from one
   place, and a site absent from that place is a build failure, not a
   silent gap. Which form that takes is the round's to choose; the
   property is the contract.

   Corpus: `a161` pins the four stores of 2a with the reuse step, so
   a wrong value is visible; `a162` pins the `for...of` loop, the
   arrow lambda, the index-store-then-await, the conditional
   expression, and `pop()` as a statement, each with `live_bytes`
   read at the end. Both are Red at `7bf2559`.
3. **A count reaching zero frees the frame**, deterministically, at
   the decrement. No traversal runs and no collector is invoked, so
   invariant 2 holds: this is `delete` at a known point, not a
   collector running unbidden.

   **Measured 2026-08-27: today a coroutine frame is never freed.**
   The emitted C for `a93-async-chain` calls `subscript_rt_free` zero
   times, and a frame is allocated with class id `CLASS_GENERATOR`
   and left to the Context's lifetime. A program that awaits a
   million async calls holds a million frames until the host
   collects.

   So this section does not only decide *who* holds a frame; **it is
   the first thing that frees one.** That is a behaviour change and
   it is recorded here rather than discovered in a measurement: peak
   Context memory for an async-heavy program falls, and the fall is
   the point, not a side effect. §70.4 item 6 pins it.
4. **`await` consumes a handle's completion, not its ownership.** A
   second holder still holds it after the first awaits.
5. **A handle is not a `Promise`.** It has no `then`, no combinator,
   and no constructor. C8's rejections stand.
6. **A cycle leaks**, and a program that leaks is correct, merely
   larger — invariant 2's own words. *(Corrected 2026-09-08, the
   shape appeared, and rule 6 asked for it to be recorded.)* A frame
   can hold a handle to itself, through a module global: a handle
   created before the global is assigned, then stored into it, then
   awaited from inside its own body. The checker accepts it. Measured
   at `b1eaa56`: the dev tier reports "program terminated abnormally
   (dev-JIT child signal 4)" with exit 2, and the ship binary repeats
   its body until the stack ends. **The control**: a plain
   `function f(n: i32): i32 { return f(n + 1); }` produces the same
   dev-tier line and the same exit code, so this is unbounded
   recursion, not a defect of the handle. The language traps no stack
   depth, in either shape. Recorded, not collected and not rejected.
7. **Workers are unaffected.** Q35 gives per-Context isolation and
   copy-only messaging, so no count crosses a thread and no atomic is
   needed.

### 70.4 Corpus and gate (pre-registered exit criteria)

1. **Red first, at the contract pin.** Each entry below fails at the
   pin, verified against a binary built from it (CLAUDE.md core
   principle 10).
2. `corpus/accept/a154-held-async-handle`: start two async calls,
   do work between them, await both, and print an order that pins
   which ran when.
3. `corpus/accept/a155-async-handle-array`: hold handles in an array
   and await them in a loop.
4. `corpus/reject/r157-dropped-async-handle`: a handle that is never
   awaited. `r100` and `r105` are rewritten to reject the dropped
   form, and their headers record that the held form is now legal.
5. **The count is measured, not asserted.** A unit test reads the
   frame's count through the emitted layout and pins its value across
   a copy, a scope exit, and an await.
6. **Free is deterministic.** A test shows the frame freed at the
   decrement that reaches zero, with no `Context.collect()`.
7. **`a22` stays at or below 1.53×.** The count costs an increment and
   a decrement per handle copy; `a22` holds no async handle, so a
   change there means something else moved.
8. Gates: the standing gate, both profiles, zero warnings, `cargo fmt
   --check`, the `tsc` gate, clippy at the recorded baseline.
9. **Tracking**: `specs/tracking/s70-held-async-handle.md`.
