# Duplication and over-complexity review — 2026-08-30

Owner decision, 2026-08-30: fix every MAJOR; decide on the MINORs after.

## Scope and method

Three fresh reviewers, one per crate (`codegen/`, `compiler/`,
`runtime/`), read-only, at `f99d4cb`. Two classes: (A) one computation
or decision table written in two or more places; (B) a mechanism larger
than the problem. The three transcribers' independence (§68) is by
design and was excluded.

Counts: codegen MAJOR 6 / MINOR 17; compiler MAJOR 6 / MINOR 21;
runtime MAJOR 4 / MINOR 28.

## The MAJOR findings and their contracts

| # | Crate | Finding | Contract | Round |
|---|---|---|---|---|
| R1 | runtime | `map`/`filter` did not root the result across callbacks; the fixed family did | §8.1e rule 1 | 1 |
| R2 | runtime | per-class release work on three free paths | §8.1e rule 2 | 1 |
| C2 | compiler | three integer-literal readers; enum reader read the `f64` value | §72.1 | 2 |
| — | compiler/codegen | `enum as i64` accepted and not lowered (found by the C2 probe) | §72.1 rule 3 | 2 |
| C3 | compiler | 13 + 13 hand-written HIR walks; two drop subtrees | §72.2 | 2 |
| C1 | compiler | four handle-type tables, drifted | §74 rule 1 | 3 |
| G1 | codegen | managed-type table in `layout.rs` and `cemit.rs`, drifted | §74 rules 2–3 | 3 |
| G4 | codegen | eleven Terminator walks; `invalidates` counted as a use in three | §73 | 4 |
| G2 | codegen | local-storage verifier is a copy of the lowering walk | §75.4 | 5 |
| G3 | codegen | fresh-async-owner classification in three tables | §75.1 | 5 |
| G5 | codegen | embedded-boundary-header derived four times | §75.2 | 5 |
| G6 | codegen | boundary-struct-pointer predicate six times, two wrong | §75.3 | 5 |
| C4 | compiler | assignment re-derives the place kind from the lowered `get` call | §76.1 | 6 |
| C5 | compiler | absence test erased to an `Int` sentinel | §76.2 | 6 |
| C6 | compiler | `using` lowered in two passes keyed by `Pos` | §76.3 | 6 |
| R3 | runtime | emitted-check trap messages are copies of the runtime's | §77 rule 1 | 7 |
| R4 | runtime | `===`/SameValueZero written in `arrops` and `assocops` | §77 rule 2 | 7 |

## Round 1 — runtime rooting and release (landed `f0cc4ed`)

Red: `a173` trapped `[internal]: array storage disappeared while
growing it` at `5487643` on the dev JIT, through a function-value
callback (a known callback takes the §8.1d loop and never reached the
runtime).

Fix: one loop per operation over an element source; `map` and `filter`
root the result across the callbacks. `Context::release_class_state`
replaces three copies; a unit test frees each class on each of the
three paths.

Measured: `a173` golden on all three tiers. Runtime 253 passed. Clippy
7/21/13. Gate on main: 1118 passed; the 3 failures are the round-2 Red
entries (`a174`, `r170`, `r171`).

The round stopped once on the corpus inventory assertions
(`golden.rs`, `corpus_accept.rs`, `corpus_reject.rs`, `corpus_warn.rs`,
`js_corpus.rs`, `lir.rs`, `generated-docs/`), which the handoff did
not name. Rule: a corpus addition updates every inventory assertion and
regenerates `generated-docs/` in the same commit as the entry.

## Round 2 — integer literals, enum widening, one HIR walk (landed `ea92d8a`)

Red at `154e221`: `a174` failed in the dev JIT with a Cranelift verifier
error (`arg 1 has type i32, expected i64`); `r170` and `r171` were
accepted and ran.

Fix: `parse_integer_spelling(raw, negate) -> Option<i128>` is the one
reader; enum members are range-checked to `i32`. The enum-to-integer
`Cast` lowered in the C emitter and the interpreter already; Cranelift
did not extend the source. `hir::Expr::children()` and
`hir::Stmt::children()` replace 13 + 13 walks; 1,484 lines removed,
1,142 added.

Measured: `a174` golden on all three tiers; `r170`, `r171` report `S100`
at the initializer (the message text still reads "enum members must
have integer literal values"; a range-specific message is a MINOR for
the next pass). Clippy 7/21/13. Gate on main: 1,125 passed, 0 failed.

## Round 3 — one handle-kind table (landed `85242e9`)

`Type::handle_kind` is the one table; four checker predicates and
`codegen/src/layout.rs` are filters over it. The C emitter's two copies
are deleted.

The first report widened two acceptance filters: `i32[] | null` (S011)
and `===` on arrays and `RegExp` (S100) became accepted. Measured
against main with two probe programs. §74 gained rule 1a (an acceptance
filter keeps the recorded answer; a widening is a corpus decision), and
the correction round restored the answers and pinned both diagnostics
in compiler unit tests. The fact filters keep the runtime's answers:
`AsyncHandle`, `RegExp`, and the `Func | null` box are managed and
dereference a Context allocation.

Candidate widenings recorded in the filters' comments, not decided:
`T[] | null`, `RegExp | null`, `Generator | null`, `AsyncHandle | null`
(S011); identity `===` on `RegExp`, `object`, arrays, generators, async
handles, `Worker`, `Inbox`, `Outbox` (S100).

Measured: `Worker`, `Inbox`, `Outbox`, bare `Func` not managed (§74
rule 3 record). Clippy 7/21/13. Gate on main: 1,131 passed, 0 failed.

## Round 4 — the terminator walks itself (landed `e3b9d4f`)

`Terminator::targets()`, `successors()`, `value_uses()`, `map_values()`
replace eleven walks; 607 lines removed, 481 added. Reads versus
mentions: liveness, address escape, and C declaration references read;
value replacement, unroll's external-use check, the root plan's
interference, and copy elimination mention (each with a one-line reason
at the site). Clippy 7/21/13. Gate on main: 1,133 passed, 0 failed.

The debug-profile interpreter ledger (`DEBUG_INTERPRETER_SUBSET`,
`DEBUG_RUNNABLE_COUNT`) had not gained `a173` and `a174`; added with
this round's record. The inventory list of round 1 gains that ledger.

## Round 5 — four LIR facts (landed `f5bb47d`)

Fresh-owner bit on the LIR value from one instruction table (function
parameters are not fresh owners; §70.3 rules 1–2). `is_embedded_header`
set once. One `boundary_box_class`; the two one-condition sites
differed on `T | null` with `T` a boundary reference class (old 1695)
and with `T` a non-boundary value class (old 5029); the 173-entry trace
reaches neither input — a corpus entry for each shape is open (core
principle 12). `verify_local_storage_classes` deleted; the interpreter
poisons Activation locals at every Suspend. 443 lines removed, 327
added. Clippy 7/21/13. Gate on main: 1,135 passed, 0 failed.

## Round 6 — three checker facts (landed `1224881`)

`Place` classified before member lowering (seven variants);
`ExprKind::AbsenceTest` replaces the `Int` sentinel and `narrow_paths`
loses its alias closure; `using` is `Stmt::Let { dispose: true }` with
one scope-exit pass (195 lines removed, 122 added in `check/mod.rs`).
The round stopped once: the `dispose` field needed pattern updates in
`codegen/src/lir.rs`, outside the handoff's file list; the list was
widened. Clippy 7/21/13. Gate on main: 1,137 passed, 0 failed.

## Round 7 — two runtime facts (landed `479676b`)

`TrapKind::message` owns every trap message; two kinds had two
spellings. `runtime/src/valeq.rs` owns `value_eq` and `read_uint`;
`F16` is array-only (§10.2 rejects it as a key; the `KeyKind` ABI has
no `F16` tag). Clippy 7/21/13. Gate on main: 1,140 passed, 0 failed.

## MAJOR pass: state

All 16 MAJOR findings landed in seven rounds (`5487643`..`479676b`).
The MINORs (66) are not started. A Phase Review of the cumulative diff
follows.

## Phase Review of the MAJOR pass (`f850e3d`..`5b87ad1`)

One fresh reviewer: CRITICAL 0, MAJOR 4, MINOR 10. All 14 fixed in one
round, landed `a7b9da3`.

MAJOR: `using` in a lambda body had become accepted (§60 says S100;
`r172` pins it); `has_dispose_binding` was a hand-written `Stmt` walk;
`HandleKind::contains_managed` counted a bare `Func` against the §74
rule 3 record; `children()` was `pub(crate)` and two codegen walks
stayed hand-written.

Correction to a handoff claim: `managed_words(Func)` was 2 at
`f850e3d` (`has_managed_interior` listed `Func`), not 0. The value is
now 0 by §74 rule 3 (the environment lives in activation or coroutine
storage and the shared plan roots it). Both tiers share the change, so
`a175-closure-environment-collect` pins the environment across
`Context.collect` on all three tiers (core principle 12).

Measured for §74's `needs_lifetime_trap`: a dev-tier dereference of a
deleted `RegExp`, `AsyncHandle`, or `Func | null` is unreachable, since
`Context.free` accepts `object` only.

`tools/hygiene.sh`: exit 0 at `a7b9da3`. Gate on main: 1,141 passed,
0 failed. Clippy 7/21/13.

**The MAJOR pass is COMPLETE.** Open: 66 MINORs from the three reviews
(`review-codegen`, `review-compiler`, `review-runtime` reports are
not tracked; the MINOR list is reproduced below when the pass is
ordered); the r170/r171 message text; two §75.3 shapes without a
corpus entry; the acceptance widenings recorded in the §74 filters.

## The MINOR findings (not started; line numbers at `f99d4cb`)

### codegen (17)
- Gm7: LIR trap kind to runtime trap kind mapped three ways. Fix: `impl l::TrapKind { fn runtime_kind(&self) -> Option<TrapKind> }` next to the contract.
- Gm8: six helpers duplicated verbatim between the two transcribers. Fix: move all six to `lir_types.rs`.
- Gm9: padding-range walk in four copies. Fix: `Layouts::padding_ranges(ty)` with no module argument; the C emitter and the interpreter consume the ranges.
- Gm10: layout arithmetic helpers duplicated. Fix: export the `layout.rs` set with `pub(crate)`.
- Gm11: `is_unsigned` twice. Fix: delete the cemit copy.
- Gm12: in-file duplicates in `lir.rs`. Fix: one free function per predicate.
- Gm13: `lower_async_call` and `lower_async_handle_create` share their first 85 lines. Fix: extract `resolve_async_target` and keep only the terminator/instruction tail in each.
- Gm14: dominance decision in two shapes. Fix: `check_dominates` calls `definition_dominates_definition` with a synthesized use site.
- Gm15: call-target operand start index table twice. Fix: one `fn counted_operand_start(kind: &CallTargetKind) -> Option<usize>`.
- Gm16: Worker intrinsic numbering in two tables. Fix: a `WorkerFn::ALL` table in `hir` and `intrinsic_index`.
- Gm17: runtime symbol names kept in four tables. Fix: `l::IntrinsicOperation` carries `runtime_symbol`; a macro emits the `jit.rs` pairs from the `ffi` names.
- Gm18: template format dispatch is a per-transcriber type table. Fix: the `Template` instruction carries a `FormatKind` per piece (core principle 8).
- Gm19: interpreter computes storage layout twice in one file. Fix: memoize every `type_layout` result and make `layout_cached` a map read.
- Gm20: `operand_is_fresh_owner` scans every block per query. Fix: insert the result id into the set at emit time and delete the walk.
- Gm21: `local_requires_frame` is quadratic over blocks per local. Fix: compute the store-block set per local once before the loop.
- Gm22: `consume_call_traps` and `consume_runtime_traps` differ by one accepted kind. Fix: one function with an `accepts_stale_coroutine: bool` argument, or accept the union in one place.
- Gm23: tier run entry points are five thin wrappers each. Fix: one public `RunConfig` struct with `Default` and one `run` per tier.

### compiler (21)
- Cm7: three integer-range tables plus two width tables. Fix: `Type::int_bounds() -> Option<(i128, i128)>` and `Type::bit_width()` in `types.rs`; the f64-exact cap and the i64 lattice cap are one `min`/`clamp` at the caller.
- Cm8: the ambient-name shadow rule is written eleven times. Fix: `fn ambient_visible(&self, name, fx) -> bool` and `fn ambient_namespace(&self, obj, fx) -> Option<&'static str>`.
- Cm9: the literal-shift-amount check and the compound operator typing exist twice. Fix: `check_assign` calls `bin_result(op, target_read, value)` for the compound case and takes its type.
- Cm10: two tables over `ast::AssignOp` and three copies of the `++`/`--` spelling. Fix: one `fn assign_op(op) -> Option<(BinOp, &'static str)>`; one `update_spelling(u)` helper.
- Cm11: `module_decl` (`check/mod.rs:36`) re-implemented ad hoc. Fix: call `module_decl(item)`.
- Cm12: the spread-argument rejection is written four times. Fix: route the Set-algebra argument and the callback argument through `check_args` with a one-parameter signature.
- Cm13: the 15-arm plain-scalar whitelist appears twice. Fix: `fn plain_value_leaf(&self, ty) -> bool`; the message walk uses it and adds only the path.
- Cm14: method-lookup enums carry non-method members that lookup filters and consumers re-reject. Fix: separate instance-method enums from constructor/static identities, so the lookup return type has no unreachable arm.
- Cm15: `ParamSig { name: String::new(), ty, has_default: false }` is built 34 times in `check/expr.rs`. Fix: `ParamSig::positional(ty)`.
- Cm16: `async_origins_at_copy_site` (`check/expr.rs:186-203`) is a nine-arm match where every arm calls `expr_async_origins`; `site` is unused. Fix: delete the function; callers call `expr_async_origins`.
- Cm17: "AsyncHandle or AsyncHandle[]" predicate written three times. Fix: `Type::carries_async_handle()`.
- Cm18: three value-flow walkers with the same arm set (`Local`, `Cast`, `Cond`, `Assign`, `ArrayLit`). Fix: `hir::Expr::flow_leaves()` iterator; each predicate is `any` over it.
- Cm19: container-to-element table exists twice. Fix: `Type::iteration_element() -> Option<(IterKind, Type)>`; both enums map from `IterKind`.
- Cm20: `validate_generator_layout` (`check/layout.rs:508-600`) repeats the place-and-check block four times (receiver, params, lets, child frames). Fix: one `place(&mut end, layout, pos, what) -> bool` closure.
- Cm21: `AmbientFn` has three name tables. Fix: `AmbientFn::name()` as `MathFn` has; lookup and label read it.
- Cm22: JSON synthesis re-declares locals and hand-copies kind codes. Fix: one `JsonLocals` struct built once per helper; `JsonKind` constants next to `json_number_target`.
- Cm23: `reduce_acc_context` (`check/expr.rs:4757-4783`) resolves the annotation, truncates `self.diags` to hide its errors, then `check_lambda_with` resolves it again. Fix: resolve once and pass the type into the lambda check.
- Cm24: `callback_params_for_arity` (`check/expr.rs:4880-4886`) derives the Q rule from `method.starts_with("Map.")`. Fix: pass the rule with the method label.
- Cm25: `check_named_call` has two identical "is not generic" arms. Fix: one guard before the match on `c.type_args`.
- Cm26: the `w001` and `w003` statement walks carry the same `loop_depth` bookkeeping. Fix: covered by finding 3; one walk with a per-statement callback.
- Cm27: the `_ =>` arms in `check/mod.rs:889` (`ModuleEffects::expr`) and `check/mod.rs:832` duplicate the walk in finding 3 for a second module-level pass over every body. Fix: covered by finding 3; the effects pass folds over `children()`.

### runtime (28)
- Rm5: Allocation header written at four sites, class id read at ten sites through raw offsets. Sites: writes `context.rs:1606-1609`, `1712-1715`, `1734-1737`, `1810-1813`; reads `context.rs:1905, 1930, 1959, 2031, 2088, 2297, 2318, 2334, 2388, 2393` use `base.add(8)` while `CLASS_ID_OFFSET` / `POS_ID_OFFSET` (`165-167`) exist. Consolidation: `write_header(base, class, pos)` and `header_class_id(base)`.
- Rm6: Dev `alloc` and `arena_alloc_large` have the same layout / `alloc_zeroed` / two-trap sequence. Sites: `context.rs:1588-1610` and `context.rs:1790-1815`. Consolidation: one `alloc_system_block(size, class, pos) -> Option<(base, layout)>`.
- Rm7: Three chunk walks that test `LIVE_STATE` per block. Sites: `context.rs:2189-2200` (`live_count`), `2216-2230` (`live_bytes`), `2284-2303` (`visit_live_allocations`). Consolidation: one `live_blocks()` iterator yielding `(base, block_size)`.
- Rm8: `collect_with_trace` repeats the same push loop nine times with a hand-built `MarkSource::Root { set, index, word }`. Sites: `context.rs:2412-2530`. Consolidation: `push_root_set(name, impl Iterator<Item = usize>)`.
- Rm9: The dev mark walk and the arena mark walk both implement "stamp, `class_holds_no_handle`, scan payload words". Sites: `context.rs:2559-2588` and `context.rs:2699-2750`. Consolidation: one `scan_payload(payload, size, class_id, work, tracer)` called by both after the tier-specific stamp.
- Rm10: `str_slice` reimplements the relative-index clamp and the UTF-8 boundary check plus allocation. Sites: `ffi.rs:1224-1231` (closure `relative`) against `arrops.rs:656-661` (`clamp_index`) and `strops.rs:127-140` (`substr_range`); `ffi.rs:1239-1256` against `str_alloc_range` (`ffi.rs:1435-1461`). Consolidation: `strops::slice_range(len, start, end)` and a call to `str_alloc_range`.
- Rm11: `str_char_at` and `str_code_point_at` share the copy / index filter / `is_char_boundary` / `chars().next()` walk. Sites: `ffi.rs:1518-1556` and `ffi.rs:1558-1599`. Consolidation: one `code_point_at(bytes, i) -> Result<(usize, char), Reason>`.
- Rm12: Twenty-three string operations copy the receiver into a `Vec<u8>` "so the borrow does not overlap" (`ffi.rs:1223, 1447, 1530, 1570, 1634, 1636, 1683, 1763, 1882, 1937-1941, 1968-1972, 2344, 2363, 2580, 2840, 2933, 2962`; `arrops.rs:581, 645`). `str_concat` (`ffi.rs:1172-1181`) and `str_pad` (`ffi.rs:1795-1803`) already show the pointer-snapshot form, and the comment at `ffi.rs:1170` states the invariant (an allocation does not move an immutable input). Consolidation: one `str_view(handle) -> (ptr, len)` helper; every entry uses it.
- Rm13: `alloc_formatted` is `Context::alloc_str`. Sites: `ffi.rs:2148-2153` against `context.rs:2845-2850`. Consolidation: delete `alloc_formatted`.
- Rm14: Four identical integer format wrappers where the JSON side already uses a macro. Sites: `ffi.rs:2161-2210` (`fmt_i32/u32/i64/u64`) against `json_integer!` at `ffi.rs:2374-2394`. Consolidation: the same macro shape.
- Rm15: `json_f32` / `json_f64` and `json_begin` / `json_begin_tracked` are pairwise identical apart from the type or a `bool`. Sites: `ffi.rs:2396-2446`, `ffi.rs:2270-2312`. Consolidation: one body each with a parameter.
- Rm16: Ten JSON-parse wrappers repeat `match Option { Some(v) => v, None => { json_parser_invalid(..); default } }`. Sites: `ffi.rs:2590-2860`. Consolidation: `fn parsed<T>(ctx, value: Option<T>, default: T, op, pos) -> T`.
- Rm17: Radix and digit validation is written twice per entry with the same message in both branches. Sites: `ffi.rs:2996-3011` (`to_fixed`), `ffi.rs:3026-3045` and `3063-3082` (`to_string_f32/f64`), `ffi.rs:2927-2934` (`parse_int`). Consolidation: `fn checked_range(ctx, value: i32, lo, hi, what, pos) -> Option<u32>`.
- Rm18: `parse_int` / `parse_float` trap on a non-UTF-8 language string. Sites: `ffi.rs:2937-2944`, `2966-2973`. Language strings are UTF-8 by construction (`ffi.rs:1237-1238`); every other entry uses `unwrap_or_default`. The branch is unreachable. Consolidation: `from_utf8(..).unwrap_or_default()` as elsewhere, or one shared `str_text(handle)`.
- Rm19: Map/Set FFI pairs have identical bodies. Sites: `ffi.rs:576-606` (`map_size`/`set_size`), `697-735` (`map_has`/`set_has`), `737-775` (`map_delete`/`set_delete`), `777-811` (`map_clear`/`set_clear`). Consolidation: one `subscript_rt_assoc_*` symbol per operation; the code generators already pass the same handle shape.
- Rm20: `fixed_arr_search_entry` and `fixed_arr_reduce_entry` re-encode a selection (`operation: u8` 0/1/2, `right: bool`) that `arrops` already expresses as `SearchMode` and `ReduceDirection`. Sites: `ffi.rs:4440-4499`, `ffi.rs:4548-4606` against `arrops.rs:1137-1149`, `1286-1300`. The dynamic wrappers (`ffi.rs:4264-4334`) call the three `arrops` entries directly. Consolidation: make `arrops::fixed_search(mode)` / `fixed_reduce_direction(dir)` `pub(crate)` and pass the enum.
- Rm21: Four spread loops push element-by-element; `array_spread_array` duplicates `arrops::concat`'s copy loop. Sites: `ffi.rs:3653-3685` against `arrops.rs:752-800`; `ffi.rs:3687-3719`, `3721-3754`, `3756-3793`. Consolidation: `Context::array_extend(out, ptr, count)` used by concat, slice, splice, and the spreads.
- Rm22: Linear-probe bucket insertion is written twice. Sites: `assocops.rs:456-470` (`rehash`) and `assocops.rs:519-533` (`compact_entries`). Consolidation: `bucket_insert(buckets, cap, hash, entry)`.
- Rm23: Set-algebra key iteration is written three ways. Sites: `assocops.rs:975-983` (`ordered_key_copy`) against `iteration_entry` / `iteration_copy` (`1217-1268`); `set_is_disjoint_from` (`1169-1199`) repeats `set_all_in` (`1130-1145`) with the negated predicate. Consolidation: one `ordered_keys(source)` iterator; `any_in` / `all_in` on it.
- Rm24: Key-kind decoding falls back differently at five sites. Sites: `assocops.rs:405`, `641`, `969` use `KeyKind::from_u32(..).unwrap_or(KeyKind::Bits)`; `assocops.rs:695` and `831` return a miss. `new` validates the kind once (`assocops.rs:130-137`). Consolidation: `fn header_kind(h) -> KeyKind` (the stored value is valid by construction) used everywhere.
- Rm25: `index_of`, `last_index_of`, `includes` are one loop with a direction and an equality function. Sites: `arrops.rs:479-560`. Consolidation: `position(ctx, h, x, kind, reverse, eq)`.
- Rm26: `splice` and `shift` shrink the array by popping into a discarded buffer once per removed element. Sites: `arrops.rs:849-853`, `arrops.rs:887-889`. Consolidation: `Context::array_truncate(handle, new_len)`.
- Rm27: Two subject-reading mechanisms and four budget-trap blocks in `regexops`. Sites: `text_from_handle` (`regexops.rs:187-209`, used by `replace`, `replace_all`, `split`, `new`) against `text_parts` + `text_from_parts` (`211-243`, used by `test`, `search`); the budget trap at `regexops.rs:284-292`, `491-499`, `558-566`, `583-591`. Consolidation: one `subject(ctx, handle, what, pos)` and one `budgeted_find(ctx, compiled, text, at, pos) -> Option<Option<CaptureMatch>>` (`find_and_record` at `271-296` already has the shape).
- Rm28: `Worker::post` and `outbox_post` have the same zero-size / null / `queue.post` body; `materialize_parent` is an alias of `materialize`. Sites: `worker.rs:213-229` against `worker.rs:395-421`; `worker.rs:422-424`. Consolidation: `Queue::post_fixed(payload: *const u8) -> PostResult`; delete the alias.
- Rm29: Two checks compare a record against the expression that produced it. Sites: `context.rs:2108` (`debug_assert_eq!(released_class_id, class_id)` reads the same header word at `2088` and `2031`/`1959`); `context.rs:2838` (`debug_assert_eq!(written, len)` where every writer returns the `len` it received: `ffi.rs:1198`, `ffi.rs:1826` via `pad_into` returning `out.len()` at `strops.rs:245`, `ffi.rs:2151`, `context.rs:2848`). Consolidation: drop the writer's return value and the assert; drop the second class-id read.
- Rm30: Three observer setters share the set-or-clear-userdata body. Sites: `context.rs:1347-1354`, `1445-1452`, `1456-1466`. Consolidation: one generic `set_observer(slot, userdata_slot, observer, userdata)`.
- Rm31: Receiver liveness is checked per wrapper, not per family. Sites: every Map/Set entry checks (`ffi.rs:448-1030`); of the array entries only `array_byte_range` (`3608`) and `array_push` (`3646`) check; `array_pop` (`3795`), `array_ptr` (`3811`), and every `arr_*` entry (`3859-4690`) do not. Consolidation: the check belongs in the `Context` array primitives (`array_len`, `array_elem_ptr`, `array_push`, `array_pop`) once, or in one wrapper macro.
- Rm32: `subscript_rt_str_method_concat` (`ffi.rs:1601-1610`) is a second symbol for `subscript_rt_str_concat`. Consolidation: the code generators emit one symbol.

## MINOR pass — runtime (landed `79ea54b`)

Rm5–Rm32 all done. 1,317 lines removed, 1,053 added. Symbols removed:
the eight Map/Set pairs became `subscript_rt_assoc_size/has/delete/clear`;
`subscript_rt_str_method_concat` deleted (§78 rule 2). `array_len`,
`array_pop`, `array_ptr`, and every `arr_*` entry now check receiver
liveness with the existing dev-tier trap kind and message (Rm31).
Clippy 7/18/13. Gate on main: 1,142 passed, 0 failed.
