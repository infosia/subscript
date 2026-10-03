# S154 group a: rejection TypeScript classes

Measured at `ee67931d2c365d61b28aac3b38efd8342900b853` with TypeScript 5.9.2 and rustc 1.95.0.
This Step 0 round measures §79 rule 2 and §153.3 item 1.
No production, test, or corpus file changed. No commit or gate run occurred.

The inventory has 237 production diagnostic calls in `compiler/src/check/expr.rs` and `compiler/src/check/expr/`.
The root `expr.rs` file has no production diagnostic call.
The inventory includes 235 direct error calls and two calls to `nullable_use_error` that take a rule code.
It excludes S014 helpers, test code, and rule-code values that construct no diagnostic.
The dynamic-code call at `call.rs:1533` constructs either S100 or S016 and counts once.

The inventory comes from diagnostic calls and a source review of each caller and guard.
`target/s154/a/inventory.json` stores each call, its function, and its source context.
No production instrumentation changed the compiler.
Source guards, witness syntax, diagnostic text, and source positions identify each target.
Identical messages need their source guards: two condition guards, two undefined guards, and three prototype guards are distinct.
The SWC AST dump `s222.ast.txt` confirms the private optional member reaches `check_optional_member`.

All paths below start at `target/s154/a/`. All program entries export `main(): void`.
The site paths start at `compiler/src/check/expr/`.
A witness can produce other diagnostics; the block column describes only the selected target diagnostic.
The table states the first 60 message characters. Complete renders are in `<program-id>.checker.txt`.
`rows.json` records each selected diagnostic index and source position.

The search includes library overloads, generic methods, generic function values, unions, readonly arrays, interfaces, optional parameters, and Iterable.
It also includes truthiness, implicit numeric conversions, literal types, and the supplied prelude.
`probe-*` programs retain search controls. An interface/class merge poisons the compiler name before the target; it proves no target reach.
The library controls distinguish S014 overload guards from the non-S014 argument-count guards.
Regex controls include lookbehind, Unicode properties, invalid ranges, escapes, quantifiers, and backreferences.
A reject label states the measured witness class. It does not prove that all TypeScript source extensions fail at that site.
Only the eight unreachable rows use caller or parser invariants to exclude every program.

Summary: 237 sites; 229 reached; 172 reached by a tsc-accepted program.
Of those accepted sites, 49 render a block and 123 need a variant.
Primary verdicts: 106 ok; 123 needs variant; 0 wrong; 8 unreachable.
A rejected control reaches one accepted primary site with a block: `member.rs:261`.
Thus the complete witness set records one wrong site; the primary table records zero wrong sites.
This shared-site observation follows §79 rule 6 and requires no change by itself.

`batch.rs` links the existing compiler rlib; its modification time follows all compiler source files at the measurement pin.
The harness reads all sources before its checker timer and calls `check_program` for all 263 programs in one process.
Checker cost: 0.048739792 seconds for parsing and checks; process wall cost: 0.076685625 seconds.
The process wall cost also includes input reads, diagnostic renders, and output writes.

One TypeScript project contains all witness sources, mirrors, helper modules, and `prelude/lang.d.ts`.
Its options match `compiler/tests/support/tsc.rs::tsconfig`: strict, noEmit, ES2022, ESNext, Bundler,
ES2022 plus ESNext.Disposable libraries, empty types, and forceConsistentCasingInFileNames.
The CLI command is `node_modules/.bin/tsc --project target/s154/a/tsconfig.json --pretty false`.
Its wall cost is 0.112121792 seconds, but syntax controls cause it to skip semantic diagnostics across the project.
Thus its empty per-file results cannot establish acceptance.
`tsc-project.cjs` loads that same project with the pinned TypeScript compiler and calls `getPreEmitDiagnostics` once.
This API includes syntactic and semantic diagnostics in one TypeScript process, even when a syntax control fails.
TypeScript project cost: 0.216783208 seconds inside the driver; process wall cost: 0.330282083 seconds.
The run reports 76 diagnostics and no unowned diagnostic.
`tsc-output.txt`, `tsc-results.json`, and `wall-costs.json` contain the complete evidence.
`tsc-cli-output.txt` contains the CLI syntax-only result. `run.py` repeats both measurements with the same project.

| Site ID: file, function, guard | Code | Message: first 60 characters | Witness | tsc | Block | Verdict | Proposed variant |
|---|---|---|---|---|---|---|---|
| aggregate.rs:34 / check_array_lit / array hole; no spread element | S100 | array holes are not decided | s000/main.ts | accepts | no | needs variant | ArrayHoleConstruction |
| aggregate.rs:72 / check_array_lit / FixedArray context; wrong literal length | S100 | FixedArray length mismatch: the annotation says 2, the liter | s001/main.ts | accepts | no | needs variant | new |
| aggregate.rs:102 / check_array_lit / empty literal; no array context | S100 | cannot infer the type of an empty array literal without cont | s002/main.ts | accepts | no | needs variant | new |
| aggregate.rs:155 / check_descriptor_lit / descriptor spread property | S100 | spread properties are not supported in descriptor literals | s003/main.ts | accepts | no | needs variant | new |
| aggregate.rs:165 / check_descriptor_lit / descriptor key is not an identifier | S100 | descriptor literal member names must be identifiers | s004/main.ts | accepts | no | needs variant | new |
| aggregate.rs:184 / check_descriptor_lit / descriptor property is not key/value or shorthand | S100 | descriptor literals contain data properties only | s005/main.ts | accepts | no | needs variant | new |
| aggregate.rs:194 / check_descriptor_lit / duplicate descriptor member name | S100 | duplicate descriptor literal member `x` | s006/main.ts | TS1117 | no | ok | — |
| aggregate.rs:202 / check_descriptor_lit / literal member has no descriptor field | S004 | descriptor class `D` has no declared property `y` | s007/main.ts | TS2353 | no | ok | — |
| aggregate.rs:242 / check_descriptor_lit / required descriptor field has no value | S100 | descriptor literal for `D` is missing required member `x` | s008/main.ts | TS2741 | no | ok | — |
| aggregate.rs:302 / check_array_spread_lit / array hole in a spread literal | S100 | array holes are not decided | s009/main.ts | accepts | no | needs variant | ArrayHoleConstruction |
| array_of_and_map_copy.rs:31 / check_array_of / Array.of has a type-argument count other than one | S100 | `Array.of<T>` takes exactly one type argument | s010/main.ts | TS2558 | no | ok | — |
| array_of_and_map_copy.rs:87 / check_map_copy / Map source construction has a type-argument count other than two | S100 | `new Map` takes exactly 2 type argument(s) | s011/main.ts | TS2743 | no | ok | — |
| array_of_and_map_copy.rs:107 / check_map_copy / Map source retains a nullable type | S011 | `Map<i32, i32> \| null` may be null here; narrow with a null  | s012/main.ts | accepts | no | needs variant | new |
| assign.rs:34 / check_assign / unsupported assignment; NullishAssign arm | S100 | assignment operator outside the decided surface | s013/main.ts | accepts | yes | ok | — |
| assign.rs:41 / check_assign / unsupported assignment; non-nullish arm | S100 | assignment operator outside the decided surface | s014/main.ts | accepts | no | needs variant | new |
| assign.rs:52 / check_assign / assignment target is a pattern | S100 | a destructuring assignment needs an evaluation and write ord | s015/main.ts | accepts | yes | ok | — |
| assign.rs:98 / check_assign / index-signature assignment in value position | S100 | `a[i] = v` cannot be used as a value | s016/main.ts | accepts | yes | ok | — |
| assign.rs:107 / check_assign / index-signature assignment through readonly | S100 | `a[i] = v` cannot write through a readonly index signature | s017/main.ts | TS2542 | no | ok | — |
| assign.rs:176 / check_assign / static-accessor assignment in value position | S100 | `C.x = v` cannot be used as a value | s018/main.ts | accepts | yes | ok | — |
| assign.rs:192 / check_assign / static-accessor assignment has no setter | S100 | `C.x = v` cannot write through a read-only accessor | s019/main.ts | TS2540 | no | ok | — |
| assign.rs:200 / check_assign / The static setter signature must have exactly one parameter before its insertion. | S100 | static write accessor `{class_name}.{name}` has no parameter | s020/main.ts | TS1049 | — | unreachable | — |
| assign.rs:260 / check_assign / instance-accessor assignment in value position | S100 | `x.x = v` cannot be used as a value | s021/main.ts | accepts | yes | ok | — |
| assign.rs:272 / check_assign / instance-accessor assignment has no setter | S100 | `x.x = v` cannot write through a read-only accessor | s022/main.ts | TS2540 | no | ok | — |
| assign.rs:280 / check_assign / The setter signature must have exactly one parameter before its insertion. | S100 | write accessor `{name}` has no parameter signature | s023/main.ts | TS1049 | — | unreachable | — |
| assign.rs:445 / check_assign_target_inner / local binding is const | S100 | cannot rebind `const` binding `x` | s024/main.ts | TS2588 | no | ok | — |
| assign.rs:466 / check_assign_target_inner / binding is an import | S100 | cannot assign to `x` because it is an import | s025/main.ts | TS2632 | no | ok | — |
| assign.rs:478 / check_assign_target_inner / global binding is const | S100 | cannot rebind `const` binding `x` | s026/main.ts | TS2588 | no | ok | — |
| assign.rs:491 / check_assign_target_inner / name denotes no assignable binding | S100 | `f` is not an assignable binding | s027/main.ts | TS2630 | no | ok | — |
| assign.rs:500 / check_assign_target_inner / simple target is neither identifier nor member | S100 | assignment target outside the decided surface | s028/main.ts | accepts | no | needs variant | new |
| assign.rs:571 / check_member_place / instance accessor has no read signature | S100 | read accessor `x` has no checker signature | s029/main.ts | accepts | no | needs variant | new |
| assign.rs:598 / check_member_place / assignment member has a private name | S100 | private names are not in the decided surface | s030/main.ts | accepts | no | needs variant | new |
| assign.rs:632 / check_namespace_place / static field is readonly | S100 | cannot rebind `const` binding `C.x` | s031/main.ts | TS2540 | no | ok | — |
| assign.rs:646 / check_namespace_place / static accessor has no read signature | S018 | static read accessor `C.x` is missing | s032/main.ts | accepts | no | needs variant | new |
| call.rs:25 / check_call / call callee is not Expr | S100 | call form outside the decided surface | s033/main.ts | TS2337 | no | ok | — |
| call.rs:67 / check_named_call / ordinary or foreign function call has type arguments | S100 | `f` is not generic | s034/main.ts | TS2558 | no | ok | — |
| call.rs:101 / check_named_call / call name denotes a class | S100 | `C` is a class; construct it with `new` | s035/main.ts | TS2348 | no | ok | — |
| call.rs:109 / check_named_call / call name denotes an enum | S100 | enum `E` is not callable | s036/main.ts | TS2349 | no | ok | — |
| call.rs:117 / check_named_call / call name denotes a type alias | S100 | type alias `Fn37` used as a value | s037/main.ts | TS2693 | no | ok | — |
| call.rs:125 / check_named_call / call name denotes a string alias | S100 | string-literal union alias `T` is not callable | s038/main.ts | TS2693 | no | ok | — |
| call.rs:137 / check_named_call / ambient eval call | S002 | no dynamic code evaluation | s039/main.ts | accepts | yes | ok | — |
| call.rs:163 / check_named_call / unreachable call is not a call statement | S100 | `unreachable()` is only legal as a call statement | s040/main.ts | accepts | yes | ok | — |
| call.rs:174 / check_named_call / call name has no scope item or ambient function | S016 | unknown function `missing` | s041/main.ts | TS2304 | no | ok | — |
| call.rs:220 / check_direct_call_with_arguments / generator call precedes its yield-type definition | S100 | generator `gen` is called before its yield type is known; de | s042/main.ts | accepts | no | needs variant | new |
| call.rs:394 / check_context_bytes_call / byte target is neither a value class nor FixedArray | S100 | `Context.bytesOf<T>` cannot use `string`; it is not a @Value | s043/main.ts | accepts | yes | ok | — |
| call.rs:425 / check_context_bytes_call / byte target contains ineligible storage | S100 | `Context.bytesOf<T>` cannot use `FixedArray<string, 1>`; its | s044/main.ts | accepts | yes | ok | — |
| call.rs:476 / check_context_bytes_call / byte argument has a different exact target type | S100 | type mismatch: `Context.bytesOf` expects exactly `FixedArray | s045/main.ts | accepts | no | needs variant | SizedOperandWidths |
| call.rs:629 / check_static_method_call / static method call has unexpected type arguments | S100 | static method `C.f` is not generic | s046/main.ts | TS2558 | no | ok | — |
| call.rs:660 / check_method_call / Promise instance combinator call | S013 | Promise combinator `.then(...)` is not in the language | s047/main.ts | accepts | yes | ok | — |
| call.rs:669 / check_method_call / Promise static call | S013 | Promise static `Promise.resolve(...)` is not in the language | s048/main.ts | accepts | yes | ok | — |
| call.rs:683 / check_method_call / Worker static method is not spawn | S018 | `Worker` has no static method `valueOf` | s049/main.ts | accepts | no | needs variant | NamespaceObjectMember |
| call.rs:775 / instantiate_generic_method_call / generic method lacks explicit type arguments | S100 | generic method `id` requires explicit type arguments | s050/main.ts | accepts | yes | ok | — |
| call.rs:818 / check_method_call_on / method receiver is an unconstrained type parameter | S018 | `T` has no method `foo` | s051/main.ts | TS2339 | no | ok | — |
| call.rs:841 / check_method_call_on / Error.toString call has type arguments | S100 | `toString` is not generic | s052/main.ts | TS2558 | no | ok | — |
| call.rs:883 / check_method_call_on / Worker method name is unknown | S018 | `Worker<M, M>` has no method `valueOf` | s053/main.ts | accepts | no | needs variant | new |
| call.rs:908 / check_method_call_on / Inbox method name is unknown | S018 | `Inbox<M>` has no method `valueOf` | s054/main.ts | accepts | no | needs variant | new |
| call.rs:930 / check_method_call_on / Outbox method name is unknown | S018 | `Outbox<M>` has no method `valueOf` | s055/main.ts | accepts | no | needs variant | new |
| call.rs:966 / check_method_call_on / array.toString call has value arguments | S100 | `toString` expects no arguments | s056/main.ts | TS2554 | no | ok | — |
| call.rs:1014 / check_method_call_on / FixedArray method is outside the callback family | S018 | `FixedArray<i32, 1>` has no method `valueOf` | s057/main.ts | accepts | no | needs variant | new |
| call.rs:1044 / check_method_call_on / generator step-result layout exceeds the aggregate limit | S100 | coroutine step-result layout exceeds the supported aggregate | s058/main.ts | accepts | yes | ok | — |
| call.rs:1067 / check_method_call_on / generator method is not next | S100 | `return` is outside the coroutine surface (next) | s059/main.ts | accepts | no | needs variant | new |
| call.rs:1110 / check_method_call_on / instance call names a static class method | S100 | `C.f` is static and must be accessed through the class name | s060/main.ts | TS2576 | no | ok | — |
| call.rs:1119 / check_method_call_on / class instance has no method signature | S018 | `C` has no method `toString` | s061/main.ts | accepts | no | needs variant | new |
| call.rs:1130 / check_method_call_on / other receiver has no method signature | S018 | `boolean` has no method `valueOf` | s062/main.ts | accepts | no | needs variant | new |
| call.rs:1157 / check_argument_count / fixed argument count is outside its range | S100 | `push` expects 1 argument(s) (1 required), got 2 | s063/main.ts | accepts | no | needs variant | VariadicArguments |
| call.rs:1300 / check_new / new callee is not an identifier | S100 | `new` requires a class name | s064/main.ts | accepts | no | needs variant | new |
| call.rs:1310 / check_new / new identifier names a non-error local value | S100 | `C` names a local value here, not a class | s065/main.ts | TS2351 | no | ok | — |
| call.rs:1319 / check_new / ambient new Function | S002 | no dynamic code evaluation (`new Function`) | s066/main.ts | accepts | yes | ok | — |
| call.rs:1328 / check_new / ambient new Promise | S013 | Promise objects cannot be constructed; async functions expos | s067/main.ts | accepts | yes | ok | — |
| call.rs:1339 / check_new / new Worker, Inbox, or Outbox | S100 | `new Inbox` is rejected; Q35 worker handles and endpoints ar | s068/main.ts | TS2673 | no | ok | — |
| call.rs:1385 / check_new / new Map or Set lacks explicit type arguments | S100 | `new Map` requires explicit type arguments (Q24) | s069/main.ts | accepts | no | needs variant | new |
| call.rs:1394 / check_new / new Map or Set has the wrong type-argument count | S100 | `new Set` takes exactly 1 type argument(s) | s070/main.ts | TS2558 | no | ok | — |
| call.rs:1454 / check_new / new Set has more than one source argument | S100 | `new Set` takes at most one source argument | s071/main.ts | TS2554 | no | ok | — |
| call.rs:1480 / check_new / non-generic class construction has type arguments | S100 | `C` is not generic | s072/main.ts | TS2558 | no | ok | — |
| call.rs:1519 / check_new / generic class construction lacks type arguments | S100 | generic class `C` requires explicit type arguments | s073/main.ts | accepts | no | needs variant | new |
| call.rs:1533 / check_new / new name denotes no class; ambient or ordinary name | S100 | unknown class `Object` | s074/main.ts | accepts | no | needs variant | new |
| call.rs:1541 / check_new / new class is an opaque handle | S100 | opaque handle `H` is obtained from the host, not constructed | s075/main.ts | TS2693 | no | ok | — |
| call.rs:1558 / check_new / new class is ambient and not a boundary mirror | S100 | ambient class `C` is obtained from the host, not constructed | s076/main.ts | accepts | yes | ok | — |
| call.rs:1570 / check_new / new class is a descriptor | S100 | descriptor class `D` is constructed with an object literal,  | s077/main.ts | accepts | yes | ok | — |
| entry.rs:167 / check_expr_with_header_receiver / checked expression has type Void | S100 | a `void` expression is only allowed as an expression stateme | s078/main.ts | accepts | yes | ok | — |
| entry.rs:199 / check_expr_inner / descriptor default reads this | S100 | §147 rule 3a: `this` is forbidden in a descriptor member def | s079/main.ts | accepts | yes | ok | — |
| entry.rs:215 / check_expr_inner / field initializer uses a forbidden this form | S100 | §147 rule 2: `this` as a value is forbidden | s080/main.ts | accepts | yes | ok | — |
| entry.rs:236 / check_expr_inner / this frame has no receiver and has a divergence | S100 | `this` is only available in constructors and methods | s081/main.ts | accepts | yes | ok | — |
| entry.rs:243 / check_expr_inner / this frame has no receiver and no divergence | S100 | `this` is only available in constructors and methods | s082/main.ts | accepts | no | needs variant | new |
| entry.rs:270 / check_expr_inner / object literal has a plain-class context | S005 | object literals do not satisfy nominal class types | s083/main.ts | accepts | yes | ok | — |
| entry.rs:281 / check_expr_inner / object literal has no descriptor context | S100 | object literals are not in the decided surface | s084/main.ts | accepts | yes | ok | — |
| entry.rs:296 / check_expr_inner / non-null assertion expression | S100 | the `!` assertion is not in the decided surface; narrow with | s085/main.ts | accepts | no | needs variant | DefiniteAssignmentAssertion |
| entry.rs:304 / check_expr_inner / function expression | S100 | function expressions are not in the decided surface; use an  | s086/main.ts | accepts | no | needs variant | new |
| entry.rs:313 / check_expr_inner / unsupported expression node | S100 | expression form outside the decided surface | s087/main.ts | accepts | no | needs variant | new |
| entry.rs:373 / reject_embedded_header_copy / boundary extension copies its embedded header | S100 | embedded header `Ext.header` cannot be copied as `Header`; s | s088/main.ts | accepts | yes | ok | — |
| entry.rs:444 / check_await / await frame is not async | S013 | `await` is only legal inside an async function | s089/main.ts | accepts | yes | ok | — |
| entry.rs:461 / check_await / non-call await operand has no async-handle type | S100 | `await` requires `Context.suspend()`, an async call, or a he | s090/main.ts | accepts | no | needs variant | new |
| entry.rs:478 / check_await / await call callee is not Expr | S100 | awaitable expressions must be direct calls | s091/main.ts | TS2337 | no | ok | — |
| entry.rs:495 / check_await / Context.suspend has type or value arguments | S100 | `Context.suspend()` takes no type arguments or value argumen | s092/main.ts | TS2554 | no | ok | — |
| entry.rs:521 / check_await / await call names a local value | S100 | an async awaitable cannot be called through a local value | s093/main.ts | accepts | no | needs variant | new |
| entry.rs:559 / check_await / await call names no declared function | S100 | `print` is not a directly declared async function | s094/main.ts | accepts | no | needs variant | new |
| entry.rs:571 / check_await / await call names a synchronous function | S100 | `g` is synchronous and cannot be awaited | s095/main.ts | accepts | no | needs variant | new |
| entry.rs:579 / check_await / await non-generic function has type arguments | S100 | `f` is not generic | s096/main.ts | TS2558 | no | ok | — |
| entry.rs:604 / check_await / await method has a non-identifier property | S100 | an awaited async method requires an identifier method name | s097/main.ts | accepts | no | needs variant | new |
| entry.rs:620 / check_await / await receiver is not a class | S018 | type `string` has no async method `toString` | s098/main.ts | accepts | no | needs variant | new |
| entry.rs:645 / check_await / await class has no method signature | S018 | `C` has no method `toString` | s099/main.ts | accepts | no | needs variant | new |
| entry.rs:653 / check_await / await method is synchronous | S100 | method `f` is synchronous and cannot be awaited | s100/main.ts | accepts | no | needs variant | new |
| entry.rs:661 / check_await / await non-generic method has type arguments | S100 | method `f` is not generic | s101/main.ts | TS2558 | no | ok | — |
| entry.rs:682 / check_await / await callee is neither an identifier nor a member | S100 | an async awaitable must directly call a named async function | s102/main.ts | accepts | no | needs variant | new |
| lambda.rs:47 / check_lambda_with / async arrow | S100 | async arrow functions are not in the decided surface; use an | s103/main.ts | accepts | yes | ok | — |
| lambda.rs:56 / check_lambda_with / The TypeScript parser never creates a generator arrow. | S100 | generator arrows are not in the decided surface | s104/main.ts | accepts | — | unreachable | — |
| lambda.rs:193 / check_lambda_with / block arrow has no result annotation or context | S100 | a lambda with a block body requires a return type annotation | s105/main.ts | accepts | no | needs variant | new |
| lambda.rs:211 / check_lambda_with / non-void block arrow fails always_returns | S100 | not all paths return a value | s106/main.ts | accepts | no | needs variant | new |
| literal.rs:55 / check_lit / regex literal has the v flag | S100 | the `v` flag requires ES2024 in a regex literal; use `new Re | s107/main.ts | TS1501 | no | ok | — |
| literal.rs:71 / check_lit / regex literal fails regress validation | S100 | invalid regular-expression literal: Unbalanced parenthesis | s108/main.ts | TS1005 | no | ok | — |
| literal.rs:130 / check_lit / unsupported literal node | S100 | literal form outside the decided surface | s109/main.ts | accepts | no | needs variant | new |
| literal.rs:185 / check_num_lit / f16 literal exceeds its range | S008 | numeric literal 70000 out of range for `f16` | s110/main.ts | accepts | no | needs variant | new |
| literal.rs:200 / check_num_lit / fractional literal has an integer context | S008 | fractional literal in integer context `i32` | s111/main.ts | accepts | no | needs variant | new |
| literal.rs:223 / check_num_lit / integer literal exceeds its contextual range | S008 | integer literal 128 out of range for `i8` | s112/main.ts | accepts | yes | ok | — |
| literal.rs:270 / check_template / template expression type has no interpolation | S100 | type `C` cannot be interpolated into a template | s113/main.ts | accepts | no | needs variant | new |
| literal.rs:295 / check_ident / identifier is undefined | S012 | `undefined` is banned; the single null story is `null` | s114/main.ts | accepts | yes | ok | — |
| literal.rs:333 / check_ident / namespace import value has no context | S100 | namespace import `ns` is a static qualifier and cannot be us | s115/main.ts | accepts | yes | ok | — |
| literal.rs:340 / check_ident / namespace import value has a context | S100 | namespace import `ns` is a static qualifier and cannot be us | s116/main.ts | accepts | no | needs variant | new |
| literal.rs:375 / check_ident / function value is async or a generator | S100 | async functions are not first-class values; call them direct | s117/main.ts | accepts | no | needs variant | new |
| literal.rs:394 / check_ident / generic function is read as a value | S100 | generic function `id` requires explicit type arguments | s118/main.ts | accepts | no | needs variant | new |
| literal.rs:405 / check_ident / class is read as a value | S100 | class `C` used as a value | s119/main.ts | accepts | no | needs variant | new |
| literal.rs:413 / check_ident / enum is read as a value | S100 | enum `E` used as a value; use a member | s120/main.ts | accepts | no | needs variant | new |
| literal.rs:421 / check_ident / type alias is read as a value | S100 | type alias `Fn121` used as a value | s121/main.ts | TS2693 | no | ok | — |
| literal.rs:429 / check_ident / string alias is read as a value | S100 | string-literal union alias `T` used as a value | s122/main.ts | TS2693 | no | ok | — |
| literal.rs:437 / check_ident / foreign function is read as a value | S100 | foreign function `host` may only be called | s123/main.ts | accepts | no | needs variant | new |
| literal.rs:457 / check_ident / ambient eval or Function is read as a value | S002 | no dynamic code evaluation | s124/main.ts | accepts | yes | ok | — |
| literal.rs:531 / check_ident / ambient function is read as a value | S100 | ambient function `print` may only be called | s125/main.ts | accepts | no | needs variant | new |
| literal.rs:554 / check_ident / identifier has no declaration or ambient value | S016 | unknown name `missing` | s126/main.ts | TS2304 | no | ok | — |
| member.rs:35 / check_member_read_inner / descriptor numeric operand reads an optional this field | S100 | §147 rule 3a: `this` is forbidden in a descriptor member def | s127/main.ts | accepts | no | needs variant | new |
| member.rs:73 / check_member_read_inner / initializer reads a field without an earlier initializer | S100 | §147 rule 2: `this.a` must read an earlier instance field wi | s128/main.ts | accepts | no | needs variant | ThisInFieldInitializer |
| member.rs:75 / check_member_read_inner / initializer reads a method or accessor | S100 | §147 rule 2: `this.f` must read an earlier instance field wi | s129/main.ts | accepts | yes | ok | — |
| member.rs:124 / check_member_read_inner / absence-capable descriptor member lacks presence narrowing | S100 | an absence-capable descriptor member requires the present ar | s130/main.ts | accepts | yes | ok | — |
| member.rs:135 / check_member_read_inner / read member has a private name | S100 | private names are not in the decided surface | s131/main.ts | accepts | no | needs variant | new |
| member.rs:190 / check_index / array index type is not i32 | S100 | array indices are `i32`, got `i64` | s132/main.ts | accepts | no | needs variant | SizedOperandWidths |
| member.rs:201 / check_index / FixedArray index type is not i32 | S100 | array indices are `i32`, got `i64` | s133/main.ts | accepts | no | needs variant | SizedOperandWidths |
| member.rs:209 / check_index / literal FixedArray index exceeds its length | S100 | index 2 out of bounds for FixedArray length 1 | s134/main.ts | accepts | no | needs variant | new |
| member.rs:221 / check_index / receiver type has no index operation | S100 | type `string` is not indexable | s135/main.ts | accepts | no | needs variant | new |
| member.rs:253 / member_on / member receiver is an unconstrained type parameter | S018 | `T` has no member `foo` | s136/main.ts | TS2339 | no | ok | — |
| member.rs:261 / member_on / instance member name is prototype | S003 | no prototype mutation | s137/main.ts | accepts | yes | ok | — |
| member.rs:305 / member_on / instance accessor has no read signature | S100 | read accessor `x` has no checker signature | s138/main.ts | accepts | no | needs variant | new |
| member.rs:329 / member_on / instance member names a static declaration | S100 | `C.x` is static and must be accessed through the class name | s139/main.ts | TS2576 | no | ok | — |
| member.rs:339 / member_on / write names no class property | S004 | nominal types are closed: `C` has no property `toString` | s140/main.ts | accepts | no | needs variant | new |
| member.rs:348 / member_on / ordinary instance method is read as a value | S100 | async method `f` is not a first-class value; call it directl | s141/main.ts | accepts | no | needs variant | new |
| member.rs:360 / member_on / generic instance method is read as a value | S100 | async method `f` is not a first-class value; call it directl | s142/main.ts | accepts | no | needs variant | new |
| member.rs:372 / member_on / class instance has no member signature | S018 | `C` has no member `toString` | s143/main.ts | accepts | no | needs variant | new |
| member.rs:397 / member_on / array method is read as a value | S100 | method `push` may only be called, not read as a value | s144/main.ts | accepts | no | needs variant | CompilerOwnedValue |
| member.rs:408 / member_on / FixedArray callback method is read as a value | S100 | method `map` may only be called, not read as a value | s145/main.ts | accepts | no | needs variant | CompilerOwnedValue |
| member.rs:421 / member_on / unknown FixedArray member | S100 | `constructor` is outside the FixedArray surface (length, ind | s146/main.ts | accepts | no | needs variant | new |
| member.rs:445 / member_on / Map method is read as a value | S100 | method `get` may only be called, not read as a value | s147/main.ts | accepts | no | needs variant | CompilerOwnedValue |
| member.rs:453 / member_on / unknown Map member | S100 | `Map` has no accepted member `constructor` (Q24) | s148/main.ts | accepts | no | needs variant | new |
| member.rs:473 / member_on / Set method is read as a value | S100 | method `add` may only be called, not read as a value | s149/main.ts | accepts | no | needs variant | CompilerOwnedValue |
| member.rs:481 / member_on / unknown Set member | S100 | `Set` has no accepted member `constructor` (Q24) | s150/main.ts | accepts | no | needs variant | new |
| member.rs:501 / member_on / string method is read as a value | S100 | method `slice` may only be called, not read as a value | s151/main.ts | accepts | no | needs variant | CompilerOwnedValue |
| member.rs:550 / member_on / RegExp member is outside its read surface | S100 | method `test` may only be called, not read as a value | s152/main.ts | accepts | no | needs variant | CompilerOwnedValue |
| member.rs:556 / member_on / write to a coroutine step result | S100 | coroutine step results are read-only | s153/main.ts | accepts | no | needs variant | new |
| member.rs:581 / member_on / unknown coroutine step-result member | S100 | `toString` is not part of the coroutine step result ({ done, | s154/main.ts | accepts | no | needs variant | new |
| member.rs:634 / member_on / unknown numeric member | S018 | `i32` has no member `valueOf` | s155/main.ts | accepts | no | needs variant | new |
| member.rs:643 / member_on / object receiver has no narrowed class type | S100 | `object` is boundary-opaque; narrow it with `as` before memb | s156/main.ts | accepts | no | needs variant | new |
| member.rs:652 / member_on / other receiver has no member signature | S018 | `boolean` has no member `valueOf` | s157/main.ts | accepts | no | needs variant | new |
| method.rs:36 / check_number_method / unknown numeric method | S018 | `f64` has no method `valueOf` | s158/main.ts | accepts | no | needs variant | new |
| method.rs:242 / str_surface_error / string member has no accepted or rejection-table entry | S100 | `valueOf` is outside the string surface (length, slice, `+`  | s159/main.ts | accepts | no | needs variant | new |
| method.rs:464 / check_array_method / splice argument count is not one or two | S100 | `splice` expects 1 or 2 arguments (start, deleteCount), got  | s160/main.ts | TS2555 | no | ok | — |
| method.rs:505 / check_array_method / unshift argument count is not one | S100 | `unshift` expects 1 argument (value), got 0 | s161/main.ts | accepts | no | needs variant | VariadicArguments |
| method.rs:520 / check_array_method / copyWithin argument count is not two or three | S100 | `copyWithin` expects 2 or 3 arguments (target, start, end?), | s162/main.ts | TS2554 | no | ok | — |
| method.rs:553 / check_array_method / sort argument count is greater than one | S100 | `sort` expects 1 argument (the comparator), got 2 | s163/main.ts | TS2554 | no | ok | — |
| method.rs:583 / check_array_method / reduce argument count is greater than two | S100 | `reduce` expects 2 arguments (callback, init), got 3 | s164/main.ts | TS2554 | no | ok | — |
| method.rs:681 / check_array_method / callback-family argument count is not one | S100 | `map` expects 1 argument (the callback), got 2 | s165/main.ts | accepts | no | needs variant | new |
| method.rs:736 / check_array_method / map callback returns Void | S100 | the `map` callback must return a value | s166/main.ts | accepts | yes | ok | — |
| method.rs:809 / check_map_group_by / Map.groupBy argument count is not two | S100 | `Map.groupBy` expects an array and a callback, got 0 argumen | s167/main.ts | TS2554 | no | ok | — |
| method.rs:825 / check_map_group_by / Map.groupBy items have no Array type | S100 | `Map.groupBy` items must be a `T[]`, got `C` | s168/main.ts | accepts | no | needs variant | new |
| method.rs:843 / check_map_group_by / Map.groupBy callback returns Void | S100 | `Map.groupBy` callback must return a key | s169/main.ts | accepts | no | needs variant | new |
| method.rs:853 / check_map_group_by / expect_callback_shape returns only Func or Error; the match excludes both. | S100 | `Map.groupBy` callback is not a function, got `{actual}` | s170/main.ts | TS2345 | — | unreachable | — |
| method.rs:1070 / check_map_method / unknown Map method | S100 | `Map` has no accepted method `valueOf` (Q24) | s171/main.ts | accepts | no | needs variant | new |
| method.rs:1153 / check_map_method / Map.forEach argument count is not one | S100 | `Map.forEach` expects exactly 1 callback, got 2 | s172/main.ts | accepts | no | needs variant | new |
| method.rs:1190 / check_set_method / unknown Set method | S100 | `Set` has no accepted method `valueOf` (Q24) | s173/main.ts | accepts | no | needs variant | new |
| method.rs:1228 / check_set_method / Set.forEach argument count is not one | S100 | `Set.forEach` expects exactly 1 callback, got 2 | s174/main.ts | accepts | no | needs variant | new |
| method.rs:1255 / check_set_method / Set algebra argument count is not one | S100 | `Set.union` expects exactly 1 Set argument, got 0 | s175/main.ts | TS2554 | no | ok | — |
| method.rs:1544 / expect_callback_shape / callback parameter or result types fail its fixed shape | S100 | type mismatch: the `map` callback expects `(i32) or (i32, i3 | s176/main.ts | accepts | no | needs variant | MethodTypeDomain |
| method.rs:1570 / arr_surface_error / array member has no accepted or rejection-table entry | S100 | `valueOf` is outside the array surface (length, indexing, pu | s177/main.ts | accepts | no | needs variant | new |
| namespace.rs:155 / check_namespace_member / class namespace member is prototype | S003 | no prototype mutation | s178/main.ts | accepts | yes | ok | — |
| namespace.rs:167 / check_namespace_member / All class static-field writes stop in check_namespace_place before this for_write guard. | S100 | cannot rebind `const` binding `{class_name}.{prop}` | s179/main.ts | TS2540 | — | unreachable | — |
| namespace.rs:186 / check_namespace_member / class namespace accessor has no read signature | S018 | static read accessor `C.x` is missing | s180/main.ts | accepts | no | needs variant | new |
| namespace.rs:205 / check_namespace_member / class static method is read as a value | S100 | static method `C.f` may only be called | s181/main.ts | accepts | no | needs variant | new |
| namespace.rs:212 / check_namespace_member / unknown class static member | S018 | class `C` has no static member `name` | s182/main.ts | accepts | no | needs variant | new |
| namespace.rs:221 / check_namespace_member / generic class namespace member is prototype | S003 | no prototype mutation | s183/main.ts | accepts | yes | ok | — |
| namespace.rs:228 / check_namespace_member / unknown generic-class static member | S018 | generic class `C` has no static member `name` | s184/main.ts | accepts | no | needs variant | new |
| namespace.rs:249 / check_namespace_member / unknown enum member | S018 | enum `E` has no member `toString` | s185/main.ts | accepts | no | needs variant | new |
| namespace.rs:259 / check_namespace_member / namespace receiver is a type alias | S100 | type alias `Fn186` used as a value | s186/main.ts | TS2693 | no | ok | — |
| namespace.rs:267 / check_namespace_member / namespace receiver is a string alias | S100 | string-literal union alias `T` has no static values | s187/main.ts | TS2693 | no | ok | — |
| namespace.rs:281 / check_namespace_member / Object.setPrototypeOf is read as a value | S003 | no prototype mutation | s188/main.ts | accepts | yes | ok | — |
| namespace.rs:516 / check_number_global_call / Both callers pass only ParseInt or ParseFloat to this function. | S100 | internal Q25 parser identity mismatch | s189/main.ts | accepts | — | unreachable | — |
| namespace.rs:633 / validate_message_class / worker message contains a non-transferable field | S100 | message class `M` is not transferable: innermost field `M.a` | s190/main.ts | accepts | yes | ok | — |
| namespace.rs:659 / check_worker_spawn / Worker.spawn argument count or spread fails | S100 | `Worker.spawn` expects one directly named entry function, go | s191/main.ts | accepts | no | needs variant | WorkerEntryShape |
| namespace.rs:675 / check_worker_spawn / Worker.spawn argument is not an identifier | S100 | `Worker.spawn` entry must be a directly named module-level f | s192/main.ts | accepts | yes | ok | — |
| namespace.rs:691 / check_worker_spawn / Worker.spawn identifier names a local value | S100 | `Worker.spawn` entry must name a module-level function direc | s193/main.ts | accepts | no | needs variant | WorkerEntryShape |
| namespace.rs:705 / check_worker_spawn / Worker.spawn identifier denotes no ordinary function | S100 | `Worker.spawn` entry must name a non-generic module-level fu | s194/main.ts | accepts | no | needs variant | new |
| namespace.rs:716 / check_worker_spawn / Worker.spawn entry is async | S100 | `Worker.spawn` entry must be synchronous; async worker entri | s195/main.ts | accepts | yes | ok | — |
| namespace.rs:729 / check_worker_spawn / worker entry result, arity, defaults, or generator flag fails | S100 | `Worker.spawn` entry must have the exact synchronous shape ` | s196/main.ts | accepts | no | needs variant | new |
| namespace.rs:742 / check_worker_spawn / worker parameters are not Inbox and Outbox | S100 | `Worker.spawn` entry must have the exact synchronous shape ` | s197/main.ts | accepts | no | needs variant | new |
| namespace.rs:758 / check_worker_spawn / Worker.spawn explicit type-argument count is not two | S100 | `Worker.spawn` takes exactly two explicit type arguments | s198/main.ts | TS2558 | no | ok | — |
| namespace.rs:768 / check_worker_spawn / Worker.spawn explicit types differ from endpoint types | S100 | `Worker.spawn` type arguments must exactly match its entry's | s199/main.ts | accepts | no | needs variant | NominalClassIdentity |
| namespace.rs:815 / check_regex_new / RegExp constructor has type arguments | S100 | `RegExp` is not generic | s200/main.ts | TS2558 | no | ok | — |
| namespace.rs:884 / check_regex_method / unknown RegExp method | S100 | `RegExp` has no accepted method `compile` | s201/main.ts | accepts | no | needs variant | new |
| namespace.rs:929 / check_string_pattern_method / replaceAll regex literal lacks g | S100 | `string.replaceAll` with a RegExp literal requires the `g` f | s202/main.ts | accepts | yes | ok | — |
| namespace.rs:944 / check_string_pattern_method / string pattern-method argument count fails | S100 | `split` expects 1 or 2 argument(s), got 0 | s203/main.ts | TS2554 | no | ok | — |
| operator.rs:53 / check_unary / unary minus operand is not numeric | S100 | unary `-` requires a numeric operand, got `string` | s204/main.ts | accepts | no | needs variant | new |
| operator.rs:82 / check_unary / logical-not operand is not boolean | S100 | `!` requires a boolean operand, got `i32` | s205/main.ts | accepts | no | needs variant | new |
| operator.rs:106 / check_unary / bitwise-not operand is not an integer | S100 | `~` requires an integer operand, got `f64` | s206/main.ts | accepts | no | needs variant | new |
| operator.rs:134 / check_unary / delete expression | S100 | the `delete` operator is not in the language; use `Context.f | s207/main.ts | accepts | no | needs variant | new |
| operator.rs:142 / check_unary / unsupported unary operator | S100 | unary operator outside the decided surface | s208/main.ts | accepts | no | needs variant | new |
| operator.rs:201 / check_update / index-signature update in value position | S100 | `a[i]++` cannot be used as a value | s209/main.ts | accepts | yes | ok | — |
| operator.rs:220 / check_update / accessor update in value position | S100 | `x.x++` cannot be used as a value | s210/main.ts | accepts | yes | ok | — |
| operator.rs:239 / check_update / update target is not numeric | S100 | `++`/`--` require a numeric target, got `boolean` | s211/main.ts | TS2356 | no | ok | — |
| operator.rs:276 / check_update / update through a readonly index signature | S100 | `a[i] = v` cannot write through a readonly index signature | s212/main.ts | TS2542 | no | ok | — |
| operator.rs:346 / check_update / accessor update has no setter | S100 | `x.x++` cannot write through a read-only accessor | s213/main.ts | TS2540 | no | ok | — |
| operator.rs:378 / check_update / The setter signature must have exactly one parameter before its insertion. | S100 | write accessor `{name}` has no parameter signature | s214/main.ts | TS1049 | — | unreachable | — |
| operator.rs:456 / check_bin / logical operand is not boolean | S100 | logical operators require booleans, got `i32` | s215/main.ts | accepts | no | needs variant | new |
| operator.rs:495 / check_bin / binary operator is in or exponentiation | S100 | operator outside the decided surface | s216/main.ts | accepts | no | needs variant | new |
| operator.rs:537 / reject_unbound_optional_chain / value-position optional chain has no fallback | S012 | an optional chain has type `i32 \| undefined` in TypeScript;  | s217/main.ts | accepts | yes | ok | — |
| operator.rs:562 / check_optional_chain_statement / statement optional chain does not end in a call | S012 | an optional chain has type `i32 \| undefined` in TypeScript;  | s218/main.ts | accepts | yes | ok | — |
| operator.rs:607 / check_optional_plan / optional member has a tested computed key | S100 | an optional chain cannot use `?.[i]`; narrow the receiver an | s219/main.ts | accepts | yes | ok | — |
| operator.rs:642 / check_optional_plan / member-step optional call has tested=true | S100 | an optional call through `?.()` is not in the decided surfac | s220/main.ts | accepts | no | needs variant | new |
| operator.rs:679 / check_optional_plan / standalone optional call has tested=true | S100 | an optional call through `?.()` is not in the decided surfac | s221/main.ts | accepts | no | needs variant | new |
| operator.rs:733 / check_optional_member / optional member has a private name | S100 | private names are not in the decided surface | s222/main.ts | TS18030 | no | ok | — |
| operator.rs:844 / require_nullable_operand / tested optional receiver has no Nullable type | S100 | the tested receiver has type `C`, which is not nullable | s223/main.ts | accepts | yes | ok | — |
| operator.rs:968 / check_absence_presence_comparison / both comparison operands are undefined | S012 | `undefined` is legal only in a presence test on an absence-c | s224/main.ts | accepts | no | needs variant | GeneralUnionAndUndefined |
| operator.rs:986 / check_absence_presence_comparison / undefined comparison has no absence-capable member | S012 | `undefined` is legal only in a presence test on an absence-c | s225/main.ts | accepts | no | needs variant | GeneralUnionAndUndefined |
| operator.rs:1236 / bin_result / compound operation is invalid and not mixed numeric | S100 | operator `+=` is not defined for `E` and `i32` | s226/main.ts | accepts | no | needs variant | new |
| operator.rs:1286 / bin_result / binary operands have different numeric types | S007 | mixed-type arithmetic (`i64` and `i32`) requires an explicit | s227/main.ts | accepts | yes | ok | — |
| operator.rs:1296 / bin_result / binary operation has no defined operand pair | S100 | operator not defined for `E` and `E` | s228/main.ts | accepts | no | needs variant | new |
| operator.rs:1382 / check_cond / conditional-expression test is not boolean | S100 | condition must be boolean, got `i32` | s229/main.ts | accepts | no | needs variant | new |
| operator.rs:1443 / check_cond / conditional branches have no common type | S100 | conditional branches have no common type: `i32` and `string` | s230/main.ts | accepts | yes | ok | — |
| operator.rs:1480 / check_yield / The parser rejects yield outside a generator; nested arrows also reject it before the checker. | S100 | `yield` is only available inside a `function*` coroutine | s231/main.ts | TS1163 | — | unreachable | — |
| operator.rs:1488 / check_yield / delegated yield | S100 | `yield*` delegation is not in the decided surface | s232/main.ts | accepts | no | needs variant | new |
| operator.rs:1514 / check_yield / bare yield follows a non-void yield type | S100 | a bare `yield;` requires `void`, but the generator element t | s233/main.ts | accepts | no | needs variant | GeneralUnionAndUndefined |
| operator.rs:1576 / check_as / cast pair is outside the accepted conversion kinds | S100 | `as` converts between sized numerics, enum to integer, or na | s234/main.ts | accepts | no | needs variant | new |
| namespace.rs:22 / check_receiver / receiver retains a Nullable type at member use | S011 | `C \| null` may be null here; narrow with a null check first | s235/main.ts | accepts | no | needs variant | new |
| call.rs:550 / check_indirect_call / indirect callee has no Func type | S100 | type `((i32) => i32) \| null` is not callable | s236/main.ts | accepts | no | needs variant | new |

The witnesses of unreachable rows are probes. Their diagnostics do not come from the named target.
A missing first setter parameter cannot survive the parser and `class_shape.rs` signature guard.
The callback shape helper returns Func or Error; `Map.groupBy` excludes both before its non-function error arm.
The static-field place helper handles every class static-field write before the namespace write guard.
The number parser callers supply only ParseInt or ParseFloat.
The SWC parser supplies no generator arrow and no yield in a non-generator checker frame.

| Shared-site control | Code | Message: first 60 characters | Witness | tsc | Block | Verdict |
|---|---|---|---|---|---|---|
| member.rs:261 / member_on / member name is prototype | S003 | no prototype mutation | control-prototype/main.ts | TS2339 | yes | wrong |

The requested `const n: i32 = 3; if (n) {}` program is `known-if/main.ts`.
It reaches `compiler/src/check/stmt.rs:535`, outside group a: S100, tsc accepts, no block, needs variant `new`.
`s229/main.ts` changes the condition to `n ? 1 : 2` and reaches group a at `operator.rs:1382`.
`known-pair/main.ts` reaches `compiler/src/check/inference.rs:149`, outside group a.
It reports S100 conflicting candidates, tsc accepts, and renders `GenericInferenceCandidates`: ok.
`s118/main.ts` tests `apply(id, 3)` and reaches `literal.rs:394`: S100, tsc accepts, no block, needs variant `new`.
The two outside-group sites do not enter group a's counts.

| Requested case | Site | Code | Message: first 60 characters | Witness | tsc | Block | Verdict |
|---|---|---|---|---|---|---|---|
| Numeric if condition | stmt.rs:535 / require_bool / condition is not boolean | S100 | condition must be boolean, got `i32` | known-if/main.ts | accepts | no | needs variant: new |
| pair with a literal conditional | inference.rs:149 / infer_call_arguments / conflicting candidates | S100 | cannot infer type parameter `T` of `pair`: conflicting candi | known-pair/main.ts | accepts | yes | ok |
| Generic function value in apply | literal.rs:394 / check_ident / generic function value | S100 | generic function `id` requires explicit type arguments | s118/main.ts | accepts | no | needs variant: new |

Variant proposals below name an existing variant only when its `why` states the reason.
The word `new` means that the current table supplies no matching reason.

## ArrayHoleConstruction: 2 sites

- `aggregate.rs:34` / `check_array_lit`: array hole; no spread element.
- `aggregate.rs:302` / `check_array_spread_lit`: array hole in a spread literal.

Existing reason: The language has no array hole and no missing-element value, so a filled array changes what a read means.

## CompilerOwnedValue: 6 sites

- `member.rs:397` / `member_on`: array method is read as a value.
- `member.rs:408` / `member_on`: FixedArray callback method is read as a value.
- `member.rs:445` / `member_on`: Map method is read as a value.
- `member.rs:473` / `member_on`: Set method is read as a value.
- `member.rs:501` / `member_on`: string method is read as a value.
- `member.rs:550` / `member_on`: RegExp member is outside its read surface.

Existing reason: Compiler-owned namespaces and methods lower to direct operations; the language has no value or writable storage for them.

## DefiniteAssignmentAssertion: 1 sites

- `entry.rs:296` / `check_expr_inner`: non-null assertion expression.

Existing reason: The assertion asks `tsc` to trust the author; this language has no null check on a non-nullable reference.

## GeneralUnionAndUndefined: 3 sites

- `operator.rs:968` / `check_absence_presence_comparison`: both comparison operands are undefined.
- `operator.rs:986` / `check_absence_presence_comparison`: undefined comparison has no absence-capable member.
- `operator.rs:1514` / `check_yield`: bare yield follows a non-void yield type.

Existing reason: A general union has no single C layout, so the one union form is a nullable reference and `undefined` stays out.

## MethodTypeDomain: 1 sites

- `method.rs:1544` / `expect_callback_shape`: callback parameter or result types fail its fixed shape.

Existing reason: Each method has a fixed receiver, element, result, and accumulator domain; TypeScript generic method domains include more kinds.

## NamespaceObjectMember: 1 sites

- `call.rs:683` / `check_method_call`: Worker static method is not spawn.

Existing reason: Compiler namespaces expose only declared intrinsics; JavaScript prototype members and inherited Object methods have no namespace representation.

## NominalClassIdentity: 1 sites

- `namespace.rs:768` / `check_worker_spawn`: Worker.spawn explicit types differ from endpoint types.

Existing reason: Each class declaration is one nominal type, so a class with the same shape is a different type.

## SizedOperandWidths: 3 sites

- `call.rs:476` / `check_context_bytes_call`: byte argument has a different exact target type.
- `member.rs:190` / `check_index`: array index type is not i32.
- `member.rs:201` / `check_index`: FixedArray index type is not i32.

Existing reason: An implicit conversion hides a width change, so every mixed-width operand and argument takes an explicit `as`.

## ThisInFieldInitializer: 1 sites

- `member.rs:73` / `check_member_read_inner`: initializer reads a field without an earlier initializer.

Existing reason: Initializers read only earlier initialized fields (§147 rule 2). Descriptor defaults forbid `this` (§147 rule 3a). Other uses expose the partial instance.

## VariadicArguments: 2 sites

- `call.rs:1157` / `check_argument_count`: fixed argument count is outside its range.
- `method.rs:505` / `check_array_method`: unshift argument count is not one.

Existing reason: The language has no variadic parameter, so every call takes a fixed argument count.

## WorkerEntryShape: 2 sites

- `namespace.rs:659` / `check_worker_spawn`: Worker.spawn argument count or spread fails.
- `namespace.rs:691` / `check_worker_spawn`: Worker.spawn identifier names a local value.

Existing reason: A worker starts on another thread with its own Context, so its entry is a named, non-capturing, synchronous module function.

## new: 100 sites

- `aggregate.rs:72` / `check_array_lit`: FixedArray context; wrong literal length.
- `aggregate.rs:102` / `check_array_lit`: empty literal; no array context.
- `aggregate.rs:155` / `check_descriptor_lit`: descriptor spread property.
- `aggregate.rs:165` / `check_descriptor_lit`: descriptor key is not an identifier.
- `aggregate.rs:184` / `check_descriptor_lit`: descriptor property is not key/value or shorthand.
- `array_of_and_map_copy.rs:107` / `check_map_copy`: Map source retains a nullable type.
- `assign.rs:41` / `check_assign`: unsupported assignment; non-nullish arm.
- `assign.rs:500` / `check_assign_target_inner`: simple target is neither identifier nor member.
- `assign.rs:571` / `check_member_place`: instance accessor has no read signature.
- `assign.rs:598` / `check_member_place`: assignment member has a private name.
- `assign.rs:646` / `check_namespace_place`: static accessor has no read signature.
- `call.rs:220` / `check_direct_call_with_arguments`: generator call precedes its yield-type definition.
- `call.rs:883` / `check_method_call_on`: Worker method name is unknown.
- `call.rs:908` / `check_method_call_on`: Inbox method name is unknown.
- `call.rs:930` / `check_method_call_on`: Outbox method name is unknown.
- `call.rs:1014` / `check_method_call_on`: FixedArray method is outside the callback family.
- `call.rs:1067` / `check_method_call_on`: generator method is not next.
- `call.rs:1119` / `check_method_call_on`: class instance has no method signature.
- `call.rs:1130` / `check_method_call_on`: other receiver has no method signature.
- `call.rs:1300` / `check_new`: new callee is not an identifier.
- `call.rs:1385` / `check_new`: new Map or Set lacks explicit type arguments.
- `call.rs:1519` / `check_new`: generic class construction lacks type arguments.
- `call.rs:1533` / `check_new`: new name denotes no class; ambient or ordinary name.
- `entry.rs:243` / `check_expr_inner`: this frame has no receiver and no divergence.
- `entry.rs:304` / `check_expr_inner`: function expression.
- `entry.rs:313` / `check_expr_inner`: unsupported expression node.
- `entry.rs:461` / `check_await`: non-call await operand has no async-handle type.
- `entry.rs:521` / `check_await`: await call names a local value.
- `entry.rs:559` / `check_await`: await call names no declared function.
- `entry.rs:571` / `check_await`: await call names a synchronous function.
- `entry.rs:604` / `check_await`: await method has a non-identifier property.
- `entry.rs:620` / `check_await`: await receiver is not a class.
- `entry.rs:645` / `check_await`: await class has no method signature.
- `entry.rs:653` / `check_await`: await method is synchronous.
- `entry.rs:682` / `check_await`: await callee is neither an identifier nor a member.
- `lambda.rs:193` / `check_lambda_with`: block arrow has no result annotation or context.
- `lambda.rs:211` / `check_lambda_with`: non-void block arrow fails always_returns.
- `literal.rs:130` / `check_lit`: unsupported literal node.
- `literal.rs:185` / `check_num_lit`: f16 literal exceeds its range.
- `literal.rs:200` / `check_num_lit`: fractional literal has an integer context.
- `literal.rs:270` / `check_template`: template expression type has no interpolation.
- `literal.rs:340` / `check_ident`: namespace import value has a context.
- `literal.rs:375` / `check_ident`: function value is async or a generator.
- `literal.rs:394` / `check_ident`: generic function is read as a value.
- `literal.rs:405` / `check_ident`: class is read as a value.
- `literal.rs:413` / `check_ident`: enum is read as a value.
- `literal.rs:437` / `check_ident`: foreign function is read as a value.
- `literal.rs:531` / `check_ident`: ambient function is read as a value.
- `member.rs:35` / `check_member_read_inner`: descriptor numeric operand reads an optional this field.
- `member.rs:135` / `check_member_read_inner`: read member has a private name.
- `member.rs:209` / `check_index`: literal FixedArray index exceeds its length.
- `member.rs:221` / `check_index`: receiver type has no index operation.
- `member.rs:305` / `member_on`: instance accessor has no read signature.
- `member.rs:339` / `member_on`: write names no class property.
- `member.rs:348` / `member_on`: ordinary instance method is read as a value.
- `member.rs:360` / `member_on`: generic instance method is read as a value.
- `member.rs:372` / `member_on`: class instance has no member signature.
- `member.rs:421` / `member_on`: unknown FixedArray member.
- `member.rs:453` / `member_on`: unknown Map member.
- `member.rs:481` / `member_on`: unknown Set member.
- `member.rs:556` / `member_on`: write to a coroutine step result.
- `member.rs:581` / `member_on`: unknown coroutine step-result member.
- `member.rs:634` / `member_on`: unknown numeric member.
- `member.rs:643` / `member_on`: object receiver has no narrowed class type.
- `member.rs:652` / `member_on`: other receiver has no member signature.
- `method.rs:36` / `check_number_method`: unknown numeric method.
- `method.rs:242` / `str_surface_error`: string member has no accepted or rejection-table entry.
- `method.rs:681` / `check_array_method`: callback-family argument count is not one.
- `method.rs:825` / `check_map_group_by`: Map.groupBy items have no Array type.
- `method.rs:843` / `check_map_group_by`: Map.groupBy callback returns Void.
- `method.rs:1070` / `check_map_method`: unknown Map method.
- `method.rs:1153` / `check_map_method`: Map.forEach argument count is not one.
- `method.rs:1190` / `check_set_method`: unknown Set method.
- `method.rs:1228` / `check_set_method`: Set.forEach argument count is not one.
- `method.rs:1570` / `arr_surface_error`: array member has no accepted or rejection-table entry.
- `namespace.rs:186` / `check_namespace_member`: class namespace accessor has no read signature.
- `namespace.rs:205` / `check_namespace_member`: class static method is read as a value.
- `namespace.rs:212` / `check_namespace_member`: unknown class static member.
- `namespace.rs:228` / `check_namespace_member`: unknown generic-class static member.
- `namespace.rs:249` / `check_namespace_member`: unknown enum member.
- `namespace.rs:705` / `check_worker_spawn`: Worker.spawn identifier denotes no ordinary function.
- `namespace.rs:729` / `check_worker_spawn`: worker entry result, arity, defaults, or generator flag fails.
- `namespace.rs:742` / `check_worker_spawn`: worker parameters are not Inbox and Outbox.
- `namespace.rs:884` / `check_regex_method`: unknown RegExp method.
- `operator.rs:53` / `check_unary`: unary minus operand is not numeric.
- `operator.rs:82` / `check_unary`: logical-not operand is not boolean.
- `operator.rs:106` / `check_unary`: bitwise-not operand is not an integer.
- `operator.rs:134` / `check_unary`: delete expression.
- `operator.rs:142` / `check_unary`: unsupported unary operator.
- `operator.rs:456` / `check_bin`: logical operand is not boolean.
- `operator.rs:495` / `check_bin`: binary operator is in or exponentiation.
- `operator.rs:642` / `check_optional_plan`: member-step optional call has tested=true.
- `operator.rs:679` / `check_optional_plan`: standalone optional call has tested=true.
- `operator.rs:1236` / `bin_result`: compound operation is invalid and not mixed numeric.
- `operator.rs:1296` / `bin_result`: binary operation has no defined operand pair.
- `operator.rs:1382` / `check_cond`: conditional-expression test is not boolean.
- `operator.rs:1488` / `check_yield`: delegated yield.
- `operator.rs:1576` / `check_as`: cast pair is outside the accepted conversion kinds.
- `namespace.rs:22` / `check_receiver`: receiver retains a Nullable type at member use.
- `call.rs:550` / `check_indirect_call`: indirect callee has no Func type.
