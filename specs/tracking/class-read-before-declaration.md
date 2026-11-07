# A class read before its declaration is rejected — `compiler.md` §241

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code of the pin (`01be4d7e`; the contract
commit `c715b0bf` changed specs only):

| Entry | Output |
|---|---|
| `r493-class-read-before-its-declaration.ts` | `check: …: no errors`, exit 0 |
| `r494-enum-read-before-its-declaration.ts` | `check: …: no errors`, exit 0 |

`tsc` 5.9.2 gives TS2449 at 9:19 for `r493` and TS2450 at 8:14 for
`r494`.

## After the change

`r493` gives one S100 "`B` is used before its declaration" at 9:19.
`r494` gives one S100 "`E` is used before its declaration" at 8:14.
No accept entry and no golden changes.

## Form

- `compiler/src/check/lookup.rs`, `scope_item`, which resolves each
  value use of a module name, now takes the function context. It
  rejects a class, a generic class, or an enum of this file (site
  `ClassReadBeforeDeclaration`) when the use is before the position of
  the declared name and no frame of the context is deferred.
- `Frame::deferred` is the fact of §241 rule 2. A function, a method, a
  getter, a setter, a constructor, an instance field initializer, a
  parameter default, a descriptor default, and a lambda set it. A
  lambda that is the callee of a call is not deferred
  (`lambda_called_at_once`, set in `check_call_inner`).
  `DeferredWork::Lambda` carries that fact, so a lambda with a result
  annotation that the checker defers while it decides a binding type
  (`const s = ((): i32 => B.k(7))();`) is called at once too.
- `declaration_starts` holds the position of the name of each class,
  generic class, and enum of a program file. An ambient declaration is
  not recorded.
- The witness program `class-read-before-declaration` holds the form of
  `r493`.

A Phase Review found that the parameter types of an unannotated lambda
were decided in a frame cloned from the outer frame, so a default such
as `(x = B.k(1)): i32 => x` took the outer evaluation and was rejected.
Each frame now states its evaluation where it is built: `FnCtx::new`
takes an `Evaluation` (`AtOnce` or `Deferred`), and
`decide_lambda_parameters` takes the evaluation of the lambda body. The
called-at-once form `((x = B.k(1)): i32 => x)()` stays rejected.

A second Phase Review found that a `const enum` read before its
declaration was rejected, while `tsc` accepts it. A `const enum` is not
recorded in `declaration_starts`. `subscript run` of
`const c: C = C.X;` before `const enum C { X, Y }` prints `true`, as the
`tsc` output under `node` does. The unit test adds a firing control for
each deferred member form of rule 2: parameter defaults of a function, a
method, and a constructor, a getter, a setter, a static method, and a
descriptor field default.

## Files

- `compiler/src/check/lookup.rs`, `mod.rs`, `pipeline.rs`,
  `declarations.rs`, `bodies.rs`, `initializer.rs`
- `compiler/src/check/expr/lambda.rs`, `call.rs`, and each caller of
  `scope_item`: `exception.rs`, `expr/assign.rs`, `expr/entry.rs`,
  `expr/literal.rs`, `expr/namespace.rs`
- `compiler/src/check/rejection.rs`, `rejection_sites.rs`,
  `rejection_witness_index.rs`, `rejection_witness_sites.rs`,
  `rejection_targets.txt`, `rejection_programs.txt`
- `compiler/tests/class_read_before_declaration.rs`,
  `compiler/tests/corpus_reject.rs`
- `corpus/reject/r493-class-read-before-its-declaration.ts`,
  `corpus/reject/r494-enum-read-before-its-declaration.ts`
- `generated-docs/corpus-index.md`

## Gate

One run after the implementation, and one after each Phase Review fix:

- `gate quick 210c90ef dirty:26 debug 3262/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick c678f9e4 dirty:27 debug 3263/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick c715b0bf dirty:27 debug 3265/0/5 skips 2 goldens-moved 0 exit 0`

The first two pins are earlier forms of the contract commit, before a
Phase Review amended it.

## Phase Review

One MAJOR and two MINOR findings were fixed, and one MINOR finding was
recorded, in §241.3. Open items are in §241.4. Hygiene: clean.
