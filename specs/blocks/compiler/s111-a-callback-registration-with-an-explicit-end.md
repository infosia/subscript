<!-- §111 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 111. A callback registration with an explicit end

*(Added 2026-09-19.)* Origin: a review of the callback model against a
host that starts one-shot requests.

Problem: a boundary callback does not capture (C5), so the state of one
request travels in userdata only. A host that starts N requests
therefore registers N distinct userdata objects. `bind_callback`
appends one record for each new identity, and no code removes a record
before the Context ends. `Context.collect` treats the userdata of every
record as a root (§14.4b (C)). The state of a completed request
therefore stays allocated for the life of the Context. The evidence is
the code: `Context::bind_callback` and the mark phase in
`runtime/src/context.rs`.

`Context.free` is not the answer. It destroys the object, so a script
cannot use it while another reference still reads the userdata. No
operation removes the root and keeps the object.

§14.4a called this shape one that a real host loop does not have. That
sentence described a host that registers one listener for each frame.
A host that starts requests has this shape, and §14.4a no longer
carries the sentence.

### 111.1 The rule

1. **A callback-info aggregate has one of two lifetimes, and the
   binder input selects it.** The default is the Context lifetime of
   §13.3, §14.4a, and §14.4b, and nothing in it changes. The explicit
   lifetime is selected for one aggregate by
   `subscript bind --explicit-callback-lifetime <aggregate>`, which can
   repeat. The mirror then carries one directive for that aggregate:

       // @subscript-c-callback-lifetime aggregate="<aggregate>"

   The directive sits at the position of the aggregate's declaration,
   with the other records, so the mirror is a function of the header
   and the selected set, not of the option order. Four selections are
   bind errors: an aggregate the header does not declare, an aggregate
   that carries no callback field, an aggregate that the mirror absorbs
   and emits no class for, and one aggregate selected two times. The
   language surface does not change: the mirrored class is the same
   text with and without the option.
2. **The form carries the selection.** The boundary class of HIR and
   of LIR carries the lifetime as a field. Each lowering reads that
   field. No lowering derives the lifetime from a name.
3. **Each crossing of an explicit-lifetime aggregate creates one
   registration.** The record is not interned. Two crossings with the
   same callback and the same userdata create two registrations, and
   each one ends apart from the other. The pointer equality of §14.4a
   consequence 1 does not hold for these aggregates.
4. **The host receives the registration where it receives a binding
   today.** The generated code calls `subscript_rt_cb_register`, which
   takes the parameters of `subscript_rt_cb_bind`. The marshaling
   writes `subscript_rt_cb_registration_trampoline` into the callback
   field, the registration pointer into the first userdata field, and
   null into the second. The host passes both back when it fires, as
   it does today.
5. **The host ends a registration one time.**

       int32_t subscript_rt_ctx_callback_release(
           subscript_rt_context *ctx, void *registration);

   The call states a guarantee: the host starts no more calls through
   this registration. Calls that already run can return. The call does
   not cancel native work and does not unregister a native callback.
   The host adapter does those first. The call returns 1 when
   `registration` is an open registration of `ctx`. If it is not, the
   call changes nothing and returns 0. It returns no other value. The
   answer reads the live set and never the pointer, so every pointer
   value is safe to pass. After a registration ends, a later
   registration can take its address, and a second release of the old
   pointer then closes the new one. That case is in the best-effort
   class of rule 14. The
   result is `int32_t`, as
   every yes-or-no result of the host header is, so the header takes
   no new include.
5a. **The registration answers which Context it belongs to.**

       subscript_rt_context *subscript_rt_cb_registration_context(
           void *registration);

   A host function that a script calls receives the mirrored arguments
   and no Context. A host that runs more than one Context, a Worker
   host for example, therefore cannot name the Context that rule 5
   needs. The registration carries that fact, and this call reads it.
   The host calls it while the registration is certainly live: at the
   crossing that delivers the registration, or inside a fire. It reads
   through the pointer, so a pointer that is not a live registration is
   a violation of the host's guarantee, in the class of rule 14.
6. **The root predicate is `open || active_calls > 0`.** The
   trampoline adds one to `active_calls` before it validates userdata,
   allocates, or enters script code. It subtracts one on every exit
   of a call that it counted. A trap inside the callback and a failed
   userdata check are such exits. A fire that rule 14 refuses is not
   counted, so it subtracts nothing. When a
   registration is closed and `active_calls` is 0, the runtime removes
   the record from the live set, returns its charge, and frees its
   storage. It does that before the release call or the trampoline
   returns.
7. **Release removes a root and does nothing else.** It does not
   collect, it does not free userdata, and it does not walk the
   userdata graph. The userdata then follows the reachability rules of
   the Context: a script reference keeps it, and the next explicit
   `Context.collect()` reclaims it when nothing reaches it (invariant
   2).
8. **The live set holds open and active registrations only.**
   `Context.collect` derives its roots from that set, as §14.4b (C)
   derives them from the binding records. Removal is O(1). A
   registration that ended leaves no entry.
9. **§14.4b applies to a live registration.** The trampoline checks
   userdata liveness at fire (A). `Context.free` of userdata that a
   live registration holds is advised (B). The count of §14.4b (B2) is
   the binding records plus the live registrations, and the message
   text of (B2) does not change.
10. **A new registration charges the quota as a binding record does**
   (§109.4 rule 2). The charge is the size of the registration record.
   The record exists when the quota refuses it, and
   the recorded trap stops the run at the next checkpoint. A null in a
   C `void *` slot is never published.
11. **Everything runs on the Context owner thread.** The counter is
   not atomic. The host brings a notification from another thread to
   the owner thread before it fires, and its release guarantee counts
   the notifications that wait.
12. **Context destruction frees every registration.** Before it, the
   host stops each notification source. No callback runs from the
   destruction, and no script code runs.
13. **A registration that is open across a hot reload calls the code
   it was created with.** The reload session keeps every dev-JIT
   generation that placed a callback, which is the rule the pending
   queue follows today.
14. **A fire through a registration that the host released is a
   violation of the host's guarantee, and it traps.** The trampoline
   finds the record through the live set. If the record is closed, or
   is not in the set, the fire enters no script code and records the
   trap `callback-registration-ended` at position 0, because no
   script site exists. If the Context already holds a trap, that trap
   stays, as for every other kind. The trap is certain while a
   call keeps a closed record alive. After the record ends it is
   best-effort, in the class of §14.4b (A): certain while the storage
   is not used again, and undefined after that. The runtime keeps no
   record of ended registrations. Each tier maps the kind to a trap
   site by name, as it maps `callback-userdata-freed`; no tier reaches
   it through a fallback arm.
   *(Superseded 2026-09-19 for the position: §112 makes id 0 the
   empty position in every table, so no tier names this kind to find
   it. The interpreter's mapping of the kind to a trap site stays by
   name.)*

### 111.2 What the host adapter owns

The adapter knows when the native side can no longer fire. The
compiler does not derive that from a name or from a mode value.

- **One-shot.** Create, publish, receive the last callback, establish
  that no later call can occur, release. A start function that fires
  before it returns uses the same release path.
- **Repeated.** Create, subscribe, receive notifications, request the
  native removal, establish that no notification can still fire,
  release. A request to cancel does not establish that. If the native
  removal ends later, the adapter holds the registration until the
  native side confirms it.
- **Release from inside the callback** is legal. Rule 6 keeps the
  userdata rooted until the last active call returns.

### 111.3 Fixture and corpus

The neutral fixture `corpus/interop/interop.h` gains one
explicit-lifetime aggregate and the functions that drive it: a
one-shot start that can complete before it returns, a subscription
with a removal that ends at a later pump, and a driver that fires what
is queued. It names no external project (invariant 4). This section
states what the entries observe. `corpus/interop/interop.h` states
the adapter's own rules: the bound on pending one-shots, the refusal
that ends a registration at once and answers -1, and the pump that
fires what is queued when the drain starts.

The fixture is the adapter of 111.2. It reads the Context at the
crossing (rule 5a) and calls the release itself. An entry observes
reclamation through the fixture: the fixture reads
`subscript_rt_ctx_charged_bytes` and returns a comparison, and the
script prints the result. No script-visible accounting is added. The
entries follow `a90-callback-userdata-rooted` and carry
`// interpreter: no`, as every interop callback entry does.

Accept entries, each with a committed golden on both tiers:

1. Register, drop the script references, collect, fire: the callback
   reads its userdata fields. Release, collect: the charged bytes fall
   by the userdata graph.
2. Register the same userdata two times. Release one. Collect and
   fire the other: the read is valid.
3. Release from inside the callback, collect inside the same
   callback, then read userdata before the return. This entry pins
   that the release is legal there and answers 1. It does not prove
   the root of rule 6: the frame of the callback holds its own
   userdata, so the script keeps it alive without the registration.
   The runtime unit test proves rule 6 with a callback that holds no
   script root, and the second trap entry proves that the closed
   record stays alive.
4. A one-shot that completes inside the start call, and one that
   completes at a later pump: each releases one time.
5. A script reference kept after release: collect keeps the object.
6. A chain of three one-shots, where each callback starts the next
   one while its own call runs: each registration ends one time, and
   collect reclaims the three graphs.

Two trap entries: the fire-time userdata check through a
registration (rule 9 (A)), and a fire through a closed registration
from inside its own callback (rule 14, the certain case). No
`.expected` file of an existing entry moves. If one moves, stop and
report. The LIR text snapshot is a total listing of the fixture, so
the new class and the new foreign functions move its ids and its
mirror line numbers; with those normalised, the snapshot only gains
lines.

### 111.4 Exit criteria

1. Runtime unit tests, one for each public item: create, release,
   release of a pointer that is not an open registration, the nested
   call counter, release during an active call, a trap inside the
   callback, quota refusal.
2. Retention, measured by a runtime test at N = 10,000 distinct
   userdata objects. After N registrations and N releases, with no
   collection, the live registration count is 0 and the binding charge
   equals its value at N = 0. After one `collect()`, `charged_bytes`
   equals its value at N = 0. The same workload on the Context-lifetime
   path is the firing control: its charge grows by N records.
3. The Context-lifetime path is unchanged: the §14.4a and §14.4b tests
   and every existing golden pass without an edit.
4. `subscript bind` with the option regenerates the fixture mirror
   byte for byte, and the mirror without the option is unchanged.
5. The standing gate is green. The new entries run in the gating
   class of the existing interop entries, byte-exact on each tier
   that class runs.
6. Each new test states its measured cost (core principle 15). The
   retention test does its N-sized work one time for each fact.

### 111.5 Out of scope

Automatic collection, shared ownership of one registration, access to
a Context from a second thread, a script-visible registration object,
and cancellation of a language-level async function.
