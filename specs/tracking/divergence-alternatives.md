# Tracking: divergence alternatives (§307 and later units)

Origin: an owner request of 2026-11-15: a form that `tsc` accepts and
this compiler rejects shows the form to write instead, if one exists.
The owner decided on 2026-11-15 that the alternative stays in the
`= subscript:` block, and that the work goes in five units, in this
order: 1 (§307, a program or a stated no-form for each variant), 3 (a
hint in the `TscRejects` approximation messages), 4 (wrong
"TypeScript accepts" claims), 5 (the embedder `Display` drops the
block), 2 (an example of the form that each site rejects, §182.4
item 1).

## Survey at `23e6f607`

Method: a copy of the tree at `23e6f607` with two survey tests that
walk each rejection site, check each witness, record the site that
fires, and render the diagnostic. The debug CLI checked each program
fragment. `tsc` 5.9.2 ran on probes outside the repository.

## Counts (measured)

| Item | Count |
|---|---|
| Rejection sites (`all_sites()`: 80 API rows + direct + general) | 1081 |
| `Diverges` | 738 |
| `TscRejects` | 340 |
| `ProgramInput` | 3 |
| Sites with no witness file (Unreachable/General guard) | 69 (10 Diverges, 56 TscRejects, 3 ProgramInput) |
| `Divergence::ALL` variants | 512 |
| Variants that a site emits ("carried") | 507 |
| Dead variants (no site emits them) | 5: AsyncReturnMissingArgument, AsyncReturnArgumentCount, ReferenceSearchMiss, GeneratorDoneValue, EntryParameterType |
| Variants whose `subscript` is a program | 170 (165 single-file; all accepted by the HEAD CLI except the dead ReferenceSearchMiss and GeneratorDoneValue, which have unbound names) |
| Variants whose `subscript` is `no equivalent; <text>` | 342 |
| `diagnostic.example` override entries (diagnostic_text.rs + the §176 generator entry) | 13; all 9 program overrides accepted by the HEAD CLI |

### Variant classes

| Class | Variants | Meaning |
|---|---|---|
| a | 170 (167 carried) | `subscript` is a program; the checker and the CLI accept it |
| a-instr | 202 | guidance text that names the accepted form ("use let or const", "use a named import") |
| b | 54 | no source form exists (44 are `regenerate the mirror from its C header`; build options; `Math.hypot`; frame/layout limits) |
| c | 84 (+2 dead) | guidance that names no form, or names forms of other variants |
| d (overlay) | 66 variants, 184 sites | a program example whose TS form the site does not reject; 136 of these sites also have no alternative in the message |

### Diverges sites by what the output shows

| Block | Example fits | Example of another form |
|---|---|---|
| a (program) | 190 (53 also in message) | 184 (48 also in message) |
| a-instr | 215 | 3 |
| b | 54 | 7 |
| c | 84 (5 also in message) | 1 |

Sites with no alternative anywhere in the output: 80 (c, no message alternative) + 136 (d, no message alternative) = 216 of 738 `Diverges` sites (upper bound for d).

### Message text (Q4, heuristic)

| Class | Message names an alternative | Note line only | Neither |
|---|---|---|---|
| Diverges (728 with witness) | 136 | 2 | 590 |
| TscRejects (284 with witness) | 26 | 1 | 257 |

### Wrong "TypeScript accepts" claim (measured from the recorded witness tsc codes)

- 9 `Diverges` sites where every witness is tsc-rejected: Api(DateLocalGetYear), Api(FormGroupBy), ContextBytesTypeCount, ContextBytesArgumentCount, NumberMethodArgumentCount, ArrayFromArgumentCount, NumberPredicateCount, JsonParseTypeCount, ForOfKeys.
- 169 `Diverges` sites have at least one tsc-rejected witness beside a tsc-accepted one (for example CallSpread: `take(...xs)` with a non-rest parameter is TS2556).
- 0 `TscRejects` sites have a tsc-accepted witness. The approximation programs below are not in the witness table.

## Class (c) variants (84 live)

| Shared text | Variants | Measured concrete form (HEAD CLI and tsc accept) |
|---|---|---|
| use Math.pow, an explicit comparison, or a null check | ExponentOperator, InOperator, UnaryPlusOperator, VoidOperator, TypeofOperator, MetaPropertyExpression, ClassExpression, TaggedTemplateExpression, CommaExpression, InstantiationExpression, AngleAssertionExpression, NonPlaceAssignmentTargetForm, NonNullAssertionExpressionForm | `Math.pow(2, 3)`; comma in a `for` update as two statements; others need per-variant text (`x as T`, a module class, separate statements); MetaProperty has no form (b) |
| name the declared type | FunctionTypeObjectPattern, FunctionTypeArrayPattern, FunctionTypeRestParameter, UnknownAnnotation, NeverAnnotation, TemplateLiteralAnnotation, BigIntLiteralAnnotation, BooleanLiteralAnnotation, NumberLiteralAnnotation, StringLiteralAnnotation, PredicateAnnotation, MappedAnnotation, IndexedAnnotation, OperatorAnnotation, ConditionalAnnotation, StructuralAnnotation, QueryAnnotation, ThisAnnotation, IntersectionTypeAnnotationForm, GeneratorYieldTypeMissingForm, QualifiedSourceTypeNameForm | per form: a declared class for `{ value: i32 }`, a string-literal union alias for `"a"`, the element type for `typeof x`, `Generator<T>`; several have no form (b) |
| convert to the integer or use a template | EnumToInteger, StringAliasAssertion, IntegerEnumAssertion, IdentityAssertion, BinaryStringOperand, BinaryEnumOperand, CompoundStringOperand, CompoundEnumOperand | `return e as i32;`, `` `a${1}` ``, `s += `${1}``, `(E.A as i32) + 1` |
| use an explicit admitted operation | EnumMethod, FunctionMethod, BooleanMethod, EnumMember, FunctionMember, BooleanMember | `` `${E.A}` ``, `` `${b}` `` |
| use the admitted type or operation | ArrayToFixedArray, NullableClassAssertion, BigIntAnnotation, SymbolAnnotation, TupleAnnotation, SourceNamespaceDeclaration | per form (a class with two fields for a tuple; a null check for `c as C`) |
| use the identical type | ErasedAssignableTypeMismatch, FunctionParameterIdentity, CollectionCallbackTypeMismatchForm, DistinctNominalContainerAssignmentForm | `xs.map((x: i32): f64 => x as f64)` |
| use an admitted X member/method | MapObjectMember, SetObjectMember, NumericObjectMember, WorkerObjectMethod, FixedArrayObjectMethod, NumericObjectMethod, StringObjectMember, MapObjectMethod, SetObjectMethod, ArrayObjectMember | the witnesses are `valueOf`/`constructor`; the message lists the surface for some |
| use if, await in the body, or a const head | SwitchDiscriminantKindForm, ForOfBindingKindForm, AsyncForOfForm | one sentence for three forms |
| use TypeScript private or a static generator method | AutoAccessorDeclaration, StaticBlockDeclaration, GeneratorMethodForm | one sentence for three forms |
| use an admitted conversion or call / value / constructor; name an admitted type | LibGlobalCall, LibGlobalValue, LibConstructorName, LibTypeName | `` `${1}` `` for `String(1)`, `parseFloat("1")` for `Number("1")` |
| text of another form | CoroutineReturnOrThrowCallForm, GeneratorResultValueWriteForm, GeneratorResultDoneWriteForm, GeneratorMember, FixedArrayConstantIndexBoundsForm, ConstructorTypeAnnotationForm | the text fits a sibling variant; the GeneratorResultDoneWriteForm witness is `/x/.test` |

The shared `why` texts have the same defect: BinaryStringOperand says "an assertion does not check membership"; EnumMethod says "a C function pointer has no properties".

## TscRejects approximation sites (Q3)

A `TscRejects` diagnostic never renders a block (the §154 test fails if it does), so only the message or the note can carry the alternative.

| Site | tsc-accepted program (measured unless marked) | Message names an alternative | Accepted alternative (measured) |
|---|---|---|---|
| NullableMember (S011) | `if (h.f !== null) { while (true) { print(`${h.f.x}`); h.f = null; break; } }`; the same with a local `f` | "narrow with a null check first": the program has the check; the step does not fix it | `const f = h.f;` before the loop; for a local, `const g = f;` inside the check |
| NullableCall | `if (cb !== null) { while (true) { cb(1); cb = null; break; } }` | no ("is not callable") | `const c = cb;` inside the check |
| NullableNominalAssignment (S005) | `if (f !== null) { while (true) { use(f); f = null; break; } }` | no | const copy |
| MethodValueTruthTest | `if (c.ready) { c.ready(); }`, `c.ready ? c.ready() : false` (tsc TS2774 fires only when the body does not call it) | no | call the method; drop the test |
| IncompatibleNominalAssignment (S005) | `const p: P<i32> = new Q<i32>(1)` with structurally equal generic classes; §186.4 item 1 (inferred) | no | construct the declared class |
| AssignmentTypeMismatch | `const fs: ((a: i32, b: i32) => void)[] = hs` with `hs: ((a: i32) => void)[]`; the same through `| null` | no | not measured (a lambda per element) |
| DisjointLiteralAliasAssignment | `if (a === "a") { const b: B = a; }` with `A = "a"/"b"`, `B = "a"/"c"` | no | `const b: B = "a";` |
| FunctionReturnPathMissing | only as a cascade after another rejection (`switch (b)` on boolean); 7 standalone probes (`do`, `try/finally`, `for(;;)`, `while (true)`, enum and alias `switch`, `unreachable()`) are accepted | no ("not all paths return a value") | n/a |
| LocalReadUnassigned | `if (b && (x = 1) > 0) { x }` (§158.4 item 2, contrived) | yes (initializer) | initializer |
| GenericInferenceIncompatibleKinds / RequiredArgumentMissing | no tsc-accepted probe found; `pair(i32, f64)` goes to a Diverges site | yes ("use explicit type arguments") | — |
| Field*UnassignedExit, ConstructorFieldReadUnassigned | tsc-accepted shapes go to Diverges sites with blocks (§158.4 item 4) | yes | — |
| Immediate*/SwitchCaseRead/ClassReadBeforeDeclaration | a closure that reads a later `const` goes to a Diverges site whose example is the shadowing case (class d) | no | move the declaration first |

TscRejects sites with a tsc-accepted program and no named alternative: NullableMember (named step is wrong for the program), NullableCall, NullableNominalAssignment, MethodValueTruthTest, IncompatibleNominalAssignment, AssignmentTypeMismatch, DisjointLiteralAliasAssignment, FunctionReturnPathMissing (cascade only).

## Output modes (Q5)

- CLI `check`, `run`, `build`/`emit`, and `watch` all print `render_diagnostics`, which prints the block (`cli/src/lib.rs` `rejection()` and `write_watch_step`).
- `impl Display for Diagnostic` (`compiler/src/diag.rs`) prints `pos: message [code]` only. `RunError`, `ReloadError` (`codegen/src/jit.rs`, `codegen/src/reload.rs`) and `EmitCFilesError` (`codegen/src/emit_files.rs`) format their diagnostics through it, so a Rust embedder that prints these errors loses the rule, the note, and the block.
- `render_items` prints `example` only inside the `divergence` branch, so an example on a `TscRejects` diagnostic is dropped. All current example overrides sit on `Diverges` sites, so no output is lost today.
- The render stops after 200 items (`MAX_ITEMS`).
- No JSON, LSP, or tsserver-plugin output exists in the tree.

