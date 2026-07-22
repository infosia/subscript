# P2 — runtime + HIR→CLIF + JIT: evidence

Status: in progress, 2026-07-22. Contract: `specs/blocks/compiler.md` §7.

## Gate evidence (orchestrator-verified, independent run)

- `cargo build --offline --all-targets`: zero warnings.
- `cargo test --offline`: 128 passing (59 compiler unit, 8 compiler
  integration, 28 runtime, 29 codegen, 2 golden-differential, 2 spike).
- `tsc -p tsconfig.json`: clean.
- **Golden differential: 21/21** — every inspection-authored golden
  (a01–a21) matches JIT output byte-exactly. The goldens were derived
  by manual evaluation with no access to the implementation
  (`specs/tracking/p0-seeding.md` conventions; authored 2026-07-22),
  so this is agreement between two independent evaluators.
- a22–a24 run to completion under the JIT (capture bin); goldens for
  them are captured only after the Phase Review (§2 procedure).
- Library code: zero panic sites outside test modules; reference sweep
  clean.

## Architecture as built

- `runtime/` (`subscript-runtime`, no deps): Context (allocations,
  stdout sink, roots, collect), strings, arrays, Q14 formatting, trap
  records; 25 `extern "C"` `sub_rt_*` entry points.
- `codegen/` (`subscript-codegen`): the lowering lives here, generic
  over `cranelift_module::Module` — string literals and globals are
  module data, runtime reached only through imported symbols, so P3
  instantiates the identical lowering with `ObjectModule`. JIT driver
  (`run_jit` → stdout bytes or `TrapReport`), capture bin, golden test.
- **Trap mechanism**: flag-check early-return. `Context` is `repr(C)`
  with the trap flag at offset 0 (unit-asserted); fault-capable calls
  are followed by an emitted flag check branching to a per-function
  unwind block; the script stack returns normally to the driver. No
  signals, no unwinding across the C boundary. Dev-tier `unsafeDelete`/
  `collect` retain bytes and poison the allocation header, making
  double delete and use-after-delete deterministic traps.
- **Coroutines**: CPS state machine per contract §1 — creator allocates
  a Context frame (state word, resume pointer, params, locals); resume
  dispatches on the state word; `yield` is a plain return with
  `done=0`; `.next()` zero-fills the step result before resuming (C8).

## Implementation decisions (recorded; binding until revised)

- float→int `as` saturates (`fcvt_to_*_sat`) — C's UB is replaced by a
  defined dev-tier result; hardware traps forbidden.
- `/0`, `%0` trap; `MIN / -1` wraps, `MIN % -1` = 0 (two's-complement
  policy).
- Enums lower as i32 constants. Default parameters evaluate at the call
  site. `a[i].f = x` writes in place; r-value reads copy (C2).
- Value-class method receivers are pointers to the receiver's storage.
- GC roots are named locals/globals (shadow frames); expression
  temporaries are unrooted — safe while `collect()` can only occur at
  statement-position calls (documented in `context.rs`).
- Statement-position `yield` only is exercised; a mid-expression
  `yield` with live temporaries fails Cranelift verification loudly
  rather than miscompiling.
- JIT uses `is_pic=false` dev flags; the AOT path chooses its own
  (the P0.5 spike used `is_pic=true`).

## Pending for P2 exit

- Phase Review; findings fixed in severity order.
- a22–a24 golden capture (post-review), one tracking entry each.
