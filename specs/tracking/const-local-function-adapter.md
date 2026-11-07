# A const local function value takes an adapter that captures it — `compiler.md` §246

Status: IN PROGRESS.

## Red at the pin

The debug CLI built from the code of the pin (`ab3b0aca`; the contract
commit `4dfb6d5e` changed specs only), run on `$TMPDIR` copies:

`subscript check a432-a-const-local-function-value-takes-an-adapter-that-captures-it.ts`
gave 10 errors, then `error: 10 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 32:7 | `run(fa)` in `later`, after an `await` | S100 type mismatch: the argument expects `(i32, i32) => void`, got `(i32) => void` |
| 34:18 | `const h: CB2 = fa;` in `later` | S100 type mismatch: the initializer expects `(i32, i32) => void`, got `(i32) => void` |
| 44:18 | `const h: CB2 = fv;` | the same |
| 46:7 | `run(fv);` | S100, the argument kind |
| 51:19 | `const h1: CB1 = fr;` | S100 type mismatch: the initializer expects `(i32) => void`, got `(i32) => i32` |
| 56:13 | `m.forEach(fv);` | S014 `Map.forEach` callback `fv` is a function value with fewer parameters than the place; an adapter for it needs a new environment, which no rule gives an owner; use a lambda that calls it |
| 57:7 | `run(G);` with a module `const G: CB1` | S100, the argument kind |
| 61:7 | `run(plain);` with a const local that captures nothing | S100, the argument kind |
| 62:11 | `run(b ? fv : both);` | S100 type mismatch: the then branch expects `(i32, i32) => void`, got `(i32) => void` |
| 63:18 | `[8, 9].forEach(b ? fv : both);` | S100 conditional branches have no common type: `(i32) => void` and `(i32, i32) => void` |

`subscript check r495-a-parameter-function-value-takes-no-adapter.ts`
gave one S100 "type mismatch: the argument expects `(i32, i32) =>
void`, got `(i32) => void`" at 15:7.

## After the change

`a432` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`fv 15 fv 11 fr 17 fv 11 fv 12 G 1 plain 1 fv 11 fv 18 fv 19 handler 20
handler 21 picked 1 picked 22 fa 51 fa 53`, one per line. `main` is async, so the LIR text snapshot grows by the
`a432` block.

`r495` gives one S100 "function value `fv` needs an adapter for this
place, and only a const local or a module `const` takes one; copy it
into a const local, or write a lambda that calls it" at 15:7. `tsc`
5.9.2 accepts `r495`.

## Form

- `compiler/src/check/type_rules.rs`. `adapter_callee` gives the callee
  of an adapter: a function declaration or a static method, a const
  local of a function type (captured by value, as a lambda written in
  the source captures it), or a module-level `const` of a function type
  (no capture). `adapt_function_reference`, the one builder of §240 and
  §243, builds the lambda for each: the parameters of the place, a call
  through `Callee::Value` of the local or the constant, the result rule
  of §240 rule 2, and the capture list. The capture analysis and S009
  read this lambda as they read a lambda written in the source, so the
  escape forms give the same diagnostic.
- The capture of an adapter comes from `lookup::local_capture`, the
  helper that gives a written lambda its capture: the storage type of
  the local, not a narrowed type. A narrowed nullable const local takes
  the adapter in a user place, a built-in callback, and an initializer.
- Rule 5 uses the decision of rule 1: `adapter_shape` gives the fit of
  the adapter for both, and rule 5 checks the adapted type against the
  place with `assignable`, as the value-taking check of §240 rule 2a
  does. A nullable value that is not narrowed fits no adapter, so it
  gets the type mismatch of the place.
- The conversion of a function value to its place takes the function
  context (`place_value`, `place_function_value`,
  `discard_function_result`, `place_callback_value`,
  `place_callback_leaves`, `expect_callback_shape`), so it reads whether
  a local is const.
- Rule 5: `needs_adapter_name` decides whether a value that is not a
  name of rules 1 and 2 fits the place with an adapter. In a user place
  the new site `FunctionValueAdapterNeedsConstName` (S100) gives "function
  value `fv` needs an adapter for this place, and only a const local or
  a module `const` takes one; copy it into a const local, or write a
  lambda that calls it". In a built-in callback the site
  `CallbackFewerParametersUnadapted` (S014) gives the same text after the
  method and "callback". Each site has its divergence, and the
  divergence of `CallbackFewerParametersUnadapted` now cites §246.
- The witness `s244-environment` has a parameter (rule 4) and the text
  of rule 5. The new site has the witnesses `s246-parameter` and
  `s246-let-local`; `tsc` 5.9.2 accepts both forms.
- The cells of the generic `tsc` matrix with a parameter of a function
  type in a `FixedArray` or `Map.groupBy` callback name the record
  `compiler.md §246`.
- The §240, §243, and §244 unit tests that pinned the rejection of a
  const local, or the former text, now accept the const local and pin
  the text of rule 5.

## Files

- `compiler/src/check/type_rules.rs`, `compiler/src/check/lookup.rs`,
  `compiler/src/check/expr/callback_shape.rs`,
  `compiler/src/check/expr/method.rs`,
  `compiler/src/check/expr/operator.rs`,
  `compiler/src/check/expr/logical_assign.rs`,
  `compiler/src/check/expr/promise_reaction.rs`
- `compiler/src/check/rejection_sites.rs`,
  `compiler/src/check/rejection_surface_classes.rs`,
  `compiler/src/check/rejection_witness_index.rs`,
  `compiler/src/check/rejection_witnesses.txt`,
  `compiler/src/divergence.rs`, `compiler/src/divergence/entries.rs`,
  `compiler/src/divergence/established_tail.rs`
- `compiler/tests/const_local_function_adapter.rs` (new),
  `compiler/tests/corpus_reject.rs`,
  `compiler/tests/built_in_callback_fewer_parameters.rs`,
  `compiler/tests/fewer_parameters_function_type.rs`,
  `compiler/tests/named_function_void_place.rs`,
  `compiler/tests/generic_tsc_matrix/api.rs`
- `corpus/accept/a432-a-const-local-function-value-takes-an-adapter-that-captures-it.ts`
  and `.expected`,
  `corpus/reject/r495-a-parameter-function-value-takes-no-adapter.ts`
- `codegen/tests/lir-goldens/corpus.txt` (the `a432` block),
  `generated-docs/corpus-index.md`
