# §135 — Opaque check of generic bodies

Contract: `specs/blocks/compiler/s135-a-generic-body-is-checked-for-every-type-argument.md`.

## Step 0 measurement at `95ddf50f`

This round built the opaque check as a prototype, measured it, and reverted it.
Nothing from the prototype is in the tree.

### Prototype

- The opaque type is `Type::Class` of a fresh empty reference class, one per type parameter.
  The class has no field, no method, and no constructor. Its source name is the parameter name.
- A new pass runs after pass C, in the final checker run only (not in the provisional run).
  It calls `instantiate_fn`, `instantiate_class`, and `instantiate_method` with opaque arguments for:
  every non-rejected generic function template (async functions included),
  every generic class template (its members included),
  and every non-rejected instance and static generic method of every non-generic class (async included).
  A generic method of a generic class is rejected before this pass (§82.4 rule 5), so the pass does not see one.
- After the pass, the checker truncates `functions`, `classes`, `class_sigs`, `type_handle_classes`,
  each existing class's `methods`, and `pending_layouts` to their lengths before the pass,
  and removes each `class_ids` entry of a removed class.
  No opaque instance reaches the HIR.
- Rule 3 (prototype form): the pass drops a diagnostic that is an exact duplicate
  (message, file, line, column) of an earlier diagnostic.
  It does not remove a duplicate that the per-instance check makes between two concrete instances.
  With `gf<i32>` and `gf<string>` over a body that calls an unknown name,
  the output was 2 errors at `95ddf50f` and 2 errors with the prototype.
- Size: 112 lines in `compiler/src/check/generics.rs`, 3 lines in `compiler/src/check/pipeline.rs`.

### Corpus measurement

Sources: every `corpus/accept`, `corpus/warn`, and `corpus/trap` entry, with directory entries and the
mirrors that the harnesses add (`codegen/tests/corpus/mod.rs` `entry_sources`, plus the trap interop mirror),
and every `examples/e*.ts`, `examples/gate/two-header-binding.ts`, `examples/context-per-scene/scene.ts`,
and `examples/hot-reload/demo.ts`, with the engine and interop mirrors.

- Sources checked: 379 programs. At `95ddf50f`, 0 are rejected.
- The pass checked 27 templates (11 functions, 9 classes, 7 methods) in 16 source files. No corpus or example source declares a generic generator.
- With the prototype, 2 programs are rejected:

| Source | Diagnostic | `tsc` | Class |
|---|---|---|---|
| `corpus/accept/a12-generics-mono.ts` | 12:3 S100 field type `T (a12-generics-mono.ts)` is outside the value-class whitelist (sized numerics, boolean, value classes, FixedArray, enums) | exit 0 | (b) a field of type `T` in a `@ValueType` generic class |
| `corpus/accept/a178-generic-method.ts` | 38:15 S100 type `A` cannot be interpolated into a template; 38:23 S100 type `B` cannot be interpolated into a template | exit 0 | (b) template-literal interpolation of a `T` value |

`tsc` ran with the `tsc_corpus.rs` options and `prelude/lang.d.ts`, one project per file.
No rejected source is class (a): both headers that state `tsc: accepts` are correct.

Both operations are language restrictions that `tsc` does not have, and the answer depends on the argument:
`Box<i32>` is a legal value-class field and a class type is not;
`${i32}` is legal and `${SomeClass}` is not.
Rule 2 does not say whether the opaque check allows such an operation and leaves it to the per-instance check.

### Rule 2 probes

Each body is `function g<T>(x: T, y: T): void { ... }` with a `main` that does not call `g`.

| Body | Prototype | `tsc` |
|---|---|---|
| `const a: T = x; let b: T = y; b = a;` | accepted | accepts |
| `x === y`, `x !== y` | accepted | accepts |
| `const arr: T[] = [x, y]; arr.push(x); arr[0]` | accepted | accepts |
| `new Map<T, i32>()`, `m.set(x, 1)` | accepted | accepts |
| `new Map<string, T>()`, `m.set("k", x)` | accepted | accepts |
| `new Set<T>()`, `s.add(x)` | accepted | accepts |
| `const n: T \| null = null;` | accepted | accepts |
| `` `${x}` `` | S100 type `T` cannot be interpolated into a template | accepts |
| `x + y` | S100 operator not defined for `T` and `T` | TS2365 |
| `x as i32` | S100 `as` converts between sized numerics, ... cannot convert `T` to `i32` | accepts |
| `x < y` | S100 operator not defined for `T` and `T` | accepts |
| `print(x)` | S100 type mismatch: the argument expects `string`, got `T` | TS2345 |

Three rows differ from `tsc` in the direction the contract names as a question: interpolation, `as` to a numeric, and `<`.
The opaque type is a reference class, so the prototype also allows the reference-only forms
(`T | null`, a `Map` key of any argument) and rejects the value-only forms (a value-class field).
Each of these is an argument-dependent language rule that rule 2 must name.

### Measured shapes

Each shape ran through `subscript check`, release CLI.

| Shape | `95ddf50f` | Prototype | `tsc` |
|---|---|---|---|
| `function gf<T>(x: T): i32 { return nope(); }` | no errors | S016 unknown function `nope` at 1:36 | TS2304 |
| `const k: i32 = 1; function gf<T>(x: T): i32 { k = 2; return 0; }` | no errors | S100 cannot rebind `const` binding `k` at 2:29 | TS2588 |
| `function gf<T>(x: T): i32 { const s: string = 5; return 0; }` | no errors | S100 type mismatch: the initializer expects `string`, got `i32` at 1:47 | TS2322 |
| `class G<T> { v: i32 = 0; run(): void { this.v = "a"; } }` | no errors | S100 type mismatch: the assignment expects `i32`, got `string` at 1:49 | TS2322 |
| `function gf<T>(x: T): i32 { return x.v; }` | no errors | S018 `T` has no member `v` at 1:38 | TS2339 |
| the same `gf` with `class B { v: i32 = 5; }` and `gf<B>(new B())` | no errors | S018 `T` has no member `v` at 2:38 | TS2339 |

### Cost

`check_program` over the 379 programs, release build, 5 runs, median:

| Build | Median (s) |
|---|---|
| `95ddf50f` | 0.1656 |
| Prototype | 0.1667 |

The prototype adds 0.7% to the checker time on the corpus.

### Result

Rule 4 applies: 2 sources are rejected, both class (b). The round stops for the owner decision.

## Implementation round at `152c3c78`

Rule 2 as corrected on 2026-09-30.

### File size (§5.y rule 2a)

No changed Rust file passes 2,000 lines. The largest changed files: `compiler/src/check/mod.rs` 1,765 lines,
`compiler/src/check/stmt.rs` 1,513 lines, `compiler/src/check/expr/operator.rs` 1,343 lines. No split.

### Form

- The opaque type is a fresh empty reference class per type parameter, as in the prototype.
  `Checker::opaque_params` holds the ids while the check runs.
- `compiler/src/check/opaque.rs` holds the pass. It runs after pass C, in the final checker run only.
  It checks every non-rejected generic function, every generic class, and every non-rejected generic method of a
  non-generic class, in source order.
- A nested instance whose type arguments carry an opaque type resolves its signature and class shape only.
  The template's own opaque check checks its body. Only the root instance of each check has its body checked.
  Consequence: a body that instantiates itself at a larger argument (`g<T[]>` inside `g<T>`) terminates.
- After the pass, the checker restores `functions`, `classes`, each class's `methods`, `class_sigs`, `class_ids`,
  `fn_sigs`, `instance_symbols`, `type_handle_classes`, `pending_layouts`, `globals`, `global_sigs`,
  `regex_literals`, `worker_entries`, `top_level`, and the three class-id sets. No opaque instance reaches the HIR.
- Rule 3: each per-instance check records its diagnostic range. After the pass, a per-instance diagnostic at a
  site that the opaque check reports is removed, and the opaque diagnostics of that site take its place.
  A per-instance diagnostic that repeats an earlier one (code, message, position) is removed.
  A site that one template check reports is not reported again by a later template check.

### Sites that defer to the per-instance check

Each site consults `Checker::defers_to_instance`, which is true only for an opaque type parameter type itself.

| Site | Restriction | `tsc` on an unconstrained `T` |
|---|---|---|
| `compiler/src/check/expr/literal.rs`, template check | `${x}` | accepts |
| `compiler/src/check/expr/operator.rs`, `bin_result` | `<`, `<=`, `>`, `>=` with a `T` operand | accepts `x < y`; TS2365 for `x > 1` |
| `compiler/src/check/expr/operator.rs`, `bin_result` | `===`, `!==` with a `T` operand | accepts |
| `compiler/src/check/expr/operator.rs`, `bin_result` | `+` with a `T` operand and a `string` operand | accepts |
| `compiler/src/check/expr/operator.rs`, `check_as` | `as` from or to `T` | accepts |
| `compiler/src/check/expr/operator.rs`, `check_unary` | unary `-`, `!`, `~` on `T` | accepts |
| `compiler/src/check/expr/operator.rs`, `check_bin` | `&&`, `\|\|` with a `T` operand | accepts |
| `compiler/src/check/expr/operator.rs`, `check_cond` | `x ? a : b` | accepts |
| `compiler/src/check/stmt.rs`, `require_bool` | `if`, `while`, `do`, `for` condition of type `T` | accepts |
| `compiler/src/check/expr/operator.rs`, `check_nullish` | `x ?? y` with `x: T` | accepts |
| `compiler/src/check/expr/assign.rs`, `check_assign` | `s += x` with `s: string`, `x: T` | accepts |
| `compiler/src/check/class_shape.rs`, `value_field_ok` | a `T` or `FixedArray<T, N>` field of a `@ValueType` class | accepts |

No site defers for a container key kind or for `T | null`: the opaque type is a reference class, so the
opaque check accepts both, and the per-instance check decides them as before.

The opaque check still reports, in the rule 2 list: a member read (TS2339), a method call, a call of the value,
a binary arithmetic or bitwise operator (`+` without a string operand TS2365, `-`, `*`, `/`, `%`, `<<`, `&` TS2362),
`++` and `--` (TS2356), and a mismatch between `T` and another type (TS2322, TS2345).
It also reports every rule that does not depend on `T` (`==`, `typeof`, `sort()` without a comparator,
`instanceof` with a non-Error class, a string field of a value class).

### Re-measurement under the corrected rule 2

Same source set as Step 0, same harness, release build.

- Sources checked: 379 programs. Rejected: 0 at `152c3c78`, 0 with the opaque check.
- The pass checked 27 templates in 13 programs.

### Red entries

Each entry ran through `subscript check`, release CLI built at `152c3c78`, and through `tsc` with the
`tsc_corpus.rs` options and `prelude/lang.d.ts`.

| Entry | `152c3c78` | This round | `tsc` |
|---|---|---|---|
| `r283-generic-body-unknown-name` | `check: r283-generic-body-unknown-name.ts: no errors` | S016 unknown function `nope` at 8:10 | TS2304 at 8:10 |
| `r284-generic-body-const-assignment` | `check: r284-generic-body-const-assignment.ts: no errors` | S100 cannot rebind `const` binding `limit` at 10:3 | TS2588 at 10:3 |
| `r285-generic-class-method-type-mismatch` | `check: r285-generic-class-method-type-mismatch.ts: no errors` | S100 type mismatch: the assignment expects `i32`, got `string` at 11:18 | TS2322 at 11:5 |
| `r286-generic-body-member-on-type-parameter` | `check: r286-generic-body-member-on-type-parameter.ts: no errors` | S018 `T` has no member `v` at 12:16 | TS2339 at 12:16 |

### Changed test expectation

`compiler/tests/field_values.rs` `a_generic_class_is_checked_per_instance` expected two diagnostics at one site
for two instances of one template. Rule 3 gives one. The test now expects one.

### Cost

`check_program` over the 379 programs, release build, 5 runs per round, median, two rounds:

| Build | Round 1 (s) | Round 2 (s) |
|---|---|---|
| `152c3c78` | 0.1681 | 0.1679 |
| This round | 0.1664 | 0.1666 |

The two builds come from two directories. The difference (-1.0 %) is inside the build-to-build variance;
the opaque check adds no measurable cost on the corpus.

### Known limits

- The passes that run over the module after the checker (`capture::check`, the module-initializer check,
  the layout validation) do not see an opaque instance. The loop narrowing effects come from the provisional run,
  which does not run the opaque check, so an uninstantiated generic body has no loop effects.
- A concrete polymorphic recursion (`g<T[]>` inside `g<T>`, with `g<i32>` called) overflows the checker stack at
  `152c3c78` and with this round. The opaque check does not add this failure: its nested instances are signature-only.

## Phase Review fix round at `f65f1590`

Contract: rules 1, 2a, 2b, and 3 as amended on 2026-09-30.
Each probe ran through `subscript check`, release CLI, and through `tsc` with the `tsc_corpus.rs` options and
`prelude/lang.d.ts`. "Before" is the tree at the start of this round (the previous round's uncommitted work).
"Pin" is a release CLI built from `git archive f65f1590`.

### Form

- `Checker::opaque_params` maps each opaque type to an `OpaqueType`: its constraint (rule 2b), `fresh`
  (a result type that matches no type parameter), and `numeric` (the `number` result of unary `-` or `~`).
  A fresh opaque type is a new empty reference class, as a type parameter is. The restore removes it.
- `Checker::defers_to_instance` is true for every opaque type: a type parameter, constrained or not, and a fresh type.
- An opaque type prints as its type parameter name, or as the `tsc` spelling of the result (`number`,
  `false | T`, `T | Box | null`). The class-name disambiguation (`T (file.ts)`) does not count opaque types.

### Fixes

| Finding | Before | After | `tsc` |
|---|---|---|---|
| `x > 1`, `1 < x`, `x >= n` (`n: i32`), `x < 1.5`, `x < k` (`k: i64`), `x < Color.Red`, `x < u` | no errors | S100 operator not defined for `T` and `i32` (and the matching pair) | TS2365 |
| `x === u` | no errors | S100 operator not defined for `T` and `U` | TS2367 |
| `x as U` | no errors | S100 `as` ... cannot convert `T` to `U` | TS2352 |
| `const v: boolean = b \|\| x` (`g<boolean>`) | no errors | S100 ... expects `boolean`, got `boolean \| T` | TS2322 |
| `const v: boolean = b && x` | no errors | S100 ... got `false \| T` | TS2322 |
| `const v: boolean = x \|\| true`, `x \|\| b` | no errors | S100 ... got `T \| boolean` | TS2322 |
| `const v: T = -x` (`g<i32>`) | no errors | S005 ... expects `T`, got `number` | TS2322 |
| `const v: T = x ?? bx` (`g<Box \| null>`) | no errors | S005 ... expects `T`, got `T \| Box \| null` | TS2322 |
| `const v: i32 = -x; const w: i32 = ~x;` (`g<i32>`) | S100 ... expects `i32`, got `T` (2) | no errors | accepts |
| `const v: T = x && y; const w: T = x \|\| y;` (`g<boolean>`) | S100 ... expects `T`, got `boolean` (2) | no errors | accepts |
| `x.join(",")` on `x: T[]` (`g<i32>`) | S014 `T` elements are not interpolatable | no errors | accepts |
| `g<T extends Box>(x: T): i32 { return x.v; }` with `g<Box>` | S018 `T` has no member `v` | no errors | accepts |
| `g<T extends i32>` called as `g<string>("s")` | no errors | S100 type argument `string` does not satisfy the constraint `i32` of `T` at 4:34 | TS2344 at 4:34 |
| the loop-narrowing shape in an uninstantiated body | no errors | S011 `Box \| null` may be null here at 3:111 | TS18047 at 3:111 |
| `const v: i32 = -x;` with `g<string>` | S100 ... expects `i32`, got `T` (opaque) | S100 unary `-` requires a numeric operand, got `string` (instance) | accepts |

At the pin, every "Before: no errors" row above is also accepted, and the three regression rows are accepted.

### Sites that defer to the per-instance check

Each site consults `defers_to_instance`. The result type is the type of the expression in the opaque check.

| Site | Form | Result type in the opaque check | `tsc` on an unconstrained `T` |
|---|---|---|---|
| `expr/literal.rs`, template check | `` `${x}` `` | `string` | accepts |
| `expr/method.rs`, array `join` | `a.join(s)` with `a: T[]` | `string` | accepts |
| `expr/operator.rs`, `bin_result` | `x < y`, `x < s`, `x < b`, `x < k` with `k` a class or a fresh type (all four relational operators) | `boolean` | accepts |
| `expr/operator.rs`, `bin_result` | `===`, `!==` with a `T` operand, except between two type parameters | `boolean` | accepts |
| `expr/operator.rs`, `bin_result` | `+` with a `T` operand and a `string` operand | `string` | accepts |
| `expr/operator.rs`, `check_as` | `as` from or to `T`, except between two type parameters | the target type | accepts |
| `expr/operator.rs`, `check_unary` | `-x`, `~x` | the contextual numeric type, else the fresh `number` | accepts (`number`) |
| `expr/operator.rs`, `check_unary` | `!x` | `boolean` | accepts |
| `expr/operator.rs`, `check_bin` | `a && b` with an opaque operand | `b`'s type when `a` is opaque; else fresh `false \| B` | accepts |
| `expr/operator.rs`, `check_bin` | `a \|\| b` with an opaque operand | `T` when both operands are `T`; else fresh `A \| B` | accepts |
| `expr/operator.rs`, `check_nullish` | `x ?? y` with an opaque `x` | `T` when `y: T`; else fresh `T \| Y` | accepts |
| `expr/operator.rs`, `check_cond` | `x ? a : b` | the arm type | accepts |
| `stmt.rs`, `require_bool` | `if`, `while`, `do`, `for` condition of type `T` | none | accepts |
| `expr/assign.rs`, `check_assign` | `s += x` with `s: string` | `string` | accepts |
| `class_shape.rs`, `value_field_ok` | a `T` or `FixedArray<T, N>` field of a `@ValueType` class | none | accepts |

The fresh `number` type takes the type of a numeric operand beside it in a binary operator
(`-x + 1` is `i32`), is assignable to every sized numeric type, and is an error beside an opaque
operand in a relational operator (`-x < y`, `tsc` TS2365).

The opaque check reports, beside the rule 2 list: a relational operator with a sized numeric, an enum,
or a different type parameter beside `T` (TS2365); `===`/`!==` between two type parameters (TS2367);
`as` between two type parameters (TS2352); and every assignment of a result type above that `tsc` rejects (TS2322).

### Constraints (rule 2b)

- A template reads each constraint from its own declaration (`ast::TsTypeParam::constraint`); no template
  record changed. The constraint resolves in the template's file, after every parameter of the instance is bound,
  so a constraint can name an earlier parameter.
- The root instance of an opaque check records the constraint on its opaque type parameter type.
- Every other instance (a program instance, or a nested instance inside an opaque body) checks each argument
  against its constraint with `assignable`. A failed argument is S100 at the type argument, and the instance
  body is not checked. `instantiate_fn`, `instantiate_method`, and `instantiate_class` take the type-argument
  positions from each call site; the opaque roots pass none.
- A member read, a method call, an index, a call of the value, a unary operator, and a binary operator see the
  constraint in place of a constrained `T` (`apparent_type`). The logical operators and `??` read the written types.
- `assignable(T, C')` is `assignable(C, C')` for a constrained `T`. A `C` value is not assignable to `T`.
- `===`/`!==` and `as` between two different type parameters are errors, constrained or not, unless one is
  constrained to the other. The operand names in the message are the written ones (`T` and `U`).
- A mismatch between an opaque type and a class type keeps S005 without the class-identity divergence note:
  an opaque type is not a class of the program.

Measured shapes, each with `tsc`:

| Shape | This round | `tsc` |
|---|---|---|
| `T extends Box`: `x.v = 3; const b: Box = x; x.v + x.get() + take(x)` with `g<Box>` | no errors | accepts |
| `T extends Box`: `const t: T = b` (`b: Box`) | S005 ... expects `T`, got `Box` | TS2322 |
| `T extends i32`: `x < 2`, `x + 1` | no errors | accepts |
| `run<T extends Box>` called as `run<Other>` | S100 at the type argument | TS2344 |
| `new G<Other>(...)` for `G<T extends Box>` | S100 at the type argument | TS2344 |
| `G<i32>` in a parameter type | S100 at the type argument | TS2344 |
| `g<T>(x)` inside `h<T>` for `g<T extends Box>` | S100 type argument `T` does not satisfy the constraint `Box` of `T` | TS2344 |
| the same inside `h<T extends Box>` | no errors | accepts |
| `T extends Box, U extends Box`: `x === u`, `x as U` | S100 | TS2367, TS2352 |

`class Other { v: i32 = 2; }` passed to `run<T extends Box>` is S100 in this language and accepted by `tsc`
(structural typing, collisions.md C1); the tests use a class with a different shape.

### Loop narrowing (rule 1)

The opaque check runs in both checker runs. Before the restore, the provisional run collects the loop effects
of every body that the opaque check added (`Checker::opaque_body_loop_effects`) and `run` merges them into the
analysis of the final run (`Analysis::merge_loops`). `x ?? y` with an opaque `x` keeps both operands in its node,
so their effects reach the analysis. This removes the "Known limits" item of the previous round about loop effects.

### Rule 3

The merge removes a per-instance diagnostic at an opaque site only when an opaque diagnostic of that site has the
same code. Measured: `const v = x * 2;` with `g<f16>` reports S014 (instance) and S100 (opaque) at 1:39;
with `g<string>` it reports the one S100. `tsc` TS2362 at 1:39 for both.

### Red entry

| Entry | Pin | This round | `tsc` |
|---|---|---|---|
| `r287-type-argument-outside-constraint` | `check: r287-type-argument-outside-constraint.ts: no errors` | S100 type argument `string` does not satisfy the constraint `i32` of `T` at 12:14 | TS2344 at 12:14 |

### Re-measurement

Same source set and harness as Step 0 (the trap interop mirror added for `t72`), release build.

- Sources checked: 379 programs. Rejected: 0.
- `check_program` over the 379 programs, 5 runs per round, median: pin 0.1691 s and 0.1674 s;
  this round 0.1668 s and 0.1667 s. The opaque check in both runs adds no measurable cost on the corpus.

### Tests

`compiler/tests/opaque_generics.rs`:

- `PAIRS` pins the result types `T` of `x && y`, `x || y`, `x ?? y`, `i32` of `-x` and `~x`, and `join` on `T[]`.
- `each_rule_2a_form_follows_tsc`: 21 rows, each a `tsc`-clean form with no diagnostic and a `tsc`-rejected form
  with its exact one diagnostic.
- `a_member_read_through_a_constraint_is_accepted`, `a_constraint_value_is_not_assignable_to_the_type_parameter`,
  `a_type_argument_outside_its_constraint_is_rejected` (function, method, construction, type annotation,
  nested type parameter), each with its control.
- `a_loop_in_an_uninstantiated_body_ends_a_narrowing`, with the control without the assignment.
- `an_instance_diagnostic_with_a_different_code_stays`, with the same-code control.

### File size (§5.y rule 2a)

No changed Rust file passes 2,000 lines. The largest: `compiler/src/check/mod.rs` 1,769 lines,
`compiler/src/check/expr/method.rs` 1,540 lines. No split.

### Known limits

- A fresh result type of `||` or `??` beside `T` in a relational operator defers (`(x || y) < x`, `tsc` accepts).
- A constraint that this language cannot resolve is an error at the constraint. No corpus or example source
  declares a constraint.

## Narrowed form round at `93390cad`

Contract: rule 2 as rewritten on 2026-09-30 (a closed list of kept kinds), rule 2b (TS2344 at an instantiation
only), rule 3, and 135.3. Rule 2a of the previous round is gone.

### Form

- The opaque check is unchanged in scope: every generic template, in both checker runs, with the same restore.
  The opaque type is the fresh empty reference class of each type parameter.
- The checker has no operator, result-type, or restriction site that reads the opaque type.
  Each site gives its pre-§135 diagnostic, and the merge drops it unless it is in the kept list.
- `Checker::independent_diagnostics` holds the index of each diagnostic of the running opaque check that is
  in the kept list. The emission site adds the index. The merge (`merge_opaque_diagnostics`) keeps an opaque
  diagnostic only when its index is in this set or its code is S016, and drops every other one.
  No message text is read.
- `Checker::opaque_instances` holds each generic class instance that the opaque check made at an argument that
  involves an opaque type parameter type (`G<T>`). `involves_type_parameter` is true for an opaque type parameter
  type, for such an instance, and for a type that contains either.

### Kept kinds and how each is identified

| Kind | Site | Identification |
|---|---|---|
| Unknown name | every S016 site | the code S016; every S016 message is an unknown name, type name, class, or export |
| Assignment to a `const` binding | `expr/assign.rs`, `check_assign_target`, local and global | `error_independent` at the site |
| Assignment to an import binding | `expr/assign.rs`, `check_assign_target` | `error_independent` at the site |
| Type mismatch | `type_rules.rs`, `require_assignable_with` (S005, S007, S011, S100) | marked when neither `from` nor `to` involves a type parameter |
| Nullable use (S011) | `narrowing.rs`, `nullable_use_error` (receiver of a member access); `expr/array_of_and_map_copy.rs` | marked when the code is S011 and the nullable type involves no type parameter |
| Member read on an unconstrained `T` | `expr/member.rs`, `member_on`, read side only | `error_independent`, S018 `` `T` has no member `v` `` |
| Method call on an unconstrained `T` | `expr/call.rs`, `check_method_call_on` | `error_independent`, S018 `` `T` has no method `m` ``; the arguments are still checked |
| Call of an unconstrained `T` value | `expr/call.rs`, `check_indirect_call` | marked when the callee type is an unconstrained type parameter |

`OpaqueType::constrained` records whether a type parameter has a constraint. The root instance of each
opaque check sets it from the template's declaration.

### Removed

- Rule 2a: `fresh_opaque_type`, `OpaqueType::fresh` and `numeric`, `opaque_numeric_result`,
  `opaque_number_operands`, `logical_result_type`, `opaque_nullish_result`, `opaque_relational_defers`,
  `distinct_type_parameters`, and the nullish early return in `check_nullish`.
- The apparent-type plumbing: `apparent_type`, `apparent_expr`, `opaque_constraint`, the stored constraint type,
  and the constraint rule in `assignable`.
- Every deferral site: `defers_to_instance` in `class_shape.rs`, `expr/assign.rs`, `expr/literal.rs`,
  `expr/method.rs`, `expr/operator.rs` (unary, logical, `bin_result`, `check_cond`, `check_as`), and `stmt.rs`.
  These files are back to their `93390cad` text, except the kept-kind marks above.
- The S005 class-identity exception for an opaque type in `require_assignable_with`.
  A mismatch that involves a type parameter is dropped, so the exception has no reader.

Kept: the opaque check in both checker runs with the loop effects of its bodies, the type-argument positions and
the TS2344 check at a program instantiation, `instance_body_is_checked`, the rule 3 merge with the different-code
rule, the instance dedupe, and the state restoration.

### Re-measurement

Same source set as Step 0, same harness (the trap interop mirror included), debug build of this round.

- Sources checked: 379 programs. Rejected: 0.

### Red entries

Each entry ran through `subscript check` built from this round.

| Entry | This round | Header |
|---|---|---|
| `r283-generic-body-unknown-name` | S016 unknown function `nope` at 8:10 | TS2304 |
| `r284-generic-body-const-assignment` | S100 cannot rebind `const` binding `limit` at 10:3 | TS2588 |
| `r285-generic-class-method-type-mismatch` | S100 type mismatch: the assignment expects `i32`, got `string` at 11:18 | TS2322 |
| `r286-generic-body-member-on-type-parameter` | S018 `T` has no member `v` at 12:16 | TS2339 |
| `r287-type-argument-outside-constraint` | S100 type argument `string` does not satisfy the constraint `i32` of `T` at 12:14 | TS2344 |

`r286` instantiates `read<Box>`. The instance accepts the member read. The opaque check keeps it as a member read
on an unconstrained `T`, and rule 3 reports it once. No header changed.

### Changed test expectations

- `compiler/tests/field_values.rs` `a_generic_class_is_checked_per_instance` is back to its `93390cad` text:
  two instances of one template report the missing-initializer rule twice. The opaque check drops that rule,
  and the two instance messages name two instances, so the instance dedupe keeps both.
- `compiler/tests/opaque_generics.rs`:
  - `each_kept_kind_is_reported_in_an_uninstantiated_body`: each kept kind with its one diagnostic, and a control
    of the same shape through a type parameter that the opaque check drops.
  - `an_assignment_to_an_import_binding_is_kept`, with a local of the same name as the control.
  - `each_dropped_form_is_accepted`: every form that the two Phase Reviews measured as `tsc`-valid and rejected
    by the previous round, with its instantiation where the review named one. Each row has a firing control:
    the same program with `nope();` in the generic body reports one S016.
  - `a_form_that_tsc_rejects_for_a_type_parameter_is_left_to_the_instance` (`x > 1`), with an instance control.
  - `a_language_restriction_in_an_uninstantiated_body_is_left_to_the_instance` (a string field of a value class).
  - `a_type_argument_outside_its_constraint_is_rejected`: function, method, construction, type annotation, each
    with a satisfied control. The nested case (`g<T>` inside `h<T>`) is removed: the opaque check drops it.
  - `a_loop_in_an_uninstantiated_body_ends_a_narrowing`, unchanged.
  - `an_instance_diagnostic_with_a_different_code_stays`: `x.toFixed` with `g<f64>` gives S014 (instance) and
    S018 (opaque) at 1:41; `x.v` with `g<i32>` gives the one opaque S018.
  - `a_diagnostic_inside_a_body_instantiated_twice_is_reported_once`, unchanged.
  - The rule 2a tables (`PAIRS`, `RULE_2A`) and the constraint-typing tests are removed.

### Residual gap (135.3)

Each form below is accepted by this round in a body that the program does not instantiate, or whose every
instance accepts it. The `tsc` codes are from the Phase Review fix round table above.

| Form | This round | `tsc` |
|---|---|---|
| `x > 1` (uninstantiated, and with `g<i32>`) | no errors | TS2365 |
| `x + 1` (uninstantiated) | no errors | TS2365 |
| `x === u` with `g<i32, i32>` | no errors | TS2367 |
| `x as U` with `g<i32, i32>` | no errors | TS2352 |
| `const v: boolean = b \|\| x` with `g<boolean>` | no errors | TS2322 |
| `const v: T = -x` with `g<i32>` | no errors | TS2322 |
| `const t: T = b` for `T extends Box`, `b: Box`, with `g<Box>` | no errors | TS2322 |
| `g<T>(x)` inside an uninstantiated `h<T>` for `g<T extends Box>` | no errors | TS2344 |
| `x.v = 3` on an unconstrained `T` (a member write) | no errors | TS2339 |
| `const s: string = x` (uninstantiated) | no errors | TS2322 |
| `this.v = this.w` for `v: i32`, `w: T` in `class G<T>` (uninstantiated) | no errors | TS2322 |
| `x.v` on `x: T \| null` for `T extends Box` (uninstantiated) | no errors | TS18047 |
| `x()` for `T extends Box` (uninstantiated) | no errors | TS2349 |

Language rules that do not depend on `T` are also left to an instance in an uninstantiated body:
`x == y` (S100), a string field of a `@ValueType` generic class, a field with no initializer.
`tsc` accepts `==` and the value-class field, and gives TS2564 for the field with no initializer.

### File size (§5.y rule 2a)

No changed Rust file passes 2,000 lines. The largest: `compiler/src/check/mod.rs` 1,774 lines,
`compiler/src/check/expr/method.rs` 1,538 lines.

## Phase Review fix round at `a3afa605`

The contract amended rule 2b (the constraint relation and each site) and rule 3 (one diagnostic per code,
message, and position). The residual gap table above has the four rows of the review.

### Changes

- Rule 2b relation: `apply_type_parameter_constraints` calls `satisfies_constraint` (`compiler/src/check/generics.rs`).
  It is `assignable(numeric_normal_form(argument), numeric_normal_form(constraint))`.
  `numeric_normal_form` replaces every numeric type (`is_numeric`: the sized integers, `f16`, `f32`, `f64`) with
  `f64`, the type of `number`, at any depth: arrays, fixed arrays, `Map`, `Set`, nullable, function types, worker,
  generator, and async types.
  A generic class instance becomes `normal_instance`: the first existing instance of its template whose type
  arguments have the same normal form. The check makes no instance.
  `same_normal_form` compares two type argument lists by one walk over both types. It does not search the
  instances of a nested template: a first form that normalized every candidate's arguments recursed without end
  on `W<W<u16>>` against `W<W<i32>>` (stack overflow, measured).
  The new `Checker::instance_arguments` records the template key and the type arguments of each class instance.
  The opaque check removes the records of the classes that it made.
- Rule 2b per site: `instantiate_fn`, `instantiate_method`, and `instantiate_class` call
  `check_constraints_at_site` when the instance already exists. It checks the constraints at the positions of
  that site, and records the diagnostic range as a per-instance range, so the rule 3 merge removes an exact repeat.
- Rule 3: `merge_opaque_diagnostics` (`compiler/src/check/opaque.rs`) keeps one opaque diagnostic per code,
  message, and position with a hash set, also inside one template check.
  The per-instance dedupe uses a hash set of the same key in place of a `Vec` search.
  The opaque diagnostics of a site are found through a site index, not a filter over every opaque diagnostic.

### Probes

`subscript check`, debug CLI. Before: the tree at the start of this round. After: this round.

| Program | Before | After |
|---|---|---|
| `f<T>` calls `g<i32>(1) + g<f64>(1.0)`; `g<U>` returns `nope()`; no instance of `f` | S016 at 2:35, twice | S016 at 2:35, once |
| The same, with `const s: string = 5;` in `g` | S100 type mismatch at 2:46, twice | once |
| `k32<u8>`, `k32<f64>` for `T extends i32`; `k64<i32>` for `T extends i64`; `kf<i32>` for `T extends f64` | 4 S100 (TS2344 form) | no errors |
| `keep<u8[]>` for `T extends i32[]`; `Map<string, i32>` for `Map<string, f64>`; `(a: u8) => i32` for `(a: i32) => f64`; `W<u8> \| null` and `W<f64>` for `W<i32> \| null` | 3 S100 (the `W` rows were not measured before) | no errors |
| `keep<string>("a"); keep<string>("b");` for `T extends i32` | S100 at 2:37 only | S100 at 2:37 and 2:56 |
| `const h: H<Box \| null> = new H<Box \| null>()` for `H<T extends Box>` | S100 at 3:43 only | S100 at 3:43 and 3:63 |
| `keep<string>` for `T extends i32` (r287 shape) | S100 | S100, unchanged |
| `keep<boolean[]>` for `T extends i32[]`; `keep<W<string>>` for `T extends W<i32>` | not measured | S100 each |
| `keep<f16>` for `T extends f32` | S100 | no errors |
| `W<u8>` for `W<i32>`; `W<f64>` for `W<i32> \| null`; `W<u8>[]` for `W<i32>[]`; `W<W<u16>>` for `W<W<i32>>` | not measured | no errors |
| `W<boolean>` for `W<i32>`; `W<boolean>[]` for `W<i32>[]` | not measured | S100 each |

Each form that this round accepts is `number`-typed in `tsc`, which sees every numeric alias as `number`.

### Kept kinds pinned

- `String(x)` in an uninstantiated body: S016 `unknown function \`String\`` at 1:38 (a language restriction that
  `tsc` binds). Control: `` `${x}` `` in the same body, no errors.
- The §124 kill: `if (shared !== null) { touch(); return shared.v; }` with a module global `shared: Box | null`
  gives S011 at 4:67 with the C17 note. Controls: no call gives no errors; `x.v` on `x: T | null` after `touch()`
  gives no errors.

### Re-measurement

Same source set and method as Step 0: every `corpus/accept`, `corpus/warn`, and `corpus/trap` entry through
`codegen/tests/corpus/mod.rs` `entry_sources`, the trap interop mirror for `t72`, and the examples with the engine
and interop mirrors. Temporary harness in `codegen/tests/`, debug build, deleted after the run.

- Sources checked: 379 programs. Rejected: 0. Measured again after the normal form relation: 379, 0 rejected.
- `r283` to `r287` give the diagnostics of the "Red entries" table above, unchanged.

### Tests

`compiler/tests/opaque_generics.rs`:

- `a_numeric_type_argument_satisfies_a_numeric_constraint`: the rule 2b accept rows (`f16`, generic instances,
  the mixed shape, nested instances), with four rejected controls.
- `each_site_of_a_repeated_type_argument_list_is_checked`: a function, a class in an annotation and a construction,
  a generic method; control: a repeated satisfied list.
- `an_opaque_check_that_makes_concrete_instances_reports_a_site_once`: the `f`/`g` shape with S016 and with a
  type mismatch, exactly one diagnostic each; control: two sites in `g` give two.
- `a_language_unsupported_name_is_kept_in_an_uninstantiated_body` and
  `a_shared_location_kill_is_kept_in_an_uninstantiated_body`: the kept kinds above.
- `a_form_that_tsc_rejects_for_a_type_parameter_is_left_to_the_instance` and
  `a_language_restriction_in_an_uninstantiated_body_is_left_to_the_instance`: a `nope();` firing control in the
  uninstantiated body gives exactly one S016.
- `KEPT` row 1: the control is `function g<T>(x: T): i32 { return x + 1; }`, the same shape as the kept form.

The checker's `assignable` has no subtyping at depth (a class to `object`, `X` to `X | null`, and `null` to
`X | null` at the top level only), so the one mixed shape is a numeric difference inside an instance with
`X` to `X | null`: `W<f64>` for `W<i32> | null`, accepted.

### File size (§5.y rule 2a)

No changed Rust file passes 2,000 lines. `compiler/src/check/mod.rs` 1,778 lines, `compiler/src/check/generics.rs`
555 lines, `compiler/src/check/opaque.rs` 410 lines.
