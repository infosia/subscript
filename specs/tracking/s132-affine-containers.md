# §132 — An affine type is no container argument in any form

Contract: `specs/blocks/compiler/s132-an-affine-type-is-no-container-argument-in-any-form.md`.

## Red (acceptance 1)

Entry: `corpus/reject/r281-affine-new-map.ts`. Binary: `subscript check`, built from `d0ee1a0` before the change.

```
check: r281-affine-new-map.ts: no errors
exit=0
```

`tsc` 5.9.2 with `prelude/lang.d.ts`, the `corpus/interop` ambient files, and the `tsconfig.json` options of `compiler/tests/tsc_corpus.rs`: exit 0, no diagnostics.
The header states `tsc: accepts` and cites `Q35`, as r111 does.
The first diagnostic carries `Divergence::WorkerContextAffinity`, so the §79 block is present.

Other forms at `d0ee1a0` (function body, `class Message { value: i32 = 0; }`):

| Form | Diagnostics at `d0ee1a0` |
|---|---|
| `new Map<Worker<M, M>, i32>()` | S014 |
| `new Map<i32, Inbox<M>>()`, `new Map<i32, Outbox<M>>()` | none |
| `new Set<Worker<M, M>>()` | S014 |
| `const m: Map<i32, Worker<M, M>> = new Map<i32, Worker<M, M>>();` | S100, S005 |
| `const s: Set<Worker<M, M>> = new Set<Worker<M, M>>();` | S100, S014, S005 |
| `const a: Worker<M, M>[] = [worker];` (§40.1 unit test form) | S100, S100 |
| `const a: Worker<M, M>[] = [];` | S100, S100 (empty literal without context) |
| r111 | S100, S005, S014 (`get` on `Map<i32, <error>>`) |
| generic field `Map<i32, T> = new Map<i32, T>()`, `T = Worker<M, M>` | S100, S005 |
| generic return `Map<i32, T>` from `new Map<i32, T>()`, `T = Worker<M, M>` | S100, S005 |

After the change, each form above gives exactly one S100. Rule 2a extends this claim to every consumer of the poisoned type (table in "Absorption (rule 2a)").

## Where the check lives (rule 1)

`compiler/src/check/container_argument.rs`: `Checker::container_argument(slot, argument, pos, context)`.
It applies `is_context_affine_type`. On a hit it reports the §40.1 S100 and returns `Type::Error`.
The container type forms with the poisoned argument (rule 2).

Formation sites that call it:

| Source | Site |
|---|---|
| Annotation `T[]`, `Array<T>`, `FixedArray<T, N>` | `check/tyres.rs` |
| Annotation `Map<K, V>`, `Set<T>` | `check/tyres.rs` |
| Generic substitution | `check/tyres.rs` (a generic body resolves its annotations with the parameter bound) |
| `new Map<K, V>()`, `new Set<T>(…)` | `check/expr/call.rs` |
| `new Map<K, V>(source)` | `check/expr/array_of_and_map_copy.rs` |
| Inferred array literal element, first inferred element of a spread literal | `check/expr/aggregate.rs` |
| `Array.from<T>` type argument, `Array.from` element, `map` result element | `check/expr/method.rs` |
| `Map.groupBy` key | `check/expr/method.rs` |

A field, a parameter, a return type, and a module global resolve through the annotation site.
The array message and its absence of a divergence block do not change (rule 3).

## Poisoning (rule 2)

- `container_argument` returns `Type::Error` for an affine argument. The container constructor then gives `Type::Error` (rule 2a), so the whole annotation or construction is the error type.
- `Checker::in_poisoned_context` suppresses the report. `enter_container_context(ctx)` sets it when the contextual type is `Type::Error`, around the type arguments of `new Map`, `new Set`, `new Map(source)`, and the first inferred element of a spread literal. The flag reaches nested arguments: `const m: Map<i32, Map<i32, W>> = new Map<i32, Map<i32, W>>();` gives one S100.
- The flag does not reach a generic instance. `instantiate_class`, `instantiate_fn`, and `instantiate_method` clear it for the shape and body check, and restore it after. The instance is cached, so a body checked under the flag reported nothing for every later use. With `class G<T> { v: i32 = 0; run(): void { const m = new Map<i32, W>(); m.clear(); } }`, the program `const x: Nope = new Map<i32, G<i32>>(); const y = new G<i32>(); y.run();` gave only S016; it now gives S016 and S100 at 3:63, the same S100 as the program without the `Nope` line. The field form `class G<T> { a: W[] = []; }` gave S016 only; it now gives S016 and S100.
- An array literal with the contextual type `Type::Error` checks each element with the context `Type::Error`, so a nested container reports nothing more and an element still reports its own errors. `const a: W[] = [];` gives one S100. `const a: Map<i32, W>[] = [new Map<i32, W>()];` gave two S100 and now gives one. `const a: W[][] = [[]];` gave S100 and "cannot infer the type of an empty array literal" and now gives one S100. `const a: Nope[] = [new Map<i32, Nope2>(), 1];` gives S016 for each name.
- An error type argument gives no class instance (rule 2a), but the counts do not depend on the argument types. `instantiate_class` compares the type-argument count before the error guard, and `new` of a generic class with an error type argument compares the constructor argument count against the template declaration (`template_constructor_arity`). `new G<Nope, i32>()` gives S016 and "`G` expects 1 type argument(s), got 2"; `new G<Nope>(1, 2, 3)` gives S016 and "`G` expects 1 argument(s) (1 required), got 3". HEAD `31760eb` reported both pairs; the first rule 2a round lost the S100 of each.
- An assignment and a declaration treat a `Type::Error` target differently. An assignment to a target of `Type::Error` checks the value without context (`compiler/src/check/expr/assign.rs`), so the value reports its own affine argument: `let x: Nope = 1; x = new Map<i32, W>();` gives S016 and S100. A declaration passes the `Type::Error` annotation as the context, so rule 2 suppresses the affine report: `const y: Nope = new Map<i32, W>();` gives S016 only. Neither form is a false accept: each program holds the S016 of the `Nope` annotation. The declaration reads an error annotation as a context that reported its failure, because an affine annotation (`const m: Map<i32, W> = new Map<i32, W>();`) gives the same context and must report once.
- A spread literal admits its element type at the first inferred element, so a later element compares against `Type::Error`. `[w, ...xs]` with `xs: i32[]` gives one S100 at `w`; before, it gave S100 "type mismatch" and then S100 at the literal.
- `Array.from<T>` admits `T` at the type argument, so the source does not report a mismatch against an affine `T`. `Array.from<W>(xs)` with `xs: i32[]` gives one S100 at the type argument; before, it gave S100 "type mismatch" and then S100 at the call.

## Absorption (rule 2a)

The composite type constructors are in `compiler/src/types.rs`: `Type::array`, `fixed_array`, `map`, `set`, `worker`, `inbox`, `outbox`, `nullable`, `generator`, `async_handle`, `iter_result`, `func`. Each returns `Type::Error` when a component is `Type::Error`. Every construction of a composite type in `compiler/src/check/` calls them.

Exceptions and other forms:

- `check/signatures.rs`: a generator signature with no `Generator<T>` annotation keeps `Type::Generator(Error)`. The `Error` there is a placeholder for an unknown yield type that `yield_known` marks, not a poisoned component. `bodies.rs` replaces it after the body is checked.
- `check/generics.rs` `instantiate_class`: an argument that is `Type::Error` gives no instance, and the instance type is `Type::Error`. `new G<Nope>(…)` checks its arguments without context and gives `Type::Error`.
- The rebuilds in `check/expr/call.rs` and `check/type_rules.rs` that only name an existing type for a message do not construct a new type.
- `check/expr/method.rs` Set algebra (`union`, `intersection`, …) checked its argument with the contextual type `Type::Error` as a placeholder for "no context". An array literal under that context now reports nothing, so `r53` was accepted. The argument is now checked with no contextual type; `r53` gives its S014 as before.
- `Map.get` has no poisoned-value case: a `Map` type cannot carry a `Type::Error` value.

Measured through `subscript check` (class `M`, `W = Worker<M, M>`, receiver `a: W[]` unless noted). "Before" is the §132 tree before rule 2a; "after" is this tree.

| Consumer | Before | After |
|---|---|---|
| `a.filter/map/forEach/find/some/every/findIndex/reduce/indexOf/includes/sort/join/lastIndexOf` (13 forms) | S100, S014 each | S100 each |
| `` `${a}` `` | S100, S100 | S100 |
| `a === a` | S100, S100 | S100 |
| `const ys: i32[] = xs`, `xs: Array<Worker<M, M>>` | S100, S100 | S100 |
| `JSON.parse<W[]>("[]")` | S100, S014 | S100 |
| message class with a `W[]` field, used by `Worker.spawn` | S100, S100 | S100 |
| `a: Array<Array<W>>`, `a[0].filter(…)` | S100, S014 | S100 |
| `m: Map<i32, W>`, `m.forEach(…)` | S100, S100 | S100 |
| `s: Set<W>`, `s.forEach(…)` | S100, S100 | S100 |
| `const m: Map<i32, W> = new Map<i32, string>();` | S100, S005 | S100 |
| `const m: Map<i32, Map<i32, W>> = new Map<i32, Map<i32, W>>();` | S100, S100 | S100 |
| `a: Nope[]`, `a.filter(…)` | S016, S014 | S016 |
| `a: Nope[]`, `` `${a}` `` | S016, S100 | S016 |

Each tabled form gives exactly one diagnostic.

A construction under a poisoned context still reports its own errors: `const m: Map<i32, W> = new Map<i32, Nope>();` gives S100 and S016. A context that is `Type::Error` for another reason also suppresses the affine report: `const m: Nope = new Map<i32, W>();` gives S016 only. The affine argument reports when the program fixes the annotation.

## Unit tests (acceptance 2 and 3)

`compiler/src/tests/collections.rs`:

- `q35_affine_type_arguments_of_new_report_exactly_one_diagnostic`: `new Map` key and value, `new Set`, for `Worker`, `Inbox`, `Outbox`, as a declaration and as a statement. Each gives one S100 at the argument, with the divergence. Control: the same constructions with a message class are accepted.
- `q35_annotated_affine_container_reports_exactly_one_diagnostic`: annotated `Map` key, `Map` value, `Set`, nullable `Map`, empty array. Control: the message-class forms are accepted.
- `q35_r111_program_reports_exactly_one_diagnostic`: r111 gives one S100 at 19:25.
- `q35_context_affinity_rejects_all_four_escape_positions` and `q35_context_affinity_rejects_every_container_type_argument` now assert exactly one diagnostic.

The surface has no type alias for a non-literal type (`type W = Worker<M, M>;` gives S100 "type aliases are limited to a union of two or more string literals"), so acceptance 2 has no alias form.

`compiler/tests/function_value_reference.rs`: `map_get_rejects_a_worker_value` pinned the §123 rule 8 open item (`new Map<i32, Worker<M, M>>()` accepted, then S014 on `get`). It is now `map_get_on_a_worker_value_reports_only_the_affine_argument`: one S100 at the type argument, with a message-class control.

`compiler/src/tests/poisoned_containers.rs`:

- `q35_every_consumer_of_a_poisoned_container_reports_exactly_one_diagnostic`: each consumer of the rule 2a table on `W` gives one S100 with the §40.1 message; the filter, interpolation, assignment, and `Map.forEach` forms also on `Nope` give one S016. Control: the same consumer on `i32` is accepted, or reports its real error (`find` and `sort` S014; interpolation, equality, assignment to `string[]`, transferability, and both `forEach` callback mismatches S100; `new Map<i32, string>()` for `Map<i32, i32>` S005).
- `q35_poisoned_context_suppresses_only_the_affine_argument`: `new Map<i32, Nope>()` under a poisoned context gives S100 and S016. Control: `new Map<i32, Map<i32, W>>()` under the context `Map<i32, string>` gives one S100 at the nested argument.
- `q35_affine_formation_sites_report_the_section_40_1_diagnostic`: the `map` result, the `Array.from<T>` type argument, the `Map.groupBy` key, an inferred array literal, and a spread literal each give one S100 with the exact §40.1 message and position. Control: the message-class forms are accepted.
- `q35_annotated_affine_container_reports_exactly_one_diagnostic` asserts the message.
- The consumer test holds two more forms: `new Map<i32, W>()` in an array literal with the context `Map<i32, W>[]`, and `[[]]` with the context `W[][]`. Each gives one S100; the `i32` control is accepted.
- `q35_poisoned_context_does_not_reach_a_generic_instance_body`: the method-body and field forms of `G<T>` above, each with and without `const x: Nope = new Map<i32, G<i32>>();`. The poisoned program gives the control's S100 at the same position, and one S016.
- `q35_error_type_argument_keeps_the_arity_diagnostics`: `new G<Nope, i32>(1)`, `new G<Nope>(1, 2, 3)`, and the annotation `G<Nope, i32> | null` each give S016 and the count S100; `new G<Nope>(1)` gives S016 only. Controls: `new G<i32, i32>(1)` and `new G<i32>(1, 2, 3)` give the count S100 only; `new G<i32>(1)` is accepted.
- `q27_set_algebra_argument_reports_exactly_one_diagnostic`: the r53 form `values.union([1, 2])` gives one S014, `values.union()` one S100, and `values.union(...xs)` one S014. Control: `values.union(new Set<i32>())` is accepted. The Set-algebra argument takes no contextual type; a spread argument goes to `check_args` with one parameter of `Type::Error`, as at HEAD.

With the instance-context clear removed from `generics.rs`, `q35_poisoned_context_does_not_reach_a_generic_instance_body` fails. With the array-literal elements checked without context, the consumer test fails at the array-literal form.

`compiler/src/types.rs`: `composite_constructors_absorb_the_error_type` tests each constructor with and without an error component.

With absorption removed from the constructors (each guard deleted, then restored), `q35_every_consumer_of_a_poisoned_container_reports_exactly_one_diagnostic` fails at its first consumer: `filter` gives S100 and S014 "`<error>` elements are outside that set".

No existing test expectation and no golden changed.

## Other changes

- `compiler/tests/corpus_reject.rs`: row `("r281-affine-new-map.ts", RuleCode::S100, 13)`.
- `compiler/src/language_reference.rs`: r281 in the Q35 feature list; `generated-docs/` regenerated.
- `corpus/reject/r281-affine-new-map.ts`: the `exercises:` tag is `function-local-affinity`; the entry holds a function-local `Map`, not a module global.
- No `.expected` golden moves. No Rust file passes 2,000 lines (largest touched: `compiler/src/tests/collections.rs`, 1,874).

## Gate

`tools/gate.sh full`, one run, after rule 2a:

```
gate full 31760ebb84e5ce3364465ba83e3b55a0d8c74e52 dirty:28 debug 1980/0/3 release 1977/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

`tools/gate.sh full`, one run, after the second Phase Review fixes:

```
gate full 31760ebb84e5ce3364465ba83e3b55a0d8c74e52 dirty:28 debug 1983/0/3 release 1980/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```
