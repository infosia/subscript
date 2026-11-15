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


## §307 result

After §307, 428 `subscript` fields are programs and 79 are form b
(`no equivalent: <reason>`). The form b entries and their reasons:


- **AsyncGeneratorFunction**: The runtime has no async iterator or async generator frame.
- **BoundaryObjectMember**: An opaque boundary object has no fields or default object formatter.
- **ClassFinalAlignmentLimit**: The requested aligned layout exceeds the signed displacement limit.
- **ClassRuntimeObject**: Classes have no runtime constructor object or constructor name property.
- **ConditionalAnnotation**: The type system cannot select a parameter type through a conditional type.
- **ContextAffineArrayElement**: Worker endpoints cannot enter array storage.
- **ContextAffineCapture**: A worker endpoint has no closure-environment representation or string formatter.
- **ContextAffineContainerArgument**: Worker endpoints cannot enter container storage.
- **CoroutineReturnOrThrowCallForm**: The generator API has no return or throw operation to force completion.
- **EnumImplicitValueOverflow**: The implicit value 2147483648 cannot fit the i32 enum representation.
- **EnumObjectMember**: An enum has no runtime object or object method value.
- **EnumObjectValueForm**: An enum has no runtime object that holds its members.
- **ErasedAssignableEquality**: Arrays with different element layouts cannot share an identity.
- **FileModuleDisabled**: File access requires build configuration and host file callbacks.
- **FixedArrayConstantIndexBoundsForm**: A fixed array of length one has no element at index two.
- **FixedArrayLengthLiteral**: A C array cannot have a negative length.
- **FixedArrayLengthRange**: The requested fixed storage exceeds the signed aggregate byte limit.
- **FixedArrayObjectMember**: The runtime has no constructor object for a fixed array.
- **ForeignDirectCallback**: A direct foreign callback parameter has no C provenance position; its C API needs a mirrored callback struct.
- **ForeignReturnProvenance**: A foreign string return has no C return provenance; its C API needs a mirrored output struct.
- **GeneratorResultObjectMember**: A generator step has no Object method or default object formatter.
- **GeneratorResultValueWriteForm**: The generator owns its step result; source code cannot overwrite its done flag.
- **GeneratorReturnValue**: A finished generator result has no slot for a return value.
- **InvalidProgramEntry**: The build input must select one non-ambient entry module.
- **MapObjectMember**: The runtime has no constructor object for a Map.
- **MathSubset**: The N-argument Math.hypot result has no admitted operation with the same rounding.
- **MetaPropertyExpression**: The runtime has no module metadata object.
- **MirrorAnonymousCallback**: A callback needs a named C typedef for its trampoline cast. Generate the mirror again from its C header.
- **MirrorArrayProvenanceMissing**: An absorbed array parameter needs its C aggregate or scalar-pair provenance for C emission. Generate the mirror again from its C header.
- **MirrorCallbackProvenanceMissing**: A callback needs its recorded C typedef identity for its trampoline cast. Generate the mirror again from its C header.
- **MirrorCallbackTargetMissing**: Callback provenance must name a callback typedef declared in the same mirror. Generate the mirror again from its C header.
- **MirrorHeaderMissing**: Foreign functions need a header identity for C emission. Generate the mirror again from its C header.
- **MirrorLifetimeTargetMissing**: A callback lifetime record must name a boundary class that carries a callback field. Generate the mirror again from its C header.
- **MirrorMemberTargetMissing**: A member record must name a field of a boundary class of its mirror. Generate the mirror again from its C header.
- **MirrorModuleDeclarationForm**: C header declarations have no namespace identity.
- **MirrorParameterProvenanceMismatch**: The declared parameter type must agree with its recorded C parameter shape. Generate the mirror again from its C header.
- **MirrorParameterTargetMissing**: Parameter provenance must name a parameter declared in the same mirror. Generate the mirror again from its C header.
- **MirrorStringProvenanceMissing**: An absorbed string parameter needs its C string-view aggregate identity for C emission. Generate the mirror again from its C header.
- **NamespaceAsValue**: A namespace import has no runtime object value.
- **NeverAnnotation**: The type system has no uninhabited parameter type.
- **NullableValueClassAssignment**: A value class has no callable identity or nullable function representation.
- **ProvenanceControlCharacter**: A provenance string must escape control characters. Generate the mirror again from its C header.
- **ProvenanceDuplicateCEnum**: One CEnum typedef has one provenance record. Generate the mirror again from its C header.
- **ProvenanceDuplicateCallback**: One callback typedef has one provenance record. Generate the mirror again from its C header.
- **ProvenanceDuplicateCallbackLifetime**: One callback aggregate has one lifetime record. Generate the mirror again from its C header.
- **ProvenanceDuplicateDescriptor**: One parameter has one recorded C shape. Generate the mirror again from its C header.
- **ProvenanceDuplicateExternalType**: One external type has one provenance record. Generate the mirror again from its C header.
- **ProvenanceDuplicateHeader**: A mirror has one header identity. Generate the mirror again from its C header.
- **ProvenanceDuplicateMember**: One boundary-struct member has one member record. Generate the mirror again from its C header.
- **ProvenanceDuplicatePointerParameter**: One pointer parameter has one pointer-parameter record. Generate the mirror again from its C header.
- **ProvenanceDuplicateScalarPair**: One parameter has one recorded C shape. Generate the mirror again from its C header.
- **ProvenanceDuplicateStringView**: One parameter has one recorded C shape. Generate the mirror again from its C header.
- **ProvenanceEmptyCEnum**: A CEnum record needs nonempty typedef and alias identities. Generate the mirror again from its C header.
- **ProvenanceEmptyCallback**: A callback record needs a nonempty C typedef identity. Generate the mirror again from its C header.
- **ProvenanceEmptyCallbackLifetime**: A callback lifetime record needs a nonempty aggregate identity. Generate the mirror again from its C header.
- **ProvenanceEmptyDescriptor**: A descriptor record needs nonempty function, parameter, aggregate, and element identities. Generate the mirror again from its C header.
- **ProvenanceEmptyExternalType**: An external-type record needs a nonempty C type identity. Generate the mirror again from its C header.
- **ProvenanceEmptyMember**: A member record names a non-empty aggregate and member. Generate the mirror again from its C header.
- **ProvenanceEmptyPointerParameter**: A pointer-parameter record names a non-empty function and parameter. Generate the mirror again from its C header.
- **ProvenanceEmptyScalarPair**: A scalar-pair record needs nonempty function, parameter, and element identities. Generate the mirror again from its C header.
- **ProvenanceEmptyStringView**: A string-view record needs nonempty function, parameter, and aggregate identities. Generate the mirror again from its C header.
- **ProvenanceFieldSeparator**: Whitespace must separate provenance fields. Generate the mirror again from its C header.
- **ProvenanceHeaderBasename**: A header identity must be a nonempty basename without control characters. Generate the mirror again from its C header.
- **ProvenanceInvalidBoolean**: A provenance boolean must use true or false. Generate the mirror again from its C header.
- **ProvenanceInvalidUnicodeDigits**: A Unicode escape must contain four hexadecimal digits. Generate the mirror again from its C header.
- **ProvenanceInvalidUnicodeScalar**: A provenance string must contain Unicode scalar values. Generate the mirror again from its C header.
- **ProvenanceMissingKind**: A provenance record needs a record kind. Generate the mirror again from its C header.
- **ProvenanceShortUnicodeEscape**: A Unicode escape must contain four hexadecimal digits. Generate the mirror again from its C header.
- **ProvenanceTrailingData**: A provenance record must end after its declared fields. Generate the mirror again from its C header.
- **ProvenanceUnexpectedKey**: Each provenance field must use the key required by its record kind. Generate the mirror again from its C header.
- **ProvenanceUnknownKind**: A provenance record must use a declared record kind. Generate the mirror again from its C header.
- **ProvenanceUnquotedString**: A provenance string must use quotes. Generate the mirror again from its C header.
- **ProvenanceUnsupportedEscape**: A provenance string must use a declared escape form. Generate the mirror again from its C header.
- **ProvenanceUnterminatedEscape**: A provenance escape must contain its escaped character. Generate the mirror again from its C header.
- **ProvenanceUnterminatedString**: A provenance string must have a closing quote. Generate the mirror again from its C header.
- **SetObjectMember**: The runtime has no constructor object for a Set.
- **SymbolAnnotation**: The runtime has no Symbol values.
- **WireEnumEmpty**: A CEnum requires at least one member with a C wire value.
- **WorkerStaticObjectMethod**: Worker has no runtime constructor object to return from valueOf.

## §308 result

Each `TscRejects` site of the approximation table names a form in a
note, except `AssignmentTypeMismatch` (not measured) and
`FunctionReturnPathMissing` (a cascade only). A read that the loop head
ends gets a site of its own; a read after a store or a test in the body
keeps the site of the pin.

## §309 measurement of rule 1

Measured at `364301e8` on a clean copy with its own target directory, with `tsc` 5.9.2 on 74 probes.

| Site | Reaching forms / guard | Measured tsc outcome | Decision |
|---|---|---|---|
| Api(DateLocalGetYear) | Date member reads and calls named `getYear`, before call-arity checking. | TS2339 | TscRejects; note names `date.getUTCFullYear()`. |
| Api(FormGroupBy) | Unshadowed `Object.groupBy` calls, independent of arguments. The pinned ES2022 library has no member. | TS2550 | TscRejects; note names `Map.groupBy(items, callback)`. |
| ContextBytesTypeCount | `bytesOf`, `bytesInto`, `fromBytes` with an explicit type-argument list of length other than one. Missing list has its own site. | TS2558 (2+ types); TS1099 (empty list) | TscRejects; note names each operation with exactly one type argument. |
| ContextBytesArgumentCount | Each byte operation, one valid storage type, source argument count different from 1 / 3 / 2. Spread expressions count as one source argument. | TS2554 for plain bad counts; accepts a valid call plus `...[]`, or a tuple spread for bytesInto/fromBytes | Keep Diverges; add accepted witness `Context.bytesOf<Box>(new Box(), ...[])`. |
| NumberMethodArgumentCount | f32/f64 toFixed, toString, toExponential, toPrecision with 2+ source arguments. Zero-argument toString/toPrecision use API sites instead; toFixed/toExponential accept zero. | TS2554 for 2 plain arguments; accepts `x.toFixed(1, ...[])` and the other methods with the same spread | Keep Diverges; add accepted toFixed witness. |
| ArrayFromArgumentCount | Array.from after valid type-argument selection, with 0 or 4+ source arguments. 2/3 source arguments select the mapper API site. | TS2554 for 0 or 4 plain arguments; accepts `Array.from([1], ...[], ...[], ...[])` | Keep Diverges; add that accepted witness. |
| NumberPredicateCount | Number.isFinite/isNaN/isInteger/isSafeInteger, and global isFinite/isNaN, with source argument count other than one. | TS2554 for plain bad counts; accepts a value plus `...[]` | Keep Diverges; add accepted Number.isFinite witness. |
| JsonParseTypeCount | JSON.parse with one non-spread argument and explicit type list of length other than one. Absent type list and spread/count branches are separate. | TS2558 (2+ types); TS1099 (empty list) | TscRejects; note names `JSON.parse<T>(text)`. |
| ForOfKeys | Zero-argument keys()/values() in a for-of subject on FixedArray. Array/Map/Set select valid fused views; non-container receivers dispatch ordinary methods; entries has its own site. | TS2339 | TscRejects; note names direct value iteration and an index loop. |


The Phase Review found that the four sites kept as `Diverges` still showed "TypeScript accepts" for plain extra arguments, which `tsc` rejects. Rule 1a sends a call with a spread argument to the spread site first, so each of these sites becomes `TscRejects`.

## §310 result

`RunError`, `ReloadError`, and `EmitCFilesError` print the rule, the
note, and the block of each diagnostic, as the CLI does. The ship tier
and `emit_c` keep the checker `Diagnostic`. Work unit 2 (an example for
each site) is next.

## §311 Red at `7f7e92ab`

The total check of §311 rule 4 failed at 185 sites. Each row names the
site, its variant, and the sites that the variant fragment reached
instead.

Count: 185 failing sites.

Site | Variant | Reached sites instead
--- | --- | ---
Api(StringToLocaleLowerCase, LocaleSensitiveString) | LocaleSensitiveString | Api(StringToLocaleUpperCase, LocaleSensitiveString), Api(StringLocaleCompare, LocaleSensitiveString)
Api(RegexStringMatchAll, RegExpSubset) | RegExpSubset | RegexExec, RegexMember
Api(ArrayFind, ArrayMethodDefaults) | ArrayMethodDefaults | Api(FormSort, ArrayMethodDefaults)
Api(ArrayFindLast, ArrayMethodDefaults) | ArrayMethodDefaults | Api(FormSort, ArrayMethodDefaults)
Api(ArrayFlatMap, MethodTypeDomain) | MethodTypeDomain | Api(FormToFixedToStringToExponentialToPrecision, MethodTypeDomain)
Api(ArrayEntries, NoTupleType) | NoTupleType | Api(FormNewMapIterable, NoTupleType), ForOfEntries
Api(ArrayKeys, IteratorTemporary) | IteratorTemporary | Api(MapKeys, IteratorTemporary)
Api(ArrayValues, IteratorTemporary) | IteratorTemporary | Api(MapKeys, IteratorTemporary)
Api(DateLocalGetMonth, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateLocalGetDate, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateLocalGetDay, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateLocalGetHours, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateLocalGetMinutes, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateLocalGetSeconds, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateLocalGetMilliseconds, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateLocalGetTimezoneOffset, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateStringToString, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateStringToDateString, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateStringToTimeString, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateStringToLocaleString, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateStringToLocaleDateString, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(DateStringToLocaleTimeString, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(MapValues, IteratorTemporary) | IteratorTemporary | Api(MapKeys, IteratorTemporary)
Api(MapEntries, NoTupleType) | NoTupleType | Api(FormNewMapIterable, NoTupleType), ForOfEntries
Api(SetKeys, IteratorTemporary) | IteratorTemporary | Api(MapKeys, IteratorTemporary)
Api(SetValues, IteratorTemporary) | IteratorTemporary | Api(MapKeys, IteratorTemporary)
Api(SetEntries, NoTupleType) | NoTupleType | Api(FormNewMapIterable, NoTupleType), ForOfEntries
Api(JsonStringifySetK, JsonSubset) | JsonSubset | JsonParseTarget, Api(JsonStringifyMapKV, JsonSubset)
Api(JsonStringifyObject, JsonSubset) | JsonSubset | JsonParseTarget, Api(JsonStringifyMapKV, JsonSubset)
Api(JsonStringifyFunction, JsonSubset) | JsonSubset | JsonParseTarget, Api(JsonStringifyMapKV, JsonSubset)
Api(JsonStringifyF16, JsonSubset) | JsonSubset | JsonParseTarget, Api(JsonStringifyMapKV, JsonSubset)
Api(JsonParseDateText, JsonSubset) | JsonSubset | JsonParseTarget, Api(JsonStringifyMapKV, JsonSubset)
Api(FormParseIntValue, NumberCoercionAndArguments) | NumberCoercionAndArguments | Api(FormNumberValue, NumberCoercionAndArguments), Api(FormToPrecision, NumberCoercionAndArguments)
Api(FormNewNumberValue, NumberCoercionAndArguments) | NumberCoercionAndArguments | Api(FormNumberValue, NumberCoercionAndArguments), Api(FormToPrecision, NumberCoercionAndArguments)
Api(FormToString, NumberCoercionAndArguments) | NumberCoercionAndArguments | Api(FormNumberValue, NumberCoercionAndArguments), Api(FormToPrecision, NumberCoercionAndArguments)
Api(FormDateParse, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(FormNewDateYearMonth, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(FormDirectComparison, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(FormSet, DateSubset) | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
Api(FormReduceCallback, ArrayMethodDefaults) | ArrayMethodDefaults | Api(FormSort, ArrayMethodDefaults)
Api(FormReduceRightCallback, ArrayMethodDefaults) | ArrayMethodDefaults | Api(FormSort, ArrayMethodDefaults)
Api(FormCallbackValueIndexArray, EscapingCapture) | EscapingCapture | CaptureEffectEscapes
Api(ArrayNullableFind, NullableElementSearch) | NullableElementSearch | Api(MapNullableGet, NullableElementSearch)
Api(ArrayNullableFindLast, NullableElementSearch) | NullableElementSearch | Api(MapNullableGet, NullableElementSearch)
Api(FormNewSetMap, NoTupleType) | NoTupleType | Api(FormNewMapIterable, NoTupleType), ForOfEntries
Api(FormNewSetGeneratorT, GeneratorSingleUse) | GeneratorSingleUse | ArraySpreadGenerator, AssignmentTypeMismatch
Api(FormArrayFromMap, BareMapToArray) | BareMapToArray | ArraySpreadMap
Api(FormArrayFromGeneratorT, GeneratorSingleUse) | GeneratorSingleUse | ArraySpreadGenerator, AssignmentTypeMismatch
ArraySpreadSource | UserIterationProtocol | ComputedMethodName, ForOfUserClass
MapCopyKey | MapKeyKind | MapSetTypeKey, NewMapSetKey
ContextBytesSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
CallSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
SetSourceSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
RegexSticky | RegExpSubset | RegexExec, RegexMember
ContextValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
NumberValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
JsonValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
DateValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
MapSetValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
NumberGlobalValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
DateMemberWrite | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
NumberMethodValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
ArrayElementDomain | MethodTypeDomain | Api(FormToFixedToStringToExponentialToPrecision, MethodTypeDomain)
MapCallbackCountedValue | MethodTypeDomain | Api(FormToFixedToStringToExponentialToPrecision, MethodTypeDomain)
ArrayCallbackSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
ArrayAccumulatorDomain | MethodTypeDomain | Api(FormToFixedToStringToExponentialToPrecision, MethodTypeDomain)
ArrayMapResult | MethodTypeDomain | Api(FormToFixedToStringToExponentialToPrecision, MethodTypeDomain)
MapGroupByKey | MapKeyKind | MapSetTypeKey, NewMapSetKey
ArrayFromTypeCount | ExplicitIntrinsicTypeArguments | ContextBytesMissingType
ArrayFromSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
ArrayFromSource | SourceConstructionDomain | SetSourceDomain
VoidPlaceDropsAsyncHandle | DroppedAsyncHandle | AsyncHandleUnawaited
ContextMember | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
JsonMember | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
ArrayFromValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
ArrayMember | NamespaceObjectMember | ArrayStaticMember
MapGroupByValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
MapSetMember | NamespaceObjectMember | ArrayStaticMember
MathMemberWrite | MathSubset | Api(FormMathUsedAsAValue, MathSubset), Api(FormHypotWithMoreThanTwoArguments, MathSubset)
MathMethodValue | MathSubset | Api(FormMathUsedAsAValue, MathSubset), Api(FormHypotWithMoreThanTwoArguments, MathSubset)
MathMember | MathSubset | Api(FormMathUsedAsAValue, MathSubset), Api(FormHypotWithMoreThanTwoArguments, MathSubset)
MathCallCount | MathSubset | Api(FormMathUsedAsAValue, MathSubset), Api(FormHypotWithMoreThanTwoArguments, MathSubset)
NumberMemberWrite | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
NumberStaticValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
NumberMember | NamespaceObjectMember | ArrayStaticMember
NumberGlobalCount | NumberCoercionAndArguments | Api(FormNumberValue, NumberCoercionAndArguments), Api(FormToPrecision, NumberCoercionAndArguments)
StringPatternSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
DateStaticWrite | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
DateStaticValue | CompilerOwnedValue | Api(FormArrayUsedAsAValue, CompilerOwnedValue)
DateNewSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
DateMember | DateSubset | Api(FormNewDate, DateSubset), Api(DateLocalGetFullYear, DateSubset), Api(FormTemplateInterpolation, DateSubset)
HalfFloatUnary | StorageOnlyHalfFloat | HalfFloatBinary
JsonStaticMember | NamespaceObjectMember | ArrayStaticMember
JsonStringifySpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
JsonParseReviver | JsonCallArguments | JsonStringifyCount
JsonParseSpread | VariadicArguments | Api(FormSpliceStartDeleteCountItems, VariadicArguments), Api(FormUnshiftValueValues, VariadicArguments)
JsonParseDomain | JsonTypeDomain | JsonStringifyDomain
JsonError | JsonSubset | JsonParseTarget, Api(JsonStringifyMapKV, JsonSubset)
RegexMatchType | RegExpSubset | RegexExec, RegexMember
HalfFloatUpdate | StorageOnlyHalfFloat | HalfFloatBinary
ErasedNominalTypeArguments | ErasedAssignableTypeMismatch | ErasedAssignableTypeMismatch
ThisInStaticMethodArrow | ThisStaticMethodMember | ThisStaticMethodMember
ClassAlignmentBelowNatural | ValueClassLayout | ValueClassInheritance
ClassFieldLayoutLimit | AggregateLayoutLimit | FixedArrayByteLimit
SuspendFrameMemberLayoutLimit | AggregateLayoutLimit | FixedArrayByteLimit
AsyncChildFrameLayoutLimit | AggregateLayoutLimit | FixedArrayByteLimit
GeneratorFrameFinalAlignmentLimit | AggregateLayoutLimit | FixedArrayByteLimit
ClosureEnvironmentLayoutLimit | AggregateLayoutLimit | FixedArrayByteLimit
StoredAggregateLayoutLimit | AggregateLayoutLimit | FixedArrayByteLimit
SwitchCaseWriteOutsideDeclaration | DeclarationScope | BlockPendingReadWithoutProgramShadow
ConstructorFieldReadAfterNestedAssignment | ConstructorFieldReadWithAssignmentFact | ConstructorDefiniteFieldReadBeforeAssignment
LambdaUsingDeclaration | UsingDeclaration | AwaitUsingDeclaration
LiteralAliasDuplicateCase | SwitchOverAlias | LiteralAliasSwitchCoverage
DescriptorDisposeMethod | UsingDeclaration | AwaitUsingDeclaration
ValueClassDisposeMethod | UsingDeclaration | AwaitUsingDeclaration
ValueClassWriteAccessor | NamedAccessor | AccessorAssignmentExpressionValue
AsyncGeneratorMethod | AsyncFunctionShape | AsyncStaticMethod, AsyncArrowFunction
ValueClassAsyncMethod | AsyncFunctionShape | AsyncStaticMethod, AsyncArrowFunction
DescriptorOptionalDefaultMissing | OptionalDescriptorMember | DescriptorOptionalInitializerMissing, UndefinedValue
TypeOnlyExportDeclaration | NamedModuleSurface | DefaultExportDeclaration
ExportSpecifierKind | NamedModuleSurface | DefaultExportDeclaration
TypeOnlyOrDefaultExportSpecifier | NamedModuleSurface | DefaultExportDeclaration
TypeOnlyImportReexport | NamedModuleSurface | DefaultExportDeclaration
ReexportSpecifierKind | NamedModuleSurface | DefaultExportDeclaration
CatchBindingUnnarrowedUse | Exceptions | ThrowOperandNotErrorFamily
CatchBindingPattern | Exceptions | ThrowOperandNotErrorFamily
CatchBindingAnnotation | Exceptions | ThrowOperandNotErrorFamily
InstanceofLeftNotErrorFamily | InstanceofNonError | InstanceofRightNotErrorFamily
EnumNonIntegerMember | IntegerLiteralRange | IntegerLiteralRange, HalfFloatLiteralRange
WireEnumMemberNonIntegerSyntax | WireEnumValues | WireEnumMemberNonIntegerValue
WireEnumValueRange | WireEnumValues | WireEnumMemberNonIntegerValue
DuplicateWireEnumValue | WireEnumValues | WireEnumMemberNonIntegerValue
TypeOnlyNamespaceImport | NamedModuleSurface | DefaultExportDeclaration
VoidTypeOutsideResult | VoidValue | VoidExpressionValue
UndefinedKeywordAnnotation | GeneralUnionAndUndefined | UnionGeneralUnionAndUndefined, UndefinedUnionMember, UndefinedValue
MirrorNullableView | NullableNonReference | NullableNonReference
NullableNominalAssignmentShared | SharedLocationNarrowing | NullableMemberShared
NullableAssignmentShared | SharedLocationNarrowing | NullableMemberShared
ImplicitNumericAssignment | SizedOperandWidths | BinaryMixedNumericTypes
CaptureEffectArgument | EscapingCapture | CaptureEffectEscapes
IndexUpdateExpressionValue | ClassIndexSignature | IndexSignatureGetterMismatch, ClassIndexSetSignature, IndexAssignmentExpressionValue
AccessorUpdateExpressionValue | NamedAccessor | AccessorAssignmentExpressionValue
OptionalMemberValueWithoutFallback | OptionalChainUnbound | OptionalCallValueWithoutFallback
DescriptorAbsentMemberRead | OptionalDescriptorMember | DescriptorOptionalInitializerMissing, UndefinedValue
InstancePrototypeMember | DynamicObjectModel | NamespaceClassPrototypeRead, DynamicEvaluatorCalled
UnknownValueName | NamedModuleSurface | DefaultExportDeclaration
DynamicEvaluatorValue | DynamicObjectModel | NamespaceClassPrototypeRead, DynamicEvaluatorCalled
ContextByteTargetLayout | ByteAccessTarget | ContextByteTargetKind
PromiseAllSpread | ForOfSpreadCall | IteratorMethodArgumentCount
CoroutineStepLayoutLimit | AggregateLayoutLimit | FixedArrayByteLimit
DynamicFunctionConstructed | DynamicObjectModel | NamespaceClassPrototypeRead, DynamicEvaluatorCalled
PromiseConstructed | PromiseObject | PromiseStaticCall
StaticMethodAssignment | MethodAssignment | MethodAssignment
NamespacePrototypeRead | DynamicObjectModel | NamespaceClassPrototypeRead, DynamicEvaluatorCalled
GlobalPrototypeRead | DynamicObjectModel | NamespaceClassPrototypeRead, DynamicEvaluatorCalled
WorkerEntryNotNamedFunction | WorkerEntryShape | WorkerEntryAsync
DescriptorDefaultThisUse | ThisInFieldInitializer | DescriptorDefaultThisArithmetic, FieldInitializerUninitializedRead, FieldInitializerSelfRead
FieldInitializerThisUse | ThisInFieldInitializer | DescriptorDefaultThisArithmetic, FieldInitializerUninitializedRead, FieldInitializerSelfRead
ObjectLiteralWithoutDescriptorContext | ObjectLiteralConstruction | FieldDefiniteAssertionUnassigned, NominalObjectLiteral
LogicalAssignmentNonBoolean | LogicalNonBooleanOperandForm | LogicalNonBooleanOperand
StaticAccessorAssignmentExpressionValue | NamedAccessor | AccessorAssignmentExpressionValue
ArrayMapVoidCallback | VoidValue | VoidExpressionValue
JsonSerializerTypeKind | JsonTypeDomain | JsonStringifyDomain
JsonValidatorTypeKind | JsonTypeDomain | JsonStringifyDomain
JsonConstructorTypeKind | JsonTypeDomain | JsonStringifyDomain
JsonArrayConstructorTypeKind | JsonTypeDomain | JsonStringifyDomain
JsonArrayStoreTypeKind | JsonTypeDomain | JsonStringifyDomain
JsonMissingSyntaxError | JsonTypeDomain | JsonStringifyDomain
JsonMissingTypeError | JsonTypeDomain | JsonStringifyDomain
JsonMissingGraphType | JsonTypeDomain | JsonStringifyDomain
BindingPatternUnsupportedRoot | NestedPattern | ArrayBindingNestedPattern
ObjectBindingNestedPattern | NestedPattern | ArrayBindingNestedPattern
ObjectBindingShorthandDefault | PatternDefaultValue | ArrayBindingDefaultValue
ObjectBindingFieldDefault | PatternDefaultValue | ArrayBindingDefaultValue
ConditionalJoinSizedOperandWidths | SizedOperandWidths | BinaryMixedNumericTypes
ConditionalJoinGeneralUnionAndUndefined | GeneralUnionAndUndefined | UnionGeneralUnionAndUndefined, UndefinedUnionMember, UndefinedValue
AggregateAnnotationLimit | AggregateLayoutLimit | FixedArrayByteLimit
AssignmentLiteralAlias | LiteralUnionAlias | UnionLiteralUnionAlias
UndefinedOptionalMember | OptionalDescriptorMember | DescriptorOptionalInitializerMissing, UndefinedValue
LocalAggregateFrameLimit | AggregateLayoutLimit | FixedArrayByteLimit
NullableCallShared | SharedLocationNarrowing | NullableMemberShared
IndexedCallChecked | IndexedReadNullCheck | IndexedMemberChecked
IndexedAssignmentChecked | IndexedReadNullCheck | IndexedMemberChecked
ThisStaticField | StaticMemberSurface | GenericClassStaticMember, ThisStaticMethodMember
IteratorBindingUnsupportedMembers | PatternSourceShape | BindingPatternSourceKind

Command: `cargo test -p subscript-compiler --lib every_subset_rejection_carries_its_divergence -- --test-threads=1 --nocapture`

s307: fragments 2.013s
s154: 1777 witnesses; 507 variants; tsc 1.705s; checker 3.272s; total 5.687s

## §311 sites with an entry of their own

After the second round, 200 sites have an entry of their
own; 37 of them are form b. The entries are in
`compiler/src/check/rejection_examples/`.

- `Api(StringToLocaleLowerCase, LocaleSensitiveString)`
- `Api(RegexStringMatchAll, RegExpSubset)`
- `Api(ArrayFind, ArrayMethodDefaults)`
- `Api(ArrayFindLast, ArrayMethodDefaults)`
- `Api(ArrayFlatMap, MethodTypeDomain)`
- `Api(ArrayEntries, NoTupleType)`
- `Api(ArrayKeys, IteratorTemporary)`
- `Api(ArrayValues, IteratorTemporary)`
- `Api(DateLocalGetMonth, DateSubset)`
- `Api(DateLocalGetDate, DateSubset)`
- `Api(DateLocalGetDay, DateSubset)`
- `Api(DateLocalGetHours, DateSubset)`
- `Api(DateLocalGetMinutes, DateSubset)`
- `Api(DateLocalGetSeconds, DateSubset)`
- `Api(DateLocalGetMilliseconds, DateSubset)`
- `Api(DateLocalGetTimezoneOffset, DateSubset)`
- `Api(DateStringToString, DateSubset)`
- `Api(DateStringToDateString, DateSubset)`
- `Api(DateStringToTimeString, DateSubset)`
- `Api(DateStringToLocaleString, DateSubset)`
- `Api(DateStringToLocaleDateString, DateSubset)`
- `Api(DateStringToLocaleTimeString, DateSubset)`
- `Api(MapValues, IteratorTemporary)`
- `Api(MapEntries, NoTupleType)`
- `Api(SetKeys, IteratorTemporary)`
- `Api(SetValues, IteratorTemporary)`
- `Api(SetEntries, NoTupleType)`
- `Api(JsonStringifySetK, JsonSubset)`
- `Api(JsonStringifyObject, JsonSubset)`
- `Api(JsonStringifyFunction, JsonSubset)`
- `Api(JsonStringifyF16, JsonSubset)`
- `Api(JsonParseDateText, JsonSubset)`
- `Api(FormParseIntValue, NumberCoercionAndArguments)`
- `Api(FormNewNumberValue, NumberCoercionAndArguments)`
- `Api(FormToString, NumberCoercionAndArguments)`
- `Api(FormHypotWithMoreThanTwoArguments, MathSubset)`
- `Api(FormDateParse, DateSubset)`
- `Api(FormNewDateYearMonth, DateSubset)`
- `Api(FormDirectComparison, DateSubset)`
- `Api(FormSet, DateSubset)`
- `Api(FormReduceCallback, ArrayMethodDefaults)`
- `Api(FormReduceRightCallback, ArrayMethodDefaults)`
- `Api(FormCallbackValueIndexArray, EscapingCapture)`
- `Api(ArrayNullableFind, NullableElementSearch)`
- `Api(ArrayNullableFindLast, NullableElementSearch)`
- `Api(FormNewSetMap, NoTupleType)`
- `Api(FormNewSetGeneratorT, GeneratorSingleUse)`
- `Api(FormArrayFromMap, BareMapToArray)`
- `Api(FormArrayFromGeneratorT, GeneratorSingleUse)`
- `InterfaceCallSignature`
- `SetClassInheritance`
- `ArrayClassInheritance`
- `StringPatternSpreadArity`
- `ForOfArrayEntries`
- `ObjectReflectionValue`
- `KeyofAnnotation`
- `CallbackSetRepeatedValue`
- `PromiseThenNullRejection`
- `AsyncArrowReceiverCapture`
- `PromiseFinallyCall`
- `ArrayMapAsyncResult`
- `ArrayMapFunctionResult`
- `ArraySpreadSource`
- `MapCopyKey`
- `ContextBytesSpread`
- `CallSpread`
- `SetSourceSpread`
- `RegexSticky`
- `ContextValue`
- `NumberValue`
- `JsonValue`
- `DateValue`
- `MapSetValue`
- `NumberGlobalValue`
- `DateMemberWrite`
- `DateMethodValue`
- `NumberMethodValue`
- `ArrayElementDomain`
- `MapCallbackCountedValue`
- `ArrayCallbackSpread`
- `ArrayAccumulatorDomain`
- `ArrayMapResult`
- `MapGroupByKey`
- `ArrayFromTypeCount`
- `ArrayFromSpread`
- `ArrayFromSource`
- `VoidPlaceDropsAsyncHandle`
- `ContextMember`
- `JsonMember`
- `ArrayFromValue`
- `ArrayMember`
- `MapGroupByValue`
- `MapSetMember`
- `MathMemberWrite`
- `MathMethodValue`
- `MathMember`
- `MathCallCount`
- `NumberMemberWrite`
- `NumberStaticValue`
- `NumberMember`
- `NumberGlobalCount`
- `StringPatternSpread`
- `DateStaticWrite`
- `DateStaticValue`
- `DateNewSpread`
- `DateMember`
- `HalfFloatUnary`
- `JsonStaticMember`
- `JsonStringifySpread`
- `JsonParseReviver`
- `JsonParseSpread`
- `JsonParseDomain`
- `JsonError`
- `ForOfEntries`
- `RegexMatchType`
- `HalfFloatUpdate`
- `ReactionReceiverCapture`
- `PromiseCatchCall`
- `RestParameter`
- `ErasedNominalTypeArguments`
- `ThisInStaticMethodArrow`
- `ClassAlignmentBelowNatural`
- `ClassFieldLayoutLimit`
- `SuspendFrameMemberLayoutLimit`
- `GeneratorFrameFinalAlignmentLimit`
- `ClosureEnvironmentLayoutLimit`
- `StoredAggregateLayoutLimit`
- `SwitchCaseWriteOutsideDeclaration`
- `FunctionBodyMissing`
- `ConstructorFieldReadAfterNestedAssignment`
- `LambdaUsingDeclaration`
- `LiteralAliasDuplicateCase`
- `DescriptorDisposeMethod`
- `ValueClassDisposeMethod`
- `ValueClassWriteAccessor`
- `AsyncGeneratorMethod`
- `ValueClassAsyncMethod`
- `TypeOnlyExportDeclaration`
- `ExportSpecifierKind`
- `TypeOnlyOrDefaultExportSpecifier`
- `TypeOnlyImportReexport`
- `ReexportSpecifierKind`
- `CatchBindingUnnarrowedUse`
- `CatchBindingPattern`
- `CatchBindingAnnotation`
- `InstanceofLeftNotErrorFamily`
- `DefaultExportDeclaration`
- `EnumNonIntegerMember`
- `GenericSourceAlias`
- `WireEnumMemberNonIntegerSyntax`
- `WireEnumValueRange`
- `DuplicateWireEnumValue`
- `TypeOnlyNamespaceImport`
- `VoidTypeOutsideResult`
- `UndefinedKeywordAnnotation`
- `MirrorNullableView`
- `NullableNominalAssignmentShared`
- `NullableAssignmentShared`
- `ImplicitNumericAssignment`
- `CaptureEffectArgument`
- `IndexUpdateExpressionValue`
- `AccessorUpdateExpressionValue`
- `OptionalMemberValueWithoutFallback`
- `DescriptorAbsentMemberRead`
- `InstancePrototypeMember`
- `UnknownValueName`
- `DynamicEvaluatorValue`
- `ContextByteTargetLayout`
- `PromiseCombinatorCall`
- `PromiseReactionSpread`
- `TaskGroupGeneratorBody`
- `PromiseAllSpread`
- `CoroutineStepLayoutLimit`
- `DynamicFunctionConstructed`
- `PromiseConstructed`
- `StaticMethodAssignment`
- `NamespacePrototypeRead`
- `GlobalPrototypeRead`
- `WorkerEntryNotNamedFunction`
- `DescriptorDefaultThisUse`
- `FieldInitializerThisUse`
- `ObjectLiteralWithoutDescriptorContext`
- `LogicalAssignmentNonBoolean`
- `StaticAccessorAssignmentExpressionValue`
- `ArrayMapVoidCallback`
- `BindingPatternUnsupportedRoot`
- `ObjectBindingNestedPattern`
- `ObjectBindingShorthandDefault`
- `ObjectBindingFieldDefault`
- `ConditionalJoinSizedOperandWidths`
- `ConditionalJoinGeneralUnionAndUndefined`
- `AggregateAnnotationLimit`
- `AssignmentLiteralAlias`
- `UndefinedOptionalMember`
- `LocalAggregateFrameLimit`
- `NullableCallShared`
- `IndexedCallChecked`
- `IndexedAssignmentChecked`
- `ThisStaticField`
- `IteratorBindingUnsupportedMembers`
