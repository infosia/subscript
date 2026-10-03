# S154: non-S014 rejection classes, group b

Measured at `ee67931d2c365d61b28aac3b38efd8342900b853` with TypeScript 5.9.2 and rustc 1.95.0.
This Step 0 measurement tests §79 rule 2 and the open item in §153.3.
No production, test, or corpus file changed. No commit or gate run occurred.

## Method

The inventory starts from diagnostic calls in the seven assigned source files.
`inventory.py` extracts complete calls to `error`, `resolution_error`, `error_diverging`, `Diagnostic::new`, and `reject_subset`.
It excludes S014 calls and test modules. The test-only counter in `bodies.rs:215` does not exclude later production code.
The `MirrorParameter` helper selects S100 and remains in this inventory.
`exports.rs:258` reconstructs resolution diagnostics from stored failures. It counts as one call site, separate from each stored constructor.
A `RuleCode::` occurrence alone does not count as a site.

Inventory: 177 sites. Per file: `class_shape.rs` 60, `declarations.rs` 33, `stmt.rs` 28, `signatures.rs` 20, `exports.rs` 13, `bodies.rs` 12, `exception.rs` 11.

Every witness path in the table starts at `target/s154/b/`.
Every source site path starts at `compiler/src/check/`.
Each program exports `main(): void`. `programs.json` lists each entry and its companion inputs.
The table selects the target diagnostic, even when another diagnostic comes first.
The message column contains the first 60 characters of that diagnostic.
The guard, source position, message, and variant identify the constructor branch.
No production instrumentation was used.

Each mirror declaration uses distinct names to prevent cross-program ambient name conflicts.
Relative module augmentations resolve only within their witness directory.
The checker receives the same entry, module, and mirror inputs that define each witness program.

The search includes library overloads, generics, unions, readonly arrays, interfaces, optional parameters, and `Iterable<T>`.
It also includes truthiness, implicit conversions, literal types, and `prelude/lang.d.ts`.
The `probe-*` programs record the general domain probes.
The per-site alternatives include method overloads, default parameters, sized aliases, ambient modules, module augmentations, and bottom-type operands.
These finite probes establish observed classes. They do not prove that every accepted TypeScript form was tested.
A reached site with only a rejected witness retains its measured TS code.

`batch.rs` links the existing compiler library from this measurement checkout.
It reads all inputs before the timer and calls `check_program` for every program in one process.
The timer includes parsing and checking. It excludes input reads, diagnostic rendering, and process startup.
`diagnostics.tsv` records every diagnostic. `<entry>.checker.txt` records the renderer output.

Two guards require the public `CheckOptions.poison_missing_modules` input.
`signatures.rs:213` and `signatures.rs:234` cannot execute with the default options of `check_program`.
The same process also calls `check_program_with` for four option witnesses.
`options-diagnostics.tsv` and `<entry>.options-checker.txt` record those observations.
The guard column identifies each poison-set branch. Each option witness also runs through ordinary `check_program`.

One TypeScript project contains every witness source and `prelude/lang.d.ts`.
Its options match `compiler/tests/support/tsc.rs::tsconfig`, used by `compiler/tests/tsc_corpus.rs`.
The options are strict, noEmit, ES2022, ESNext, Bundler, ES2022 plus ESNext.Disposable libraries, and empty types.
The project also sets forceConsistentCasingInFileNames.
The CLI omits semantic diagnostics when the project contains parse errors.
`tsc-project.cjs` therefore uses `createProgram` and `getPreEmitDiagnostics` from the pinned TypeScript package.
This single process obtains both parse diagnostics and semantic diagnostics from that one project.
`tsc-results.json` records each program's codes across its entry and companion inputs. It reports no global diagnostics.
The API run, not the incomplete CLI run, determines every class in the table.

## Counts and costs

| Measure | Count |
| --- | ---: |
| Sites | 177 |
| Reached | 163 |
| Reached through ordinary `check_program` | 161 |
| Reached through option guards | 2 |
| Reached by a tsc-accepted program | 146 |
| Accepted-class sites with a block | 43 |
| Needs variant | 103 |
| Wrong: rejected-class sites with a block | 8 |
| Unreachable | 14 |

“Wrong” counts distinct sites with any measured rejected-class block.
The main table prefers an accepted witness when one reaches the site.
The extra rejected-class rows record the remaining wrong observations.
A shared-site block is permitted by §79 rule 6. This label alone does not establish a contract violation.

Checker cost: `programs=250 checker_seconds=0.040609542 options_programs=4 options_seconds=0.000694416`.
TypeScript project cost: 0.208586458 seconds for 250 programs and 285 source files in one process.
The TypeScript timer includes configuration, program creation, and all diagnostics. It excludes diagnostic formatting and process startup.

## Inventory table

| Site / function | Guard | Code | Message: first 60 characters | Witness | tsc | Block | Verdict | Proposed variant |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| <code>class_shape.rs:33 / claim_class_member_name</code> | read accessor exists | <code>S017</code> | <code>two accessors cannot declare the read member `x`</code> | <code>class_shape-33/main.ts</code> | TS2300 | no | ok | <code>—</code> |
| <code>class_shape.rs:44 / claim_class_member_name</code> | write accessor exists | <code>S017</code> | <code>two accessors cannot declare the write member `x`</code> | <code>class_shape-44/main.ts</code> | TS2300 | no | ok | <code>—</code> |
| <code>class_shape.rs:83 / claim_class_member_name</code> | method overload claims the same name | <code>S017</code> | <code>a method cannot share the member name `f` with a method</code> | <code>class_shape-83-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:136 / resolve_class_method</code> | method key is not an identifier or Symbol.dispose | <code>S100</code> | <code>computed method names are not decided</code> | <code>class_shape-136/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:142 / resolve_class_method</code> | descriptor disposal method | <code>S100</code> | <code>descriptor classes cannot declare `[Symbol.dispose]()`</code> | <code>class_shape-142/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:150 / resolve_class_method</code> | descriptor accessor | <code>S100</code> | <code>descriptor classes cannot declare accessors</code> | <code>class_shape-150/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:162 / resolve_class_method</code> | boundary static method | <code>S100</code> | <code>mirror classes cannot declare static methods or accessors</code> | <code>class_shape-162/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:170 / resolve_class_method</code> | static disposal method | <code>S100</code> | <code>`[Symbol.dispose]()` must be non-static</code> | <code>class_shape-170/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:178 / resolve_class_method</code> | async static method | <code>S100</code> | <code>async static methods are not in the decided surface</code> | <code>class_shape-178/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:187 / resolve_class_method</code> | value-class disposal method | <code>S100</code> | <code>value classes cannot declare `[Symbol.dispose]()`</code> | <code>class_shape-187/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:197 / resolve_class_method</code> | boundary accessor | <code>S100</code> | <code>mirror classes cannot declare accessors</code> | <code>class_shape-197/main.ts</code> | accepts | no | needs variant | <code>NamedAccessor</code> |
| <code>class_shape.rs:215 / resolve_class_method</code> | The getter has parameters. The parser rejects this form before the checker. | <code>S100</code> | <code>a read accessor must declare no parameters</code> | <code>class_shape-215/main.ts</code> | TS1054 | — | unreachable | <code>—</code> |
| <code>class_shape.rs:223 / resolve_class_method</code> | getter has no return annotation | <code>S100</code> | <code>a read accessor requires an explicit return type</code> | <code>class_shape-223/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:252 / resolve_class_method</code> | value-class setter | <code>S100</code> | <code>value class `A` cannot declare a write accessor</code> | <code>class_shape-252/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:270 / resolve_class_method</code> | setter has return annotation | <code>S100</code> | <code>a write accessor cannot declare a return type</code> | <code>class_shape-270/main.ts</code> | TS1095 | no | ok | <code>—</code> |
| <code>class_shape.rs:278 / resolve_class_method</code> | The setter parameter count differs from one. The parser rejects this form before the checker. | <code>S100</code> | <code>a write accessor must declare exactly one parameter</code> | <code>class_shape-278/main.ts</code> | TS1049 | — | unreachable | <code>—</code> |
| <code>class_shape.rs:288 / resolve_class_method</code> | setter parameter has default | <code>S100</code> | <code>a write accessor parameter cannot have a default</code> | <code>class_shape-288/main.ts</code> | TS1052 | no | ok | <code>—</code> |
| <code>class_shape.rs:296 / resolve_class_method</code> | setter parameter is a pattern | <code>S100</code> | <code>a write accessor parameter must be an identifier</code> | <code>class_shape-296/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:305 / resolve_class_method</code> | setter parameter has no annotation | <code>S100</code> | <code>a write accessor parameter requires a type annotation</code> | <code>class_shape-305/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:347 / resolve_class_method</code> | async generator method | <code>S100</code> | <code>async generator methods are not in the decided surface</code> | <code>class_shape-347/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:354 / resolve_class_method</code> | synchronous generator method | <code>S100</code> | <code>generator methods are not in the decided surface</code> | <code>class_shape-354/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:364 / resolve_class_method</code> | value-class async instance method | <code>S100</code> | <code>async methods on `@ValueType` value classes are not in the d</code> | <code>class_shape-364/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:379 / resolve_class_method</code> | declared generic method has no body | <code>S100</code> | <code>function bodies are required</code> | <code>class_shape-379/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:386 / resolve_class_method</code> | generic overload signature lacks body | <code>S100</code> | <code>function bodies are required</code> | <code>class_shape-386-accept/main.ts</code> | accepts | no | needs variant | <code>BodilessDeclareGenericMethod</code> |
| <code>class_shape.rs:414 / resolve_class_method</code> | async disposal method | <code>S100</code> | <code>`[Symbol.dispose]()` must be synchronous</code> | <code>class_shape-414/main.ts</code> | accepts | no | needs variant | <code>UsingDeclaration</code> |
| <code>class_shape.rs:423 / resolve_class_method</code> | disposal method has parameter | <code>S100</code> | <code>`[Symbol.dispose]()` takes no parameters and returns `void`</code> | <code>class_shape-423/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:454 / resolve_class_shape</code> | superclass present; class kind selects branch | <code>S006</code> | <code>value classes do not inherit</code> | <code>class_shape-454/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:461 / resolve_class_shape</code> | superclass present; class kind selects branch | <code>S100</code> | <code>descriptor classes do not inherit</code> | <code>class_shape-461/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:463 / resolve_class_shape</code> | superclass present; class kind selects branch | <code>S100</code> | <code>class inheritance is not in the decided surface</code> | <code>class_shape-463/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:475 / resolve_class_shape</code> | field key is not an identifier | <code>S100</code> | <code>computed or non-identifier field names are not decided</code> | <code>class_shape-475/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:485 / resolve_class_shape</code> | descriptor static field | <code>S100</code> | <code>descriptor classes cannot declare static fields</code> | <code>class_shape-485/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:493 / resolve_class_shape</code> | boundary static field | <code>S100</code> | <code>mirror classes cannot declare static fields</code> | <code>class_shape-493/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:511 / resolve_class_shape</code> | static optional property | <code>S012</code> | <code>optional static fields imply `undefined`; use `T &#124; null`</code> | <code>class_shape-511/main.ts</code> | accepts | no | needs variant | <code>GeneralUnionAndUndefined</code> |
| <code>class_shape.rs:520 / resolve_class_shape</code> | static field lacks annotation | <code>S100</code> | <code>static fields require a type annotation</code> | <code>class_shape-520/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:529 / resolve_class_shape</code> | static context-affine field | <code>S100</code> | <code>Worker, Inbox, and Outbox values may not be static fields</code> | <code>class_shape-529-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:561 / resolve_class_shape</code> | The guard requires both definite and optional flags, with no initializer. The parser rejects the combined spelling. | <code>S012</code> | <code>optional descriptor members require a default initializer</code> | <code>class_shape-561-both/main.ts</code> | TS1068, TS1109, TS1128, TS1434, TS2693, TS7008 | — | unreachable | <code>—</code> |
| <code>class_shape.rs:570 / resolve_class_shape</code> | required descriptor field has initializer | <code>S100</code> | <code>a required descriptor member (`name!: T`) cannot have an ini</code> | <code>class_shape-570/main.ts</code> | TS1263 | no | ok | <code>—</code> |
| <code>class_shape.rs:578 / resolve_class_shape</code> | descriptor initializer lacks optional spelling | <code>S100</code> | <code>a descriptor member initializer requires the optional `?` sp</code> | <code>class_shape-578/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:586 / resolve_class_shape</code> | descriptor required field lacks definite spelling | <code>S100</code> | <code>required descriptor members must be spelled `name!: T`</code> | <code>class_shape-586-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:595 / resolve_class_shape</code> | ordinary optional field | <code>S012</code> | <code>optional properties imply `undefined`; use `T &#124; null`</code> | <code>class_shape-595/main.ts</code> | accepts | no | needs variant | <code>GeneralUnionAndUndefined</code> |
| <code>class_shape.rs:612 / resolve_class_shape</code> | instance field lacks annotation | <code>S100</code> | <code>fields require a type annotation</code> | <code>class_shape-612/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:624 / resolve_class_shape</code> | nested wire alias boundary field | <code>S100</code> | <code>wire-mapped aliases are supported only as direct boundary-st</code> | <code>class_shape-624/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:644 / resolve_class_shape</code> | optional descriptor field lacks default | <code>S012</code> | <code>optional descriptor members require a default initializer</code> | <code>class_shape-561/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:653 / resolve_class_shape</code> | instance context-affine field | <code>S100</code> | <code>Worker, Inbox, and Outbox values may not be class fields</code> | <code>class_shape-653-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:679 / resolve_class_shape</code> | field is outside value-class whitelist | <code>S100</code> | <code>field type `string` is outside the value-class whitelist (si</code> | <code>class_shape-679/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:701 / resolve_class_shape</code> | descriptor constructor | <code>S100</code> | <code>descriptor classes cannot declare constructors</code> | <code>class_shape-701/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:719 / resolve_class_shape</code> | nested constructor wire alias | <code>S100</code> | <code>wire-mapped aliases are supported only as direct mirror-cons</code> | <code>class_shape-719/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:732 / resolve_class_shape</code> | constructor parameter property | <code>S100</code> | <code>constructor parameter properties are not decided</code> | <code>class_shape-732/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:765 / resolve_class_shape</code> | numeric and string index signatures | <code>S100</code> | <code>a class can declare at most one index signature</code> | <code>class_shape-765-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:774 / resolve_class_shape</code> | value class index signature | <code>S100</code> | <code>only reference classes can declare an index signature</code> | <code>class_shape-774/main.ts</code> | accepts | no | needs variant | <code>ClassIndexSignature</code> |
| <code>class_shape.rs:781 / resolve_class_shape</code> | static index signature | <code>S100</code> | <code>a class index signature cannot be static</code> | <code>class_shape-781/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:791 / resolve_class_shape</code> | An index parameter without an annotation parses as a computed field. A colon without a type fails in the parser. | <code>S100</code> | <code>a class index signature parameter requires a type annotation</code> | <code>class_shape-791/main.ts</code> | TS2552, TS2564 | — | unreachable | <code>—</code> |
| <code>class_shape.rs:800 / resolve_class_shape</code> | The guard requires an index parameter list other than one identifier. Multiple, rest, pattern, and optional parameters fail in the parser. | <code>S100</code> | <code>a class index signature requires one identifier parameter</code> | <code>class_shape-800/main.ts</code> | TS1096 | — | unreachable | <code>—</code> |
| <code>class_shape.rs:813 / resolve_class_shape</code> | index type is not i32 or u32 | <code>S100</code> | <code>a class index signature requires an `i32` or `u32` index, go</code> | <code>class_shape-813/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:824 / resolve_class_shape</code> | index has no element annotation | <code>S100</code> | <code>a class index signature requires an element type</code> | <code>class_shape-824/main.ts</code> | TS1021 | no | ok | <code>—</code> |
| <code>class_shape.rs:841 / resolve_class_shape</code> | private field AST form | <code>S100</code> | <code>class member form outside the decided surface</code> | <code>class_shape-841/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:859 / resolve_class_shape</code> | setter has no getter | <code>S100</code> | <code>write accessor `x` requires a read accessor with the same na</code> | <code>class_shape-859/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:881 / resolve_class_shape</code> | getter/setter types differ | <code>S100</code> | <code>the read and write accessors of `x` must have the same type</code> | <code>class_shape-881/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>class_shape.rs:909 / validate_class_index_accessors</code> | index signature lacks matching get method | <code>S100</code> | <code>the index signature requires `get(index: i32): i32` with exa</code> | <code>class_shape-909/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>class_shape.rs:936 / validate_class_index_accessors</code> | index signature lacks matching set method | <code>S100</code> | <code>the index signature requires `set(index: i32, value: i32): v</code> | <code>class_shape-936/main.ts</code> | accepts | no | needs variant | <code>ClassIndexSignature</code> |
| <code>declarations.rs:40 / register_scope_binding</code> | scope already owns top-level name | <code>S017</code> | <code>duplicate top-level name `f`</code> | <code>declarations-40/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:68 / collect_file</code> | default export module declaration | <code>S100</code> | <code>the module surface requires named exports</code> | <code>declarations-68/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:78 / collect_file</code> | namespace import equals declaration | <code>S100</code> | <code>only `export` declarations and named imports are in the deci</code> | <code>declarations-78-namespace/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:119 / collect_decl</code> | module-level using | <code>S100</code> | <code>module-level `using` is not in the decided surface</code> | <code>declarations-119/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:131 / collect_decl</code> | program interface declaration | <code>S100</code> | <code>declaration form outside the decided surface</code> | <code>declarations-131/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:203 / class_decorators</code> | local decorator accepts broader alignment | <code>S100</code> | <code>`@ValueType` alignment must be an integer literal in {2, 4, </code> | <code>declarations-203-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:215 / class_decorators</code> | local decorator accepts options | <code>S100</code> | <code>`@Descriptor` does not accept options</code> | <code>declarations-215-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:223 / class_decorators</code> | unrecognized decorator | <code>S100</code> | <code>the only decided decorators are the ambient `@ValueType` and</code> | <code>declarations-223/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:232 / class_decorators</code> | both class decorators | <code>S100</code> | <code>`@Descriptor` declares a reference class and cannot be combi</code> | <code>declarations-232/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:255 / collect_class</code> | generic class static member | <code>S100</code> | <code>generic classes cannot declare static members</code> | <code>declarations-255/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:284 / collect_class</code> | generic class generic method | <code>S100</code> | <code>generic classes cannot declare generic methods</code> | <code>declarations-284/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:361 / collect_fn</code> | non-generic function lacks body | <code>S100</code> | <code>function bodies are required</code> | <code>declarations-361/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:403 / collect_type_parameter_names</code> | duplicate type parameter | <code>S017</code> | <code>duplicate type parameter `T`</code> | <code>declarations-403/main.ts</code> | TS2300 | no | ok | <code>—</code> |
| <code>declarations.rs:436 / reject_outer_pattern</code> | top-level binding pattern | <code>S100</code> | <code>a binding pattern binds inside a function body; a declaratio</code> | <code>declarations-436/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:461 / collect_enum</code> | enum member has string key | <code>S100</code> | <code>string enum member names are not decided</code> | <code>declarations-461/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:474 / collect_enum</code> | implicit enum successor exceeds i32 | <code>S008</code> | <code>implicit value for enum member `b` overflows i32</code> | <code>declarations-474/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:489 / collect_enum</code> | enum initializer not integer literal | <code>S100</code> | <code>enum members must have integer literal values</code> | <code>declarations-489/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:518 / collect_string_alias</code> | generic string alias | <code>S100</code> | <code>string-literal union aliases cannot be generic</code> | <code>declarations-518/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:530 / collect_string_alias</code> | program alias not string-literal union | <code>S100</code> | <code>type aliases are limited to a union of two or more string li</code> | <code>declarations-530/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:538 / collect_string_alias</code> | The guard requires more than 2147483647 string members. Their minimum source size exceeds the 32-bit source-position domain. | <code>S100</code> | <code>string-literal union has more members than fit its i32 discr</code> | <code>—</code> | — | — | unreachable | <code>—</code> |
| <code>declarations.rs:550 / collect_string_alias</code> | duplicate union literal | <code>S100</code> | <code>duplicate string-literal union member `a`</code> | <code>declarations-550/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:577 / collect_wire_string_alias</code> | empty CEnum mapping | <code>S100</code> | <code>wire-mapped string-literal union must have at least one memb</code> | <code>declarations-577/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:585 / collect_wire_string_alias</code> | The guard requires more than 2147483647 property members. Their minimum source size exceeds the 32-bit source-position domain. | <code>S100</code> | <code>wire-mapped string-literal union has more members than fit i</code> | <code>—</code> | — | — | unreachable | <code>—</code> |
| <code>declarations.rs:599 / collect_wire_string_alias</code> | CEnum index signature mapping | <code>S100</code> | <code>CEnum mappings contain only named properties with integer-li</code> | <code>declarations-599-index/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:610 / collect_wire_string_alias</code> | CEnum property key is numeric | <code>S100</code> | <code>CEnum member keys must be string literals or identifiers</code> | <code>declarations-610/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:619 / collect_wire_string_alias</code> | duplicate CEnum member name | <code>S100</code> | <code>duplicate string-literal union member `a`</code> | <code>declarations-619/main.ts</code> | TS2300 | no | ok | <code>—</code> |
| <code>declarations.rs:627 / collect_wire_string_alias</code> | CEnum member lacks annotation | <code>S100</code> | <code>wire value for CEnum member `a` must be an integer literal</code> | <code>declarations-627/main.ts</code> | TS7008 | yes | wrong | <code>—</code> |
| <code>declarations.rs:640 / collect_wire_string_alias</code> | CEnum member type is not numeric literal | <code>S100</code> | <code>wire value for CEnum member `a` must be an integer literal</code> | <code>declarations-640/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:649 / collect_wire_string_alias</code> | CEnum wire value is fractional | <code>S100</code> | <code>wire value for CEnum member `a` must be an integer literal</code> | <code>declarations-649/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:662 / collect_wire_string_alias</code> | CEnum wire value exceeds i32 | <code>S100</code> | <code>wire value 2147483648 for CEnum member `a` is outside the i3</code> | <code>declarations-662/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:674 / collect_wire_string_alias</code> | duplicate CEnum wire value | <code>S100</code> | <code>duplicate CEnum wire value 1 for members `a` and `b`</code> | <code>declarations-674/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>declarations.rs:749 / collect_mirror_decl</code> | unsupported mirror declaration | <code>S100</code> | <code>mirror declaration form outside the decided surface</code> | <code>declarations-749/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>declarations.rs:818 / collect_ambient_consts</code> | mirror constant lacks integer literal initializer | <code>S100</code> | <code>mirror variable `A_declarations_818` is outside the decided </code> | <code>declarations-818/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:197 / check_stmt</code> | using in lambda frame | <code>S100</code> | <code>nested declarations are not in the decided surface</code> | <code>stmt-197-lambda/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>stmt.rs:210 / check_stmt</code> | other local declaration | <code>S100</code> | <code>nested declarations are not in the decided surface</code> | <code>stmt-210/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:241 / check_stmt</code> | A valid target label encloses the break. The checker rejects that labelled statement without entry into its body. An unknown label fails in the parser. | <code>S100</code> | <code>labeled break is not decided</code> | <code>stmt-241/main.ts</code> | accepts | — | unreachable | <code>—</code> |
| <code>stmt.rs:244 / check_stmt</code> | The guard requires zero loop and switch depth. The parser rejects an unlabelled break outside these statements. | <code>S100</code> | <code>`break` outside a loop or switch</code> | <code>stmt-244/main.ts</code> | TS1107 | — | unreachable | <code>—</code> |
| <code>stmt.rs:261 / check_stmt</code> | A valid target label encloses the continue. The checker rejects that labelled statement without entry into its body. An unknown label fails in the parser. | <code>S100</code> | <code>labeled continue is not decided</code> | <code>stmt-261/main.ts</code> | accepts | — | unreachable | <code>—</code> |
| <code>stmt.rs:268 / check_stmt</code> | The guard requires zero loop depth. The parser rejects an unlabelled continue outside a loop. | <code>S100</code> | <code>`continue` outside a loop</code> | <code>stmt-268/main.ts</code> | TS1107 | — | unreachable | <code>—</code> |
| <code>stmt.rs:295 / check_stmt</code> | unhandled statement AST form | <code>S100</code> | <code>statement form outside the decided surface</code> | <code>stmt-295/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:311 / check_let</code> | local var declaration | <code>S100</code> | <code>`var` is not in the language; use `let` or `const`</code> | <code>stmt-311/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:324 / check_using</code> | await using | <code>S100</code> | <code>`await using` is not in the decided surface</code> | <code>stmt-324/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>stmt.rs:359 / check_bindings</code> | local lacks initializer | <code>S100</code> | <code>local declarations require an initializer</code> | <code>stmt-359/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:391 / check_bindings</code> | inferred initializer type null | <code>S100</code> | <code>cannot infer a type from `null`; annotate the declaration</code> | <code>stmt-391/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:425 / check_bindings</code> | null disposal resource | <code>S100</code> | <code>a `using` binding must be a reference class with a disposal </code> | <code>stmt-425-null/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:475 / check_return</code> | generator returns a value | <code>S100</code> | <code>generator return values are not in the decided surface</code> | <code>stmt-475/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:484 / check_return</code> | contextual void lambda returns value | <code>S100</code> | <code>a `void` function cannot return a value</code> | <code>stmt-484-lambda/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>stmt.rs:516 / check_return</code> | nonvoid return lacks value | <code>S100</code> | <code>missing return value of type `i32`</code> | <code>stmt-516/main.ts</code> | TS2322 | no | ok | <code>—</code> |
| <code>stmt.rs:535 / require_bool</code> | condition type is not bool | <code>S100</code> | <code>condition must be boolean, got `i32`</code> | <code>stmt-535/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:788 / check_for_of</code> | for await of | <code>S013</code> | <code>`for await…of` requires the Promise object/iterator surface,</code> | <code>stmt-788/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:994 / for_of_binding</code> | for-of var declaration | <code>S100</code> | <code>`var` is not in the language; use `let` or `const`</code> | <code>stmt-994/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:1007 / for_of_binding</code> | for-of await using | <code>S100</code> | <code>`await using` in a `for` head is not in the decided surface</code> | <code>stmt-1007/main.ts</code> | accepts | no | needs variant | <code>UsingDeclaration</code> |
| <code>stmt.rs:1019 / for_of_binding</code> | for-of existing identifier head | <code>S100</code> | <code>`for…of` requires a `const` or `let` identifier binding</code> | <code>stmt-1019/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:1028 / for_of_binding</code> | The guard requires a for-of declaration count other than one. The parser rejects multiple bindings. | <code>S100</code> | <code>`for…of` requires exactly one identifier binding</code> | <code>stmt-1028/main.ts</code> | TS1188 | — | unreachable | <code>—</code> |
| <code>stmt.rs:1037 / for_of_binding</code> | The guard requires an initializer in a for-of binding. The parser rejects that form. | <code>S100</code> | <code>`for…of` bindings cannot have an initializer</code> | <code>stmt-1037/main.ts</code> | TS1190 | — | unreachable | <code>—</code> |
| <code>stmt.rs:1101 / check_for_of_subject_under_flag</code> | zero-length tuple spread to iterator method | <code>S100</code> | <code>`values()` expects no arguments</code> | <code>stmt-1101-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:1238 / check_switch_case_test</code> | duplicate case on parameter union | <code>S100</code> | <code>duplicate case label &quot;a&quot; for string-literal union alias `A`</code> | <code>stmt-1238-accept/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>stmt.rs:1249 / check_switch_case_test</code> | case literal absent from alias | <code>S100</code> | <code>case label &quot;c&quot; is not a member of string-literal union alias</code> | <code>stmt-1249/main.ts</code> | TS2678 | no | ok | <code>—</code> |
| <code>stmt.rs:1260 / check_switch_case_test</code> | alias case is not literal | <code>S100</code> | <code>case labels for string-literal union alias `A` must be strin</code> | <code>stmt-1260/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:1298 / check_switch</code> | switch discriminant unsupported type | <code>S100</code> | <code>switch discriminants are integers, enums, strings, or string</code> | <code>stmt-1298/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>stmt.rs:1428 / check_switch</code> | alias switch misses member | <code>S100</code> | <code>non-exhaustive switch over string-literal union alias `A`; m</code> | <code>stmt-1428/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>signatures.rs:47 / resolve_mirror_signatures</code> | foreign parameter nests wire alias | <code>S100</code> | <code>wire-mapped aliases are supported only as direct foreign-fun</code> | <code>signatures-47/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:60 / resolve_mirror_signatures</code> | foreign return nests wire alias | <code>S100</code> | <code>wire-mapped aliases are supported only as direct foreign-fun</code> | <code>signatures-60/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:79 / resolve_mirror_signatures</code> | direct foreign callback parameter | <code>S100</code> | <code>mirror `target/s154/b/signatures-79/mirror.d.ts` foreign fun</code> | <code>signatures-79/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:106 / resolve_mirror_signatures</code> | foreign return lacks provenance vocabulary | <code>S100</code> | <code>mirror `target/s154/b/signatures-106/mirror.d.ts` foreign fu</code> | <code>signatures-106/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:119 / resolve_mirror_signatures</code> | The guard requires no foreign mirror ID. collect_mirror_provenance inserts an ID for every mirror with function declarations, even without a header. | <code>S100</code> | <code>mirror `{}` has no header identity for foreign function `{}`</code> | <code>signatures-119/main.ts</code> | accepts | — | unreachable | <code>—</code> |
| <code>signatures.rs:187 / resolve_imports</code> | type-only namespace import | <code>S100</code> | <code>`import type * as` is outside the decided surface</code> | <code>signatures-187/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>signatures.rs:213 / resolve_imports</code> | missing module and empty import in poison set | <code>S100</code> | <code>imported module `./missing` is not among the program&#x27;s files</code> | <code>signatures-213/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:234 / resolve_imports</code> | poisoned library default import | <code>S100</code> | <code>only named imports are in the decided surface</code> | <code>signatures-234-lib/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:263 / resolve_imports</code> | ambient library module absent from program files | <code>S100</code> | <code>imported module `s154_external_263` is not among the program</code> | <code>signatures-263-lib/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:300 / resolve_imports</code> | known module default import | <code>S100</code> | <code>only named imports are in the decided surface</code> | <code>signatures-300/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:317 / resolve_imports</code> | library module augmentation supplies imported name | <code>S016</code> | <code>`missing_signatures_317_augment` is not exported by `./other</code> | <code>signatures-317-augment/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:373 / resolve_signatures</code> | global lacks annotation | <code>S100</code> | <code>module-level variables require a type annotation</code> | <code>signatures-373/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:382 / resolve_signatures</code> | context-affine global | <code>S100</code> | <code>Worker, Inbox, and Outbox values may not be module globals</code> | <code>signatures-382-accept/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>signatures.rs:409 / resolve_fn_sig</code> | async generator function | <code>S100</code> | <code>a function cannot be both async and a generator</code> | <code>signatures-409/main.ts</code> | accepts | no | needs variant | <code>AsyncFunctionShape</code> |
| <code>signatures.rs:425 / resolve_fn_sig</code> | async function lacks explicit Promise annotation | <code>S100</code> | <code>async functions require an explicit `Promise&lt;T&gt;` return anno</code> | <code>signatures-425/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:478 / resolve_fn_sig</code> | synchronous function lacks return annotation | <code>S100</code> | <code>function return types must be annotated</code> | <code>signatures-478/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:509 / resolve_param_pat</code> | optional parameter | <code>S012</code> | <code>optional parameters imply `undefined`; use a default value o</code> | <code>signatures-509/main.ts</code> | accepts | no | needs variant | <code>GeneralUnionAndUndefined</code> |
| <code>signatures.rs:519 / resolve_param_pat</code> | default parameter lacks annotation | <code>S100</code> | <code>parameters require a type annotation</code> | <code>signatures-519/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:548 / resolve_param_pat</code> | pattern parameter lacks annotation | <code>S100</code> | <code>parameters require a type annotation</code> | <code>signatures-548/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>signatures.rs:562 / resolve_param_pat</code> | mirror pattern parameter | <code>S100</code> | <code>parameter pattern outside the decided surface</code> | <code>signatures-566/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exports.rs:44 / register_export</code> | register declaration after re-export owns name | <code>S017</code> | <code>duplicate export name `x`</code> | <code>exports-44/main.ts</code> | TS2323, TS2484 | no | ok | <code>—</code> |
| <code>exports.rs:73 / collect_named_exports</code> | mirror named export list | <code>S100</code> | <code>export lists are outside the mirror surface</code> | <code>exports-73/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>exports.rs:81 / collect_named_exports</code> | type-only export declaration | <code>S100</code> | <code>type-only exports are outside the named module surface</code> | <code>exports-81/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exports.rs:101 / collect_named_exports</code> | ambient library module absent from program files | <code>S100</code> | <code>export source module `s154_external_101` is not among the pr</code> | <code>exports-101-lib/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>exports.rs:114 / collect_named_exports</code> | namespace export specifier | <code>S100</code> | <code>the module surface requires named exports</code> | <code>exports-114/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exports.rs:126 / collect_named_exports</code> | named export of default | <code>S100</code> | <code>type-only and default exports are outside the named module s</code> | <code>exports-126/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exports.rs:139 / collect_named_exports</code> | local type-only import re-export | <code>S100</code> | <code>`A` was imported with `import type`; its re-export is a type</code> | <code>exports-139/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exports.rs:162 / collect_named_exports</code> | named export already registered | <code>S017</code> | <code>duplicate export name `x`</code> | <code>exports-162/main.ts</code> | TS2300 | no | ok | <code>—</code> |
| <code>exports.rs:258 / resolve_exports</code> | resolution failures nonempty | <code>S016</code> | <code>`missing` is not defined in `target/s154/b/exports-258/main.</code> | <code>exports-258/main.ts</code> | TS2304 | no | ok | <code>—</code> |
| <code>exports.rs:292 / resolve_export_name</code> | export alias dependency cycle | <code>S016</code> | <code>export alias chain reaches no declaration: target/s154/b/exp</code> | <code>exports-292/main.ts</code> | TS2303 | no | ok | <code>—</code> |
| <code>exports.rs:313 / resolve_export_name</code> | local namespace export | <code>S100</code> | <code>the module surface requires named exports</code> | <code>exports-313/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exports.rs:327 / resolve_export_name</code> | local export name absent | <code>S016</code> | <code>`missing` is not defined in `target/s154/b/exports-327/main.</code> | <code>exports-327/main.ts</code> | TS2304 | no | ok | <code>—</code> |
| <code>exports.rs:370 / resolve_export_source</code> | library module augmentation supplies remote export | <code>S016</code> | <code>`missing_exports_370_augment` is not exported by `./other`</code> | <code>exports-370-augment/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:123 / check_body_decl</code> | top-level var | <code>S100</code> | <code>`var` is not in the language; use `let` or `const`</code> | <code>bodies-123/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:158 / check_body_decl</code> | top-level initializer absent | <code>S100</code> | <code>module-level variables require an initializer</code> | <code>bodies-158/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:225 / check_function</code> | ambient ordinary method lacks body | <code>S100</code> | <code>function bodies are required</code> | <code>bodies-225-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:236 / check_function</code> | unhandled async origin | <code>S013</code> | <code>an async handle is dropped without any await of its completi</code> | <code>bodies-236/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>bodies.rs:261 / check_function</code> | exhaustive enum switch has no default | <code>S100</code> | <code>not all paths return a value</code> | <code>bodies-261-enum/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:387 / check_class_body</code> | static field initializer absent | <code>S100</code> | <code>static fields require an initializer</code> | <code>bodies-387/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:703 / require_field_values</code> | unassigned definite field | <code>S100</code> | <code>field `x` of `A` asserts with `!` a value that nothing assig</code> | <code>bodies-703/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>bodies.rs:713 / require_field_values</code> | field assignment after statement that holds return | <code>S100</code> | <code>field `x` of `A` is assigned at the constructor&#x27;s top level </code> | <code>bodies-713/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:724 / require_field_values</code> | only nested field assignments | <code>S100</code> | <code>field `x` of `A` is assigned inside a nested statement of th</code> | <code>bodies-724/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>bodies.rs:734 / require_field_values</code> | constructor always throws before field assignment | <code>S100</code> | <code>field `x` of `A` has no initializer, and no constructor stat</code> | <code>bodies-734-throw/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:824 / check_this_in_assignment_prefix</code> | definite assertion permits prefix read | <code>S100</code> | <code>`this.x` reads field `x` of `A` before the constructor assig</code> | <code>bodies-824-accept/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>bodies.rs:839 / check_this_in_assignment_prefix</code> | prefix calls member before fields hold values | <code>S100</code> | <code>the constructor of `A` calls a member of `this` before field</code> | <code>bodies-839/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exception.rs:218 / check_error_new</code> | Error type arguments | <code>S100</code> | <code>`Error` is not generic</code> | <code>exception-218/main.ts</code> | TS2558 | no | ok | <code>—</code> |
| <code>exception.rs:235 / check_error_new</code> | Error message is a literal union alias | <code>S100</code> | <code>the `Error` message must be a `string`, got `Message`</code> | <code>exception-235-alias/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>exception.rs:244 / check_error_new</code> | Error arguments count exceeds one | <code>S100</code> | <code>`new Error` takes one `string` message argument</code> | <code>exception-244/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>exception.rs:282 / reject_error_call</code> | ambient Error call without new | <code>S100</code> | <code>`Error` is constructed with `new Error(message)`; a call wit</code> | <code>exception-282/main.ts</code> | accepts | no | needs variant | <code>new</code> |
| <code>exception.rs:328 / reject_caught_read</code> | catch binding read before narrowing | <code>S010</code> | <code>the catch binding `e` is used outside `instanceof` and `thro</code> | <code>exception-328/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exception.rs:356 / check_throw</code> | throw operand is not Error | <code>S010</code> | <code>`throw` requires an Error-family object; this operand has ty</code> | <code>exception-356/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exception.rs:382 / check_try</code> | finally present | <code>S010</code> | <code>`finally` is not in the decided exception surface; repeat th</code> | <code>exception-382/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exception.rs:448 / catch_binding</code> | any-annotated catch pattern | <code>S010</code> | <code>a catch binding is one name; a binding pattern is not in the</code> | <code>exception-448-any/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exception.rs:464 / catch_binding</code> | catch annotation is not unknown | <code>S010</code> | <code>a catch binding annotation other than `unknown` is rejected;</code> | <code>exception-464/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exception.rs:511 / check_instanceof</code> | instanceof RHS is non-Error class | <code>S100</code> | <code>`instanceof` requires an Error-family class as its right ope</code> | <code>exception-511/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>exception.rs:526 / check_instanceof</code> | instanceof LHS is non-Error object | <code>S100</code> | <code>`instanceof` tests an Error-family object or a catch binding</code> | <code>exception-526/main.ts</code> | accepts | yes | ok | <code>—</code> |
| <code>stmt.rs:484 / check_return (rejected class)</code> | void function returns nonvoid value | <code>S100</code> | <code>a `void` function cannot return a value</code> | <code>stmt-484/main.ts</code> | TS2322 | yes | wrong | <code>—</code> |
| <code>stmt.rs:1238 / check_switch_case_test (rejected class)</code> | duplicate string alias case | <code>S100</code> | <code>duplicate case label &quot;a&quot; for string-literal union alias `A`</code> | <code>stmt-1238/main.ts</code> | TS2678 | yes | wrong | <code>—</code> |
| <code>bodies.rs:724 / require_field_values (rejected class)</code> | conditional nested assignment leaves field unset | <code>S100</code> | <code>field `x` of `A` is assigned inside a nested statement of th</code> | <code>bodies-724-reject/main.ts</code> | TS2564 | yes | wrong | <code>—</code> |
| <code>exception.rs:448 / catch_binding (rejected class)</code> | catch pattern | <code>S010</code> | <code>a catch binding is one name; a binding pattern is not in the</code> | <code>exception-448/main.ts</code> | TS2339 | yes | wrong | <code>—</code> |
| <code>exception.rs:464 / catch_binding (rejected class)</code> | catch numeric annotation | <code>S010</code> | <code>a catch binding annotation other than `unknown` is rejected;</code> | <code>exception-464-reject/main.ts</code> | TS1196 | yes | wrong | <code>—</code> |
| <code>exception.rs:511 / check_instanceof (rejected class)</code> | invalid instanceof RHS | <code>S100</code> | <code>`instanceof` requires an Error-family class as its right ope</code> | <code>exception-511-reject/main.ts</code> | TS2358, TS2359 | yes | wrong | <code>—</code> |
| <code>exception.rs:526 / check_instanceof (rejected class)</code> | primitive instanceof LHS | <code>S100</code> | <code>`instanceof` tests an Error-family object or a catch binding</code> | <code>exception-526-reject/main.ts</code> | TS2358 | yes | wrong | <code>—</code> |

## Guard evidence

The parser uses `no_early_errors: false` in `compiler/src/parse.rs`.
Getter parameters, setter arity, loop jumps, and for-of declaration errors fail before the checker.
The corresponding probe outputs record those parse failures.
A labelled statement reaches `check_stmt`'s unsupported-statement arm, which does not visit its body.
The valid labelled probes therefore never reach the inner break or continue site.
The invalid label probes fail in the parser.

The first optional-descriptor call requires `(definite, optional, initializer) = (true, true, false)`.
The parser rejects that combined spelling. The ordinary optional probe reaches `class_shape.rs:644`, which uses the same text.
A missing index parameter annotation parses as a computed property. A colon without a type produces a parse error.
Multiple, rest, pattern, and optional index parameter probes also produce parse errors.

The two member-count guards require more than `i32::MAX` AST members.
Even empty string members need at least three source bytes each. Property members need at least as many.
These inputs exceed the parser's 32-bit source-position domain before the count guard can execute.
No multi-gigabyte allocation was attempted.

`collect_mirror_provenance` inserts a foreign mirror ID whenever the mirror declares a function.
A missing header produces another diagnostic, but the function still gets an ID.
The missing-ID arm at `signatures.rs:119` therefore cannot execute through the public program checker.

## Required examples

§149.3 item 1: `known-pair/main.ts` reaches `inference.rs:149`. tsc: accepts. Block: yes. Verdict: ok.
§149.3 item 2: `known-apply/main.ts` reaches `expr/literal.rs:394`. tsc: accepts. Block: no. Verdict: needs variant.
Both constructors are outside group b and do not enter its counts.
`stmt-535/main.ts` is the required `const n: i32 = 3; if (n) {}` example.
It reaches `stmt.rs:535`, tsc accepts, and the diagnostic lacks a block. Its proposed variant is `new`.

## Needs variant, grouped by proposal

An existing variant appears only when its `why` covers the measured restriction.
`new` means that no selected existing reason covers the target guard.

### AsyncFunctionShape

- `signatures.rs`: `409`.

### BodilessDeclareGenericMethod

- `class_shape.rs`: `386`.

### ClassIndexSignature

- `class_shape.rs`: `774`, `936`.

### GeneralUnionAndUndefined

- `class_shape.rs`: `511`, `595`.
- `signatures.rs`: `509`.

### NamedAccessor

- `class_shape.rs`: `197`.

### UsingDeclaration

- `class_shape.rs`: `414`.
- `stmt.rs`: `1007`.

### new

- `class_shape.rs`: `83`, `136`, `150`, `162`, `170`, `223`, `296`, `305`, `354`, `423`, `461`, `463`, `475`, `485`, `493`, `520`, `529`, `578`, `586`, `612`, `624`, `653`, `679`, `701`, `719`, `732`, `765`, `781`, `813`, `841`, `859`, `881`.
- `declarations.rs`: `40`, `78`, `119`, `131`, `203`, `215`, `223`, `232`, `361`, `461`, `474`, `518`, `530`, `550`, `577`, `599`, `610`, `749`, `818`.
- `stmt.rs`: `210`, `295`, `311`, `359`, `391`, `425`, `475`, `535`, `788`, `994`, `1019`, `1101`, `1260`, `1298`.
- `signatures.rs`: `47`, `60`, `79`, `106`, `213`, `234`, `263`, `300`, `317`, `373`, `425`, `478`, `519`, `548`.
- `exports.rs`: `73`, `101`, `370`.
- `bodies.rs`: `123`, `158`, `225`, `261`, `387`, `713`, `734`, `824`.
- `exception.rs`: `235`, `244`, `282`.

