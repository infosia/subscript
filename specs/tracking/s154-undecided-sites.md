# §154 undecided sites: classification

Scope: the 125 sites in the three "Undecided sites: acceptance 3" tables of
`specs/tracking/s154-rejection-sites.md` (round 2: 33, round 3: 35, round 4: 57).

Method: each guard was read at its source line. Each accepted witness ran through
`target/debug/subscript check` (built at 2026-10-03 18:52 JST from the working tree,
`cargo build --offline --locked -p subscript-cli`). Probes ran under `tsc` from
`node_modules/.bin/tsc` with the repository `tsconfig.json` options and
`prelude/lang.d.ts`, and under `node` v24.18.0 where stated. Probe files are in
`$TMPDIR` only. No repository file changed.

Kinds:

- RESTRICTION: the language does not have the form on purpose. A reason and a place are proposed.
- DEFECT: the checker rejects because of a bug or a missing fact. Evidence is given.
- FORM: one site rejects forms that need different reasons. The site lacks the fact that separates them.
- ALREADY-DECIDED: a section sentence decides the restriction. The sentence is quoted.

## 1. Summary

| Kind | Round 2 | Round 3 | Round 4 | Total |
|---|---:|---:|---:|---:|
| RESTRICTION | 5 | 24 | 39 | 68 |
| DEFECT | 22 | 3 | 2 | 27 |
| FORM | 4 | 4 | 9 | 17 |
| ALREADY-DECIDED | 2 | 4 | 7 | 13 |
| Total | 33 | 35 | 57 | 125 |

Main findings:

1. 16 of the 27 DEFECT sites are cascades. A rejected declaration does not bind or poison
   its name in a function body or in a mirror, so a later use reaches a second site. The
   accepted witness of the second site always has an earlier diagnostic. Module-level
   rejected declarations are already poisoned (`check/declarations.rs` lines 89–99); local
   declarations and mirror `declare module` are not.
2. Four type-name guards (`Promise`, `FixedArray`, `Generator`, and the `Array` guard)
   ignore a source declaration of the same name. tsc resolves the name to the source
   declaration. Measured: `class FixedArray<T, N> { tag: i32 = 7; }` with
   `x: FixedArray<i32, 3>` and `x.length` is accepted by this checker and rejected by
   tsc (TS2339). The same holds for `class Generator<T>` with `x.next()`. This contradicts
   C14 ("this compiler rejects. It never accepts a program and gives it a different
   value").
3. The checker never reads a type-parameter default (`TsTypeParam.default` is read only in
   `check/namespace_import.rs`). The declaration `function f<T, U = i32>` is accepted, and
   every use that omits `U` is rejected.
4. Message misfits (§103):
   - `CheckDeclarations518` says "string-literal union aliases cannot be generic" for every
     generic alias, for example `type Promise<T = void> = globalThis.Promise<T>`.
   - `CheckExprLiteral394` says "requires explicit type arguments". The only spelling,
     the instantiation expression `id<i32>`, is rejected by `CheckExprEntry313`
     ("expression form outside the decided surface"). Measured; tsc accepts it.
5. Existing `Divergence` variants already state the reason of several RESTRICTION
   sites, but their `collision` section holds no sentence for it:
   - `CompilerOwnedValue` (cites stdlib.md §9.0; used by `DateMethodValue`) fits the
     builtin method values and `print` as a value.
   - `ClassRuntimeObject` (cites compiler.md §71.1) fits `new (expr)()` and constructor
     types.

   §154 acceptance 2 permits a shared variant only when one `ts` fragment shows both.

## 2. Topics (RESTRICTION rows)

CANDIDATE-TO-ACCEPT marks a topic, or the named sites in it, where accepting the form
has a clear lowering. The mark is evidence, not a decision.

| # | Topic | Proposed rule text | Place | Sites | Accept? |
|---|---|---|---|---|---|
| T1 | Truth tests require `boolean` | A truth test takes a `boolean`; the language has no implicit conversion (C3), so a program writes `n !== 0` or `x !== null`. | C3 (extend), or NEW | CheckStmt535, CheckExprOperator82, CheckExprOperator456, CheckExprOperator1382 | CANDIDATE: ECMA ToBoolean has a total lowering per kind (integer ≠ 0; float ≠ 0 and not NaN; string length ≠ 0; reference ≠ null). §143.3 item 3 already names the gap. For `&&` and `\|\|` the result takes the operand type. |
| T2 | Declared types at signature positions | A module variable, a field, a parameter, and a function or block-lambda result state their type; the checker infers only locals and expression lambdas. | NEW (compiler.md) | CheckSignatures373, CheckSignatures478, CheckSignatures519, CheckSignatures548, CheckClassShape520, CheckClassShape612, CheckExprLambda193 | CANDIDATE for 373, 519, 548, 520, and 612 when the initializer or default is a literal: C4 already gives the context-free type (`i32`, `f64`, `string`) that `const x = 1` uses. CANDIDATE for 193: a block body's return types join as §146 joins branches. Not for 478: recursion needs the return type before the body. |
| T3 | Every declaration starts with a value | A `let` without an initializer holds `undefined` until its first assignment; the language has no `undefined` (C7), so every declaration starts with a value. | C7; §108.1 states the same reason for static fields (`StaticFieldInitializerMissing`) | CheckBodies158, CheckStmt359 | CANDIDATE for CheckStmt359 only: tsc does definite-assignment analysis for locals, and §108 already does it for fields. Not for module scope. |
| T4 | `var` | `var` binds its name for the whole function with the value `undefined`; the language has block-scoped `let` and `const` only, and no `undefined` (C7, C14). | C14 | CheckBodies123, CheckStmt311, CheckStmt994 | No: an early read diverges, and C14 rejects where resolution diverges. |
| T5 | Instance method values | A method read as a value loses its receiver, and the language has no bound-function value. | NEW (compiler.md, beside §37.1 `AsyncMethodValue`) | SynchronousMethodValue, GenericSynchronousMethodValue, CheckExprMember397, CheckExprMember408, CheckExprMember445, CheckExprMember473, CheckExprMember501, CheckExprMember550 | No. A bound value is a capturing value, and C5 forbids its escape. For the builtin rows, `CompilerOwnedValue` states a second true reason. |
| T6 | Named functions without a receiver as values | A generator, a static method, an ambient intrinsic, and a foreign function are direct call targets with no first-class function value. | NEW (compiler.md; §26.1 has the async sentence) | GeneratorFunctionValue, CheckExprNamespace205, CheckExprLiteral531, CheckExprLiteral437 | CANDIDATE for CheckExprNamespace205: a static method has no `this` (`StaticMemberSurface`, §71), so it is a plain C function. CANDIDATE for GeneratorFunctionValue: a generator is a function that returns `Generator<T>`. CheckExprLiteral437 needs a wrapper, because a foreign call converts strings and arrays at the call site. CheckExprLiteral531: `CompilerOwnedValue` reason fits. The topic sentence is a decision; only 531 has a reason that fits. |
| T7 | Class and enum have no run-time object | A class and an enum have no run-time object: a class lowers to a C layout, and an enum lowers to integer constants. | compiler.md §71.1 (`ClassRuntimeObject` cites it; add the sentence) and NEW for enums | CheckExprCall1300, CheckTyres673, CheckExprLiteral413, CheckExprNamespace249 | No. |
| T8 | Annotation forms with one reason | An annotation names a declared or builtin type; the only qualifier is a §148 namespace import, and no structural type exists to intersect (C1). | C1 and C18 (§148) | CheckTyres193, CheckTyres581 | No. |
| T9 | Generator surface | A generator gives values of one declared yield type through `next()` (C8); its finished result carries a zero, so it has no return value. | C8 | CheckExprCall1067, CheckTyres403, CheckExprOperator1488, CheckClassShape354, CheckExprMember556 | The rule fits 1067 and 403 only. CANDIDATE for 1488: `yield* xs` is `for (const x of xs) yield x;` over every stdlib §14.1 source (a generator has no return value to forward). CANDIDATE for 354: static generator methods and async instance methods (§37) are accepted; an instance generator method is a frame with a receiver. CANDIDATE for 556: a step result is a value-struct copy, and C2 accepts field writes to a copy. No reason found for 1488, 354, or 556. |
| T10 | ECMAScript private names | TypeScript `private` is the language's member privacy; a `#name` adds a run-time brand that a nominal class (C1) does not need. | C1 | CheckExprMember135, CheckExprAssign598 | CANDIDATE: a `#x` field is an ordinary field with a reserved C name; C1 already rejects foreign receivers. Measured: `private x: i32` is accepted. |
| T11 | Indexing | An index is `i32` (C3, no implicit conversion); a constant index outside a `FixedArray` length always traps (Q3); a string has no index, because `s[i]` gives `undefined` out of range (C7). | C3, Q3, C7/Q21 | CheckExprMember190, CheckExprMember201, CheckExprMember209, CheckExprMember221 | CANDIDATE for 221 only: `s[i]` lowers to the byte-offset `at` with a trap, as §8.10 does for `at`. |
| T12 | Descriptor literals | A descriptor literal fills data members by identifier (Q33); an accessor or a method has no data member to fill. | collisions.md Q33, compiler.md §25 | CheckExprAggregate155, CheckExprAggregate165, CheckExprAggregate184 | The rule fits 184. CANDIDATE for 165: a quoted key that spells an identifier names the same member. CANDIDATE for 155: a spread of a value of the same descriptor class copies its fields. |
| T13 | Type alias forms | A source type alias declares a closed string-literal set (Q32) or a wire-mapped alias (§50); a transparent alias and a generic alias are not in the language. | Q32 / compiler.md §24 | CheckDeclarations518, CheckDeclarations530, CheckDeclarations550 | CANDIDATE for 530: mirror aliases are already transparent (§12.2; `ScopeItem::TypeAlias` in `check/tyres.rs`). CANDIDATE for 550: tsc collapses a repeated member, and the alias keeps one discriminant. 518: the message names literal unions for every generic alias (misfit). The rule text is a decision; no reason found. |
| T14 | Declaration sugar | A string-literal enum member name and a constructor parameter property are TypeScript spellings with no decided lowering. | NEW | CheckDeclarations461, CheckClassShape732 | CANDIDATE: a parameter property is a field plus a constructor assignment; a string member name that spells an identifier is that identifier. No reason found. |
| T15 | Mirror forms | A mirror declares the C items of a header (§12.2, §128.1); C has no namespace and no module scope. | compiler.md §12.2 / §128.1 (`MirrorExportList` states the same reason) | CheckDeclarations749 | No. |
| T16 | Module resolution | An import names a module among the program's files; `node` cannot load a module that no file supplies. | C18 | CheckSignatures213, CheckSignatures263 | No. Measured: `import "./missing.mjs"` under node v24.18.0 throws `ERR_MODULE_NOT_FOUND` before any output; tsc accepts the side-effect import. |
| T17 | Statement forms with one reason each | `for await` reads async iteration (§26.1); a `for…of` head declares its binding; a `switch` dispatches on integer, enum, string, or alias constants (§41). | §26.1, stdlib §14, §41 | CheckStmt788, CheckStmt1019, CheckStmt1298 | CANDIDATE for 1019: assignment to an existing binding per element. CANDIDATE for 788 over a handle array: `for (const h of hs) { const x = await h; }`; a non-handle element stays `AwaitNonHandle` (§26.1). CANDIDATE for 1298 with constant `true`/`false` cases. |
| T18 | Assertions and assignment operators | An `as` or `!` assertion yields a checked value, not a place (C3, C7); `&&=`, `\|\|=`, and `**=` have no HIR form. | C3, C7 | CheckExprAssign500, CheckExprEntry296, CheckExprAssign41 | CANDIDATE for 296: `x!` lowers to the checked narrowing that traps on `null`, as `as C` does (C3). CANDIDATE for 41: `a ||= b` rewrites as §82.1 rewrites `op=`. |
| T19 | Type compatibility of instances and function types | Instances and function types match only with identical type arguments and parameter types: C3 has no implicit conversion, C1 no structural substitution, and there is no variance. | C1 and C3; closes collisions.md §3 open item "variance" | CheckTypeRules299, CheckExprMethod1544 | CANDIDATE for nullable-parameter contravariance in 1544: `(x: C \| null) => R` and `(x: C) => R` have one pointer ABI. Measured: both forms are rejected; tsc accepts both. |
| T20 | Lexical `this` and function expressions | A lambda captures `const` locals only (C5), and a `function` expression binds its own `this`, which the language does not have. | C5 | CheckExprEntry243, CheckExprEntry304 | CANDIDATE for 243: `this` is immutable in a method, so it is captured by value as a `const` local is. CANDIDATE for 304 when the body does not read `this` and is not a generator: it is an arrow. |
| T21 | Template interpolation domain | Interpolation formats scalars, strings, enums, and literal aliases (Q14); other kinds have no implicit string form. | Q14 | CheckExprLiteral270 | CANDIDATE for arrays (`join` exists, `ArrayJoinDomain`), and for a class that declares `toString(): string`. |
| T22 | Empty array literal without context | An empty literal without context gives no element type, and the language has no `any[]` or evolving array type (C4, S001). | C4 | CheckExprAggregate102 | No: an evolving array needs flow analysis of later writes. |

## 3. DEFECT, FORM, and ALREADY-DECIDED lists

### 3.1 DEFECT (27)

**D1. Cascade after a rejected declaration (16).** The accepted witness has an earlier
diagnostic at another site. The second site fires because the rejected declaration
neither binds nor poisons its name. If the name is poisoned, the site is reached only
by tsc-rejected programs.

| Site | First diagnostic of the accepted witness | Missing fact |
|---|---|---|
| CheckLookup19 | CheckStmt210 (local `class C {}`) | the local shadow of an `import type` name |
| CheckText49 | CheckStmt210 (local `function encodeURI<T>`) | the local generic function |
| CheckTyres214, CheckTyres224 | CheckStmt210 (local `type Worker…`) | the local alias |
| CheckTyres267 | CheckStmt210 (local `type RegExp<T>`) | the local alias |
| CheckTyres419 | CheckStmt210 (local `type Error<T>`) | the local alias |
| CheckTyres430, CheckTyres439 | CheckStmt210 (local `type Map…`) | the local alias |
| CheckTyres495 | CheckStmt210 (local `type Date<T>`) | the local alias |
| CheckTyres512 | CheckStmt210 (local `type C<T>` over module `class C`) | the local alias |
| CheckTyres551 | CheckStmt210 (local `type Choice<T>`) | the local alias |
| CheckBindings97 | CheckDeclarations518 (`type Generator<T> = …`) | the alias, and the `Generator` guard ignores scope (D2) |
| CheckExports101 | CheckDeclarations749 (`declare module "s154_external_101"` in a mirror) | the ambient module; `poison_missing_modules` exists (§63) but is not fed by a rejected ambient module |
| CheckExports370, CheckSignatures317 | "export lists are outside the mirror surface" and CheckDeclarations749 (`declare module "./other"` augmentation) | the augmented export |
| DescriptorRequiredMemberQuotedKey | CheckExprAggregate165 (quoted key) | the rejected key's member; the site exists only for this follow-on (`aggregate.rs` line 248 selects it when a quoted key names the field) |

Top-level probes show the shadowing works when the declaration is accepted:
`class Worker {}`, `class Map<K> {}`, `class RegExp<T> {}`, `class Error<T> {}`,
`class Date<T> {}`, and top-level `function encodeURI<T>` each check clean. The same
class also appears in two non-listed forms: a rejected local `var x` makes a later
`x` "unknown name" (measured), which is the listed second witness of CheckExprLiteral554.

**D2. A builtin type name ignores a source declaration (4).** The `Promise`,
`FixedArray`, and `Generator` arms of `resolve_type_ref` (`check/tyres.rs` lines
282–420) do not test `type_scope_item(name)`. The `Worker`, `RegExp`, `Map`, `Set`,
`Date`, and Error-family arms do. Measured, each tsc-clean:

- CheckTyres283: `class Promise { x: i32 = 1; } function f(x: Promise): void {}` gives "`Promise` requires exactly one fulfilled-value type argument".
- CheckTyres291: `class Promise<T, U> {…} function f(x: Promise<i32, i32>)` gives the same message.
- CheckTyres303: `class FixedArray {…} function f(x: FixedArray)` gives "requires element type and length arguments".
- CheckTyres311: `class FixedArray<T> {…} function f(x: FixedArray<i32>)` gives "takes exactly two type arguments".

Silent divergence, measured: `class FixedArray<T, N> { tag: i32 = 7; }` with
`x: FixedArray<i32, 3>` and `return x.length` is accepted here and rejected by tsc
(TS2339); `class Generator<T>` with `x.next()` is the same. C14 requires a rejection.
CheckTyres403 (bare `Generator`) has the same defect, but it also has a tsc-accepted
witness without a shadow, so it is classified RESTRICTION. CheckTyres390 (`Array`) is
decided by stdlib.md §9.0 (§3.3).

**D3. A type-parameter default is ignored (4).** CheckGenerics283, CheckGenerics375,
CheckGenerics492, CheckTyres518. The declaration `<T, U = i32>` is accepted; the
instantiation compares `type_params.len()` with the argument count
(`check/generics.rs` lines 281, 373, 490). No checker code reads `param.default`.
Measured: `function f<T, U = i32>(x: T): void {}` with `f(1)` gives "cannot infer type
parameter `U`"; `class C<T = i32>` with `new C()` gives "requires explicit type
arguments"; tsc accepts both. The alternative to the fix is a restriction at the
declaration: reject `= T` on a type parameter.

**D4. A proof from the initializer is lost (2).** NullableCall, NullableMember. A local
`const c: C | null = new C()` is not narrowed by its non-null initializer. tsc narrows on
assignment. Measured: `const cb: ((x: i32) => i32) | null = g; cb(1)` and
`let c: C | null = new C(); c.x` are tsc-clean and rejected here. The NullableCall
witness `if (cb) { cb(1); }` is also a cascade of CheckStmt535: the rejected truth test
gives no narrowing. C7 states "narrowing is required before member access (`tsc`
already enforces this under `strictNullChecks`)"; the parenthesis is false for this
form.

**D5. Order-dependent inference (1).** CheckExprCall220. An unannotated generator's
yield type comes from its body. A call that the checker checks first sees no type, and
the message asks the program to reorder declarations. tsc has no order dependence.
No section decides that declaration order changes acceptance.

### 3.2 FORM (17)

Each site is one branch that rejects several forms. The forms need different reasons
(§103), and some have a reason already decided. The fact the site lacks is the form
kind named in the "split by" column.

| Site | Split by | Members, with a reason where one exists |
|---|---|---|
| CheckTyres97 | `TsType` kind | tuple (stdlib §14 `NoTupleType`); type literal `{…}` (C1); single literal type `"a"` (Q32: an inline literal set stays rejected); numeric literal type `3` (none); `keyof`, `typeof`, indexed, mapped, conditional, `infer` (none); type predicate `x is C` (none); `this` type (none); `readonly T[]` (none) |
| CheckTyres180 | keyword | `symbol`, `bigint` (stdlib §7 non-goals); `unknown` (`AnyType` reason fits: no C layout); `never` (none; the prelude itself declares `unreachable(): never`) |
| CheckTyres698 | `TsFnParam` kind | rest parameter (stdlib §14.4: no variadic parameters); array or object pattern in a function type (none; the name has no effect on the type) |
| CheckTypeRules329 | type pair | literal alias to `string` (Q32 "Comparison or assignment with plain `string` … is rejected"); enum to integer (C3 `as`); `i32[]` to `FixedArray` (Q3 permits only a literal); function types with other sized parameters (T19). The listed witness is a D1 cascade. |
| CheckStmt210 | declaration kind | local class, function, enum, type alias, interface, namespace. A local non-capturing function is an arrow (CANDIDATE); a local alias is a T13 form. |
| CheckStmt295 | statement kind | `do…while` (§124 acceptance 10 keeps it out, no reason); `for…in` (C10: no dynamic properties); labeled statement (none); `debugger` (none); `with` (tsc rejects in strict mode) |
| CheckClassShape841 | class member kind | `#x` and `#m()` (T10); `static {}` block (none); `accessor x` (none) |
| CheckDeclarations131 | declaration kind | `interface` (C1: structural type); `namespace` (C18 module surface, `UnsupportedModuleDeclaration`) |
| CheckExprOperator142 | unary operator | `typeof` (none; static types make it a constant, CANDIDATE); `void` (C7: yields `undefined`); unary `+` (Q25 coercion; identity on a sized numeric) |
| CheckExprOperator495 | binary operator | `in` (C10: no dynamic properties); `**` (none; `Math.pow` exists, CANDIDATE) |
| CheckExprOperator1236 | operand types | enum `+=` (C3 `as`); `string += i32` (implicit ToString; Q14 template is the spelling) |
| CheckExprOperator1296 | operand types | enum arithmetic and bitwise (C3 `as`); `string + i32/boolean/enum` (implicit ToString); `string < string` (Q5 permitted surface lists `===`/`!==` only); `boolean < boolean` (none) |
| CheckExprOperator1576 | source and target | identity `c as C` (none, CANDIDATE); integer to enum (no membership check); `C \| null as C` (unchecked; CANDIDATE as a trap, like T18); `string as Alias` (no membership check) |
| CheckExprEntry313 | `Expr` kind | `<T>x` and `x satisfies T` (CANDIDATE: `as` and a check); instantiation `id<i32>` (CANDIDATE; also the remedy of CheckExprLiteral394); comma operator; tagged template; class expression; `import.meta`, `new.target`; `#x in o` |
| CheckExprMember652 | receiver type | `boolean` members (stdlib §0 rule 1); function value `.length`, `.name` (C5: a C function pointer has no properties); generator `.next` as a value (T5); enum value members; literal alias `.length` (Q32 operations list) |
| CheckExprCall1130 | receiver type | same receivers as 652, as method calls (`b.toString()`, `e.toString()`, `l.toUpperCase()`, `f.call()`) |
| CheckExprCall1157 | callee origin | `check_argument_count` serves 52 callers. A source function with extra arguments is tsc-rejected (TS2554). A builtin with a variadic lib signature is tsc-accepted and decided per member (Q27 for `push`/`unshift`, Q19 for `Math.max`). The builder has no origin. |

### 3.3 ALREADY-DECIDED (13)

| Site | Section | Sentence |
|---|---|---|
| CheckTyres390 | stdlib.md §9.0 | "`Array<T>` in type position stays a builtin that no declaration shadows." Note: with `class Array<T>`, `new Array<i32>()` builds the source class while the annotation `Array<i32>` is the builtin, and the program is rejected with "type mismatch: the argument expects `i32[]`, got `Array<i32>`" (measured). |
| CheckTyres562 | stdlib.md §0 rule 1 | "This compiler accepts a **deterministic subset** of the lib API … and rejects out-of-subset members with a clear S-code". The sentence says "members"; lib type names (`ReadonlyArray`, `Iterable`) need "and names". |
| CheckBodies261, CheckExprLambda211 | compiler.md §42.1 | "Enum, integer, and string switches are unchanged — they have `default`-free coverage no analysis can prove." §103 check: for a source enum, `as` does not convert an integer to an enum (CheckExprOperator1576), so the reason holds only through paths such as byte reads and mirror enums. Re-measure before reuse. |
| CheckStmt475 | collisions.md C8 | "`.next()` returns the language-level value-struct shape `{ done: boolean; value: T }`, with `value` zero-initialized when `done`". A return value has no slot. |
| CheckStmt1101 | stdlib.md §14.4 | "**`f(...xs)` is rejected** (S014): it needs variadic parameters, which the language does not have". The only tsc-accepted witness is `values(...([] as []))`. |
| CheckClassShape475 | collisions.md Q28 | "field names are identifiers, the checker rejecting computed and non-identifier ones." The sentence states the rule as a premise of JSON field order. A quoted key that spells an identifier is a CANDIDATE. |
| UnknownNamespaceConstructor | stdlib.md §0 rule 1 | Same sentence as CheckTyres562. The only tsc-accepted program is `new Object()` (measured: `new Number`, `new Promise`, `new Array`, `new Date()` reach other sites; `new Math()`, `new JSON()` are TS2351). |
| CheckExprCall174 | stdlib.md §0 rule 1; §8 | §8: "`String.fromCharCode`/`raw` and `String` as a value or constructor are rejected through the standing unknown-name paths". Other reachers: `Boolean(1)` (Q25 coercion), `Symbol()`, `BigInt()` (stdlib §7 non-goals). |
| CheckExprLiteral554 | stdlib.md §0 rule 1; §8 | Same as CheckExprCall174, for `String`, `globalThis` as values (measured). Its listed accepted witness is a D1 cascade (a rejected local `var`). |
| CheckExprLiteral130 | stdlib.md §7 | "**Stdlib non-goals** …: … `BigInt` (`i64`/`u64` exist)." The only literal kind that reaches the catch-all is a bigint literal. |
| CheckExprLiteral394 | compiler.md §149.1 rule 7 | "Out of scope, still rejected as before: inference for a generic method, a generic constructor (`new G(x)`), a callback's parameter types from context, and the return type from context." The message remedy `id<i32>` is rejected (§1 item 4). |
| CheckExprCall1385 | compiler.md §149.1 rule 7 | Same sentence ("a generic constructor (`new G(x)`)", "the return type from context"). The variant `GenericConstructorTypeArguments` already cites §149.1. |

## 4. Per-site table

| Site | Kind | Topic | One line |
|---|---|---|---|
| NullableCall | DEFECT | D4 | Initializer and truth-test narrowing lost; `const cb: F \| null = g; cb(1)` is tsc-clean. |
| NullableMember | DEFECT | D4 | `const c: C \| null = new C(); c.x` is not narrowed by its initializer. |
| CheckBindings97 | DEFECT | D1 | Reached only after a rejected `type Generator` alias; the `Generator` guard ignores scope. |
| CheckLookup19 | DEFECT | D1 | A rejected local class does not shadow the `import type` name. |
| CheckGenerics283 | DEFECT | D3 | The declared default `U = i32` is ignored in a function call. |
| CheckGenerics375 | DEFECT | D3 | The declared default is ignored in a method call. |
| CheckGenerics492 | DEFECT | D3 | The declared default is ignored in a class instance. |
| CheckText49 | DEFECT | D1 | A rejected local `function encodeURI<T>` does not shadow the builtin. |
| CheckTyres97 | FORM | §3.2 | Annotation catch-all: tuple, type literal, literal, `keyof`, mapped, predicate, and other kinds. |
| CheckTyres180 | FORM | §3.2 | Keyword catch-all: `never`, `unknown`, `symbol`, `bigint`. |
| CheckTyres193 | RESTRICTION | T8 | `globalThis.String`: the only type qualifier is a §148 namespace import. |
| CheckTyres214 | DEFECT | D1 | Reached after a rejected local `type Worker`; bare `Worker` is TS2314. |
| CheckTyres224 | DEFECT | D1 | Same, with a different parameter count. |
| CheckTyres267 | DEFECT | D1 | Reached after a rejected local `type RegExp<T>`. |
| CheckTyres283 | DEFECT | D2 | `Promise` arm ignores a source `class Promise`. |
| CheckTyres291 | DEFECT | D2 | Same, with two type arguments. |
| CheckTyres303 | DEFECT | D2 | `FixedArray` arm ignores a source class. |
| CheckTyres311 | DEFECT | D2 | Same, with one type argument; a two-argument shadow is silently accepted. |
| CheckTyres390 | ALREADY-DECIDED | stdlib §9.0 | `Array<T>` in a type is a builtin that no declaration shadows. |
| CheckTyres403 | RESTRICTION | T9 | Bare `Generator` has yield type `unknown`, which the language does not have; it also has the D2 defect. |
| CheckTyres419 | DEFECT | D1 | Reached after a rejected local `type Error<T>`. |
| CheckTyres430 | DEFECT | D1 | Reached after a rejected local `type Map`. |
| CheckTyres439 | DEFECT | D1 | Same, with one type argument. |
| CheckTyres495 | DEFECT | D1 | Reached after a rejected local `type Date<T>`. |
| CheckTyres512 | DEFECT | D1 | Reached after a rejected local alias that shadows a module class. |
| CheckTyres518 | DEFECT | D3 | `class C<T = i32>` used as `C`: the default is ignored. |
| CheckTyres551 | DEFECT | D1 | Reached after a rejected local alias that shadows a literal alias. |
| CheckTyres562 | ALREADY-DECIDED | stdlib §0 rule 1 | A lib type name outside the subset (`ReadonlyArray`). |
| CheckTyres581 | RESTRICTION | T8 | An intersection combines structural members that no nominal class holds. |
| CheckTyres673 | RESTRICTION | T7 | A constructor type needs a class value; a class has no run-time object. |
| CheckTyres698 | FORM | §3.2 | Function-type parameter catch-all: rest (decided) and patterns (none). |
| CheckTypeRules299 | RESTRICTION | T19 | `Map<i32, i32>` is not `Map<i64, i64>`: distinct instances, no implicit conversion. |
| CheckTypeRules329 | FORM | §3.2 | Type-mismatch catch-all over alias, enum, FixedArray, and function pairs. |
| CheckBodies123 | RESTRICTION | T4 | Module-level `var`. |
| CheckBodies158 | RESTRICTION | T3 | Module `let x: i32;` holds `undefined` until assigned. |
| CheckBodies261 | ALREADY-DECIDED | §42.1 | Enum switch coverage does not complete return flow. |
| CheckStmt210 | FORM | §3.2 | Nested declaration catch-all over six declaration kinds. |
| CheckStmt295 | FORM | §3.2 | Statement catch-all: `do…while`, `for…in`, labeled, `debugger`. |
| CheckStmt311 | RESTRICTION | T4 | Local `var`. |
| CheckStmt359 | RESTRICTION | T3 | Local `let x: i32;`; candidate through definite assignment. |
| CheckStmt475 | ALREADY-DECIDED | C8 | The done result holds the zero, so `return v` in a generator has no slot. |
| CheckStmt535 | RESTRICTION | T1 | `if (n)` with `n: i32`. |
| CheckStmt788 | RESTRICTION | T17 | `for await`; candidate over a handle array. |
| CheckStmt994 | RESTRICTION | T4 | `var` in a `for…of` head. |
| CheckStmt1019 | RESTRICTION | T17 | `for (x of xs)` with an existing binding; candidate. |
| CheckStmt1101 | ALREADY-DECIDED | stdlib §14.4 | The only accepted witness is a spread argument. |
| CheckStmt1298 | RESTRICTION | T17 | `switch (true)`; dispatch takes integer, enum, string, or alias constants. |
| CheckClassShape354 | RESTRICTION | T9 | Instance generator method; candidate (static ones and async instance methods exist). |
| CheckClassShape475 | ALREADY-DECIDED | Q28 | Computed and non-identifier field names. |
| CheckClassShape520 | RESTRICTION | T2 | `static x = 1` without an annotation. |
| CheckClassShape612 | RESTRICTION | T2 | `x = 1` without an annotation. |
| CheckClassShape732 | RESTRICTION | T14 | Constructor parameter property; candidate. |
| CheckClassShape841 | FORM | §3.2 | Class member catch-all: `#x`, `static {}`, `accessor`. |
| CheckExports101 | DEFECT | D1 | Reached after a rejected mirror `declare module`; otherwise TS2307. |
| CheckExports370 | DEFECT | D1 | Reached after a rejected module augmentation; otherwise TS2305. |
| CheckDeclarations131 | FORM | §3.2 | Declaration catch-all: `interface` and `namespace`. |
| CheckDeclarations461 | RESTRICTION | T14 | `enum E { "a" = 1 }`; candidate. |
| CheckDeclarations518 | RESTRICTION | T13 | Generic alias; the message names literal unions for every generic alias. |
| CheckDeclarations530 | RESTRICTION | T13 | `type A = i32`; candidate (mirror aliases are transparent). |
| CheckDeclarations550 | RESTRICTION | T13 | `"a" \| "a"`; candidate (collapse the repeat). |
| CheckDeclarations749 | RESTRICTION | T15 | Mirror `declare namespace` or `declare module`; C has neither. |
| CheckSignatures213 | RESTRICTION | T16 | Side-effect import of an absent module in discovery mode. |
| CheckSignatures263 | RESTRICTION | T16 | `import "./missing"` is tsc-clean; node throws `ERR_MODULE_NOT_FOUND`. |
| CheckSignatures317 | DEFECT | D1 | Reached after a rejected module augmentation; otherwise TS2305. |
| CheckSignatures373 | RESTRICTION | T2 | `const x = 1` at module level. |
| CheckSignatures478 | RESTRICTION | T2 | `function f() {}` without a return annotation. |
| CheckSignatures519 | RESTRICTION | T2 | `f(x = 1)` without an annotation. |
| CheckSignatures548 | RESTRICTION | T2 | `f([x] = [1])` without an annotation. |
| SynchronousMethodValue | RESTRICTION | T5 | `new C().f` read as a value. |
| GenericSynchronousMethodValue | RESTRICTION | T5 | Generic method read as a value. |
| GeneratorFunctionValue | RESTRICTION | T6 | `const f = gen`; candidate. |
| UnknownNamespaceConstructor | ALREADY-DECIDED | stdlib §0 rule 1 | `new Object()`. |
| CheckExprOperator82 | RESTRICTION | T1 | `!n` with `n: i32`. |
| CheckExprOperator142 | FORM | §3.2 | Unary catch-all: `typeof`, `void`, unary `+`. |
| CheckExprOperator456 | RESTRICTION | T1 | `1 && 2`. |
| CheckExprOperator495 | FORM | §3.2 | Binary catch-all: `in`, `**`. |
| CheckExprOperator1236 | FORM | §3.2 | Compound catch-all: enum `+=`, `string += i32`. |
| CheckExprOperator1296 | FORM | §3.2 | Binary operand catch-all: enum arithmetic, string concatenation with non-strings, relational on strings and booleans. |
| CheckExprOperator1382 | RESTRICTION | T1 | `n ? 1 : 2`. |
| CheckExprOperator1488 | RESTRICTION | T9 | `yield*`; candidate (for-of plus yield). |
| CheckExprOperator1576 | FORM | §3.2 | `as` catch-all: identity, integer to enum, nullable to class, string to alias. |
| CheckExprMember135 | RESTRICTION | T10 | `this.#x` read; candidate. |
| CheckExprMember190 | RESTRICTION | T11 | `a[i]` with `i: i64` on `T[]`. |
| CheckExprMember201 | RESTRICTION | T11 | `a[i]` with `i: i64` on `FixedArray`. |
| CheckExprMember209 | RESTRICTION | T11 | Constant index outside the `FixedArray` length always traps. |
| CheckExprMember221 | RESTRICTION | T11 | `s[0]` on a string; candidate. |
| CheckExprMember397 | RESTRICTION | T5 | `a.push` read as a value. |
| CheckExprMember408 | RESTRICTION | T5 | `fa.map` read as a value. |
| CheckExprMember445 | RESTRICTION | T5 | `m.get` read as a value. |
| CheckExprMember473 | RESTRICTION | T5 | `s.add` read as a value. |
| CheckExprMember501 | RESTRICTION | T5 | `str.slice` read as a value. |
| CheckExprMember550 | RESTRICTION | T5 | `re.test` read as a value. |
| CheckExprMember556 | RESTRICTION | T9 | Write to `s.done`; candidate (a value copy). |
| CheckExprMember652 | FORM | §3.2 | Member catch-all over boolean, function, generator, enum, and alias receivers. |
| CheckExprLambda193 | RESTRICTION | T2 | Block lambda without a return annotation; candidate. |
| CheckExprLambda211 | ALREADY-DECIDED | §42.1 | Enum switch coverage in a lambda. |
| CheckExprLiteral130 | ALREADY-DECIDED | stdlib §7 | Bigint literal. |
| CheckExprLiteral270 | RESTRICTION | T21 | `${new C()}`; candidate for arrays and declared `toString`. |
| CheckExprLiteral394 | ALREADY-DECIDED | §149.1 rule 7 | `apply(id, 3)`; the message remedy is itself rejected. |
| CheckExprLiteral413 | RESTRICTION | T7 | `const x = E`. |
| CheckExprLiteral437 | RESTRICTION | T6 | Foreign function as a value; needs a wrapper. |
| CheckExprLiteral531 | RESTRICTION | T6 | `const x = print`; `CompilerOwnedValue` reason fits. |
| CheckExprLiteral554 | ALREADY-DECIDED | stdlib §0 rule 1, §8 | Lib globals as values; the listed witness is a D1 cascade. |
| CheckExprAggregate102 | RESTRICTION | T22 | `const a = []`. |
| CheckExprAggregate155 | RESTRICTION | T12 | Spread in a descriptor literal; candidate for a same-class source. |
| CheckExprAggregate165 | RESTRICTION | T12 | Quoted key in a descriptor literal; candidate. |
| CheckExprAggregate184 | RESTRICTION | T12 | Accessor in a descriptor literal. |
| DescriptorRequiredMemberQuotedKey | DEFECT | D1 | Follow-on of CheckExprAggregate165 only. |
| CheckExprCall174 | ALREADY-DECIDED | stdlib §0 rule 1, §8 | `String(1)`, `Boolean(1)`. |
| CheckExprCall220 | DEFECT | D5 | Acceptance depends on declaration order. |
| CheckExprCall1067 | RESTRICTION | T9 | `gen().return(v)`, `throw(e)`. |
| CheckExprCall1130 | FORM | §3.2 | Method-call catch-all over the same receivers as 652. |
| CheckExprCall1157 | FORM | §3.2 | Shared argument-count builder with no callee origin. |
| CheckExprCall1300 | RESTRICTION | T7 | `new (c ? C : D)()` needs a class value. |
| CheckExprCall1385 | ALREADY-DECIDED | §149.1 rule 7 | `new Map()` without type arguments. |
| CheckExprNamespace205 | RESTRICTION | T6 | `C.f` static method value; candidate. |
| CheckExprNamespace249 | RESTRICTION | T7 | `E.toString`; an enum has no object. |
| CheckExprEntry243 | RESTRICTION | T20 | `this` in a lambda in a method; candidate. |
| CheckExprEntry296 | RESTRICTION | T18 | `c!`; candidate as a checked trap. |
| CheckExprEntry304 | RESTRICTION | T20 | `function` expression; candidate without `this`. |
| CheckExprEntry313 | FORM | §3.2 | Expression catch-all: `<T>x`, `satisfies`, `id<i32>`, comma, tagged template, and others. |
| CheckExprAssign41 | RESTRICTION | T18 | `&&=`, `\|\|=`, `**=`; candidate. |
| CheckExprAssign500 | RESTRICTION | T18 | `(x as i32) = 2`: an assertion is not a place. |
| CheckExprAssign598 | RESTRICTION | T10 | `this.#x = 2`; candidate. |
| CheckExprMethod1544 | RESTRICTION | T19 | Callback type must match exactly; candidate for nullable-parameter contravariance. |
