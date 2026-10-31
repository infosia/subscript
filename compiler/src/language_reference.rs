//! Generated AI-facing language reference and corpus index.
//!
//! The prose in this module is curated, while diagnostics, warning excerpts,
//! and corpus metadata are read from their checker and harness sources.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{RuleCode, WarnCode};

const GENERATED_BY: &str = "cargo run --offline -p subscript-compiler --bin generate-api-reference";

const SURFACE_SUMMARY: &str = "subscript is a deliberately closed, TypeScript-shaped language for deterministic embedded programs. The entry module exports the host entry functions. Types are nominal; only Error-family objects are thrown, and dynamic evaluation, `any`, general unions, `undefined` outside a function result, and an implicit scheduler are outside the language. Standard-library acceptance is narrower than the stock ES2022 declarations; consult `api-reference.md` for the checker-owned surface and replacements.";

const SIZED_NUMERICS: &str = "Numeric types are `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f16`, `f32`, and `f64`; bare `number` is rejected. Literals are checked against their contextual sized type. Integer shifts use `count & (width - 1)` for every count spelling, including compound assignments. Literal counts must fit their contextual type. Mixed-width operands require an explicit `as`. `>>` preserves signedness; `>>>` fills with zero bits. `f16` is storage-only: convert to `f32` or `f64` before arithmetic.";

const VALUE_REFERENCE_CLASSES: &str = "`@ValueType class` declares a nominal C-layout value class, copied on assignment and argument passing. The `@ValueType({ align: N })` form raises alignment to 2, 4, 8, or 16 bytes and rounds size without changing field offsets. A plain `class` declares a nominal heap reference class: `new` allocates it in the active `Context`, and assignments copy the reference. Value classes do not inherit, and same-shaped nominal types do not substitute for one another. A `FixedArray` or value class result of a call, a read accessor, `new`, or a ternary is a copy, so a write through it fails with S100 (§218): `cur()[0] = 9`, `cv().x = 3`, `obj.vget.x = 1`, `cur()[0]++`, and `cur()[0] += 2`. The text names the copy and states the fix: bind the result to a variable, or assign to the source. A path through a reference class or a dynamic array reaches shared storage and stays legal: `getC().x = 1` and `getArr()[0] = 1`.";

const NAMED_ACCESSORS: &str = "R37 defines named accessors as checker sugar for ordinary methods. `get name(): T` becomes the method `name`. `set name(value: T)` becomes the method `name=`. A read `x.name` calls `name` without arguments. A statement write `x.name = value` calls `name=` with the value. Compound assignments and updates in statement position use a read-then-write rewrite. Static accessors use the same rules through the class name. Reference classes and `@ValueType` value classes can declare read accessors. Only reference classes can declare instance write accessors. Value-position writes, value-class instance write accessors, and mirror accessors are outside the surface.";

const NULLISH_OPERATORS: &str = "`a ?? b` requires `a` to have type `Ref | null` or `Ref | undefined`. It evaluates `a` once and evaluates `b` only when `a` is absent. An optional chain can be the whole left operand of `??`. An optional chain can also be a statement when its last step is a call. Other optional-chain positions require `undefined` and are rejected.";

const NULLABLE_ARRAYS: &str = "`T[] | null` is a legal `Ref | null` form (§217). Its absence is a null pointer. It narrows and flows as a class type does: `=== null`, `==`, narrowing by `if (x !== null)`, `??`, `??=`, and `?.` with a member or a call. A nullable array is a contextual type for an array literal, as its array type is: `const c: f64[] | null = [1, 2];`, and `b([])` with `b(x: f64[] | null)`. `.length`, an index, a method call, and `for...of` need a null test first. `a === b` with two arrays is rejected. A `T[] | null` whose array is a counted owner fails with S011: an array is a counted owner if its element is a counted handle or a counted owner, as `Promise<void>[]`, `Promise<void>[][]`, and `Generator<i32>[]` are. A nullable has no holder count, so the handles in a counted array have no owner. A type argument that makes the array a counted owner fails with the same text. A class and a `Map` own their values, so `J[] | null` for a class `J` with a `Promise<T>` field and `Map<K, Promise<T>>[] | null` are legal. `Map.get` on a `Map` whose value is a counted owner fails with S014 and names `getOr`, and `getOr` is legal there. A mirror declaration cannot hold this form at any depth (S100): a foreign parameter, a foreign result, a field, a constructor, a method, a type alias, and a callback signature. `JSON.stringify` and `JSON.parse<D>` accept a field of this type. A missing key raises, as for `C | null`. A Worker message field stays rejected.";

const NULLABLE_STRINGS: &str = "`string | null` is a legal `Ref | null` form (§218), in each position of `T[] | null`. Its absence is a null pointer. An empty string is not absent: an `s: string | null` that holds `\"\"` gives `false` for `s === null`. A compare against `null` is a null test, which reads no content. `===`, `!==`, `==`, and `!=` compare by content if one operand has type `string | null` and the other has type `string` or `string | null`, and an absent operand equals an absent operand only. The order of the operands does not change the result. A `string | undefined` result compares in the same way and with `== null`. `includes`, `indexOf`, and `lastIndexOf` on `(string | null)[]` use the same comparison. A template operand, `+`, `+=`, a `string` parameter such as that of `print`, and a `switch` discriminant reject a `string | null`, and the text states the fix: a null test first gives a `string`. `.length`, an index, a method call, and `for...of` need a null test first. `??`, `??=`, and `?.` with a member or a call are legal. `join`, `toString()`, and `sort()` on `(string | null)[]` are rejected, and so is a `string | null` key of `Map` or `Set`. `JSON.stringify` and `JSON.parse<D>` accept a field of this type, and a missing key raises, as for `C | null`. A Worker message field, a host entry, and each mirror declaration reject it, as for `T[] | null`: the C ABI of a `string` is a view, which has no null form. `L | null` for a string-literal alias stays rejected.";

const UNDEFINED_RESULTS: &str = "`T | undefined` is legal only as the declared result of a function declaration or a class method, for the `T` of `T | null` (§195). It has the representation of `T | null`. `Map.get`, `find`, and `findLast` return `T | undefined`. `pop()` and `shift()` on `T[]` return `T | undefined` for these `T` (§196): a reference class, a `Map`, a `Set`, a function type, and an array that holds no async handle. On an empty array they give `undefined`. They return `T` for each other `T`, such as a scalar, `string`, a value class, a nullable type, or `Promise<void>[]`. An empty array then traps with `empty-pop`. The `undefined` token is legal in `return undefined;` and as an operand of an absence comparison. `===` and `!==` across a `null` and an `undefined` absence are rejected; `==` tests either absence. A `T | undefined` value has no annotation, no container, and no C boundary crossing.";

const USING_DECLARATIONS: &str = "`using x = e` declares an immutable reference-class binding whose class declares `[Symbol.dispose](): void`. The binding can have type `R | null`. Each initializer evaluates once. Scope exits call the hooks in reverse declaration order and skip null bindings. Inner scopes dispose first; a return expression evaluates before disposal. Suspension preserves resources until completion. Member reads through nullable bindings require ordinary null narrowing. A bare `using x = null` needs a type annotation. Traps do not run disposal. `await using`, value-class hooks, and descriptor-class hooks stay rejected.";

const GENERIC_FUNCTIONS: &str = "A directly named generic function infers omitted type arguments from its arguments (§149) and its contextual type (§212). Imported and namespace-qualified functions use the same rule. Structural candidates pass through arrays (`T[]` and `Array<T>`), FixedArray, nullable types, Map, Set, `Promise<T>`, generic classes, and declared function signatures. Non-literal arguments decide each candidate first. Numeric, string, and array literals use that candidate as their context. With only literals, integer literals default to i32, fractional literals to f64, and string literals to string. Equal candidates agree; C and C | null join to C | null. Conflicting candidates and a type parameter with no candidate require explicit type arguments. Explicit type arguments override inference. Constraints, opaque body checks, and instance-chain checks still apply. Each argument evaluates once, in order. A callback's parameter types do not come from context, so a lambda parameter needs its annotation.";

const GENERIC_CONTEXT_INFERENCE: &str = "`new G(args)` with no type arguments infers the type arguments of a generic class, `Map`, or `Set` (§197). `recv.m(args)` and `C.m(args)` infer the type arguments of a generic method. Candidates come from the arguments, by the rules of a generic function; a constructor uses the parameters of its `constructor`. `new Set(source)` takes its element candidate from a non-literal `source`. A contextual type gives a constructor a candidate: `const b: Box<i64> = new Box(7);`, `const m: Map<string, i32> = new Map();`, `const s: Set<string> = new Set();`. A generic function or method call takes a candidate through its declared result type (§212): with `create<T>(n: i32): Buf<T>`, `const a: Buf<P> = create(1);` is accepted. The context is a local annotation, a parameter, a `return`, a field initializer, an assignment target, or an array element. It also passes through `| null`. A call operand of `await` with the context `C` has the contextual type `Promise<C>` (§215): with `af<T>(): Promise<Buf<T>>`, `const b: Buf<P> = await af();` and `const p: Promise<Buf<P>> = af();` are accepted. The order is a non-literal argument candidate, then the contextual candidate, then the literal default. So `const b: Box<i64> = new Box(n);` with `n: i32` stays a type error. A type list is complete or absent: for `f<A, B>`, the partial list `f<P>(1)` is rejected, also with a context. With no context, a type parameter with no candidate fails with \"cannot infer type parameter `T` of `create`: no candidate; use explicit type arguments\": `const a = create(1);`, `create(1);`, `const b = await af();`. `new Map()` and `new Set()` with no context fail with their Q24 text. These positions give no contextual candidate: an `as` operand, an optional call, an arithmetic operand, an index, a `Promise.all` element, and an `await` operand that is not a call. A call under `await` at one of these positions takes none too: `const x: i32 = -(await az());` is rejected. `return af();` in an async function stays rejected, also as `return af<P>();`: an async function has no promise adoption (§167 rule 3), so write `return await af();`. An `await` of a synchronous generic callee gets no context from the `await`. A `null` argument for a parameter `T` or `T | null` stops the contextual candidate of `T`.";

const GENERIC_METHODS: &str = "R39.6 admits type parameters on a method of a non-generic class, instance or static. A method call infers omitted type arguments from its arguments and its contextual type (§197, §212), or supplies them explicitly. A type parameter with no candidate needs an explicit list: `box.empty<i32>()`. Each distinct type-argument list yields one method instance named `m<A>` in the class's instance or static namespace. The declared name owns the member namespace, and an instance name collides with no declared member. An async instance method on a non-generic reference class can declare type parameters (§93). A generic method on a generic class stays outside the surface.";

const Q33_DESCRIPTORS: &str = "`@Descriptor class` declares a closed, data-only reference class for literal construction. A required member is written `name!: T`; a defaulted member is written `name?: T = default`. When `A` is a Q32 string-literal union alias, `name?: A` without an initializer is absence-capable: omission is a distinct state, explicit `undefined` is rejected, and reads are legal only in the present arm established by `member != undefined` / `member !== undefined` or the inverse arm of `member == undefined` / `member === undefined`. No other member type admits that spelling. Literals may be nested, may omit defaulted and absence-capable members, and remain constructible through a `Descriptor | null` contextual type. Construction uses an object literal in a descriptor context; `new Descriptor(...)`, literals against plain nominal classes (including through `| null`), methods, missing required members, and excess members are rejected.";

const Q32_LITERAL_UNIONS: &str = "Q32 admits declared aliases whose members are string literals, such as `type Mode = \"fast\" | \"safe\"`. The member set is closed and the alias is nominal: a non-member, an inline literal union, or a value from a distinct same-shaped alias is rejected. A switch over an alias uses member string literals as case labels. Without `default` it must name every member exactly once; with `default` any subset of distinct members is accepted. A default-less exhaustive alias switch is a diverging statement when every arm diverges, so it satisfies non-void function return flow without a trailing return. A value of a Q32 alias does not convert to `string`. `type L2 = Label;` names the same closed set (§202). `xs.join(s)` on an array of a Q32 alias is `` xs.map((x) => `${x}`).join(s) ``, and `xs` evaluates once (§216). An array of a Q32 alias takes each array method that an array of an enum takes, and `includes` and `indexOf` compare the alias value. `<` on a Q32 alias, `sort()` with no comparator, and `find` and `findLast` stay rejected.";

const DIVERGENCE_FLOW: &str = "`unreachable()` is legal only as a call statement. It marks that path as diverging for return-flow analysis and traps with `unreachable-reached` if execution reaches the call. Returns, `unreachable()`, and recursively exhaustive Q32 and source-enum switches compose as diverging statements; a switch arm that falls through or breaks does not diverge. A source-enum switch with a `case` for each member and no `default` satisfies return flow when each arm diverges (§200). At the end of a function that returns a value, a value that is not a member traps at that switch with `unreachable-reached`. A switch with a missing member or a `break` arm keeps \"not all paths return a value\". An exhaustive switch followed by a statement that reaches the end of the function keeps that text too. A following `return` is reached by a value that is not a member, as in JavaScript.";

const PARAMETER_PROPERTIES: &str = "A constructor parameter with `public`, `private`, `protected`, or `readonly`, or a combination that TypeScript accepts, is a parameter property (§198). It declares an instance field with the name, type, and modifiers of the parameter. The parameter-property fields come first, in parameter order, and the declared fields follow. The C layout of a value class and the key order of `JSON.stringify` use this order. At construction, the field initializers run first. Then the constructor assigns each parameter property, in parameter order, and then its body runs. With no annotation, a literal default gives the field its type: `constructor(public count = 0)` declares an `i32`. Any other default with no annotation fails with \"a parameter property requires a type annotation\". A field initializer that reads a parameter-property field is rejected. A field and a parameter property with one name fail with S017.";

const LOGICAL_ASSIGNMENT: &str = "`p ??= v`, `p ||= v`, and `p &&= v` are tested assignments (§199). `v` evaluates only if the assignment runs. The receiver and the index of `p` evaluate once. `p` is a place that a plain assignment accepts: a local, a field, a static field, an array element, or an accessor. For `??=`, `p` must be nullable, or the form fails with \"the left operand of `??=` has type `T`, which is not nullable\". For `||=` and `&&=`, `p` and `v` must be `boolean`; there is no truthiness test on another type. `x ??= v` in statement position, with `v` of the unwrapped type, narrows `x` as the spelled `if` does. Value position is legal, but it does not narrow `x`. An accessor place is legal in statement position only.";

const MULTI_ARGUMENT_CALLS: &str = "`r.push(a1, …, ak)` takes any argument count, zero included (§201). It evaluates `r` once, then every argument in order, and then appends each value. The result is the length after the last append. `Math.max` and `Math.min` take any count of `f64` arguments and fold from the left. `Math.max(a)` is `a`, `Math.max()` is `-Infinity`, and `Math.min()` is `Infinity`. `Array.of(a1, …, ak)` gives the array `[a1, …, ak]`. A spread argument stays rejected: `xs.push(...ys)` and `Math.max(...xs)`. `Math.hypot` with three or more arguments stays rejected. `unshift` with two or more arguments, the insert form of `splice`, and `concat` with two or more arrays also stay rejected.";

const TYPE_ALIASES: &str = "A module-level `type N = T;` declares a second name of `T` (§202). `N` has no type parameters, and `T` is not a union of string literals. Each use of `N` in a type position is the type `T`, and a diagnostic prints `T`. The alias has no layout, no C name, and no run-time form. `T` can be a class, an enum, a sized numeric, an array, a function type, `C | null`, a generic instance, or another alias. Declaration order does not matter. `export type N = T;` in an imported module exports the alias; the entry module exports functions only. A union of two or more string literals declares a Q32 alias instead. That alias is a nominal closed set, not a second name of `string`. Two Q32 aliases with one member set stay distinct. An alias of a Q32 alias is that same set. A generic alias, a `T | undefined` member, an alias that holds a `TaskGroup`, and an alias in a function body stay rejected. `type One = \"a\";` stays rejected too. An async function result must spell `Promise<T>`, not an alias of it. An alias that reaches itself fails with \"type alias `N` refers to itself\", also through an array or a type argument (`type L = L[];`). A type argument on an alias fails with \"type alias `N` is not generic\". `new N()` fails with \"type alias `N` used as a value\".";

const DO_WHILE: &str = "`do { body } while (c);` runs `body`, then evaluates the `boolean` test `c`, and repeats while `c` is `true` (§203). `continue` goes to the evaluation of `c`, and `break` leaves the loop. The form is legal wherever `while` is legal, including a lambda, a generator, and an async function. A local that `body` assigns on every path to `c` is assigned in `c`. An assignment after a `continue` does not count on that path. `c` narrows the next iteration: `do { s += n.v; } while ((n = n.next) !== null);` reads `n.v` with `n` not null. After a loop with no `break`, the false arm of `c` narrows as after a `while` loop.";

const ERASED_FORMS: &str = "Global `isNaN(x)` and `isFinite(x)` are `Number.isNaN(x)` and `Number.isFinite(x)` (§204). The argument is an `f64`, and each name can only be called, not read as a value. `` `${xs}` `` with an array `xs` is `` `${xs.join(\",\")}` ``, and `xs` evaluates once. An element type that `join` rejects, such as a nested array, fails with the `join` text. A `FixedArray` operand and a class operand stay rejected. `x satisfies T` checks `x` with the context `T`, and a mismatch fails with the text of the assignment to a `T` local. The expression is `x`, with the type of `x`. An operand with an empty array literal gets no context, so `[] satisfies string[]` is rejected. `debugger;` is an empty statement. `<T>x` and `string +` an array stay rejected.";

const INTERFACES: &str = "A module-level `interface I { … }` declares a type-only constraint (§208). It has no layout, no C name, and no run-time form. A member is a method signature `m(a: A): R;` or a read accessor signature `get g(): R;`. `class C implements I, J` declares that `C` has each member, and the checker compares them. A method signature needs an instance method with the same parameter and result types. A read accessor signature needs an instance read accessor or an instance field with that name and type. A `private` member and a static member do not count. An `async` method implements a member with the result `Promise<R>`. A reference class, a value class, and a generic class can declare `implements`. `T extends I` on a function, a method, or a class lets the body call each member of `I` on a value of type `T`. Each instance calls the member of its type argument directly, with no dispatch. A type argument must declare `implements I`, or be a type parameter with that constraint. A class with the same members and no clause fails with \"`A` does not declare `implements I`\". An interface is not a value type. It fails with \"interface `I` is a constraint only; write a type parameter `T extends I`\". This applies to a parameter, a result, a local, a field, an array element, a union, and a type argument. An interface in an imported module can be exported; the entry module exports functions only. Each other member form fails with \"interface `I` has a member form outside the decided surface\". This text covers a property, an optional member, and a call, construct, or index signature. It covers a generic method signature, a write accessor signature, a rest parameter, and an overload. It also covers a type parameter on the interface, `extends` on the interface, and a second declaration of one name. A local interface stays rejected. An `implements` clause that names another kind fails with \"`implements` names an interface; `A` is not an interface\". `T extends I & J` and a union constraint stay rejected.";

const PRIVATE_NAMES: &str = "A class can declare a `#x` instance field, static field, instance method, static method, getter, and setter, with `readonly` on a field (§206). The class body can read and write `this.#x`, `o.#x` where `o` has the class type, and `C.#s`. Code outside the class body fails with \"member `#x` is private in class `C`\". `#x` and `x` are two members of one class, and no brand check runs. A `#x` field is a field of the C layout, in declaration order. `JSON.stringify` omits it, but its type must still be a JSON type. `JSON.parse<C>` fails if `C` has a `#x` field. `#x in o`, `o?.#x`, a `#x` member of a `@Descriptor` class, and a static and an instance member with one private name stay rejected.";

const FUNCTION_VALUES: &str = "`C.m`, where `m` is a static method of `C`, is a function value, as a module function is (§207). It can be an argument, a local, or an array element. A generic static method and a static generator method stay rejected as values. A `function` declaration in a function body, a method body, a lambda body, or a block is a local function. Its name is visible in the whole block, so a call before the declaration is legal. It can call itself and the other local functions of its block, and it is a function value. A local function cannot read or write a local, a parameter, `this`, or a type parameter of an enclosing function. The form fails with \"local function `f` reads the local `k` of its enclosing function\". Use a `const` lambda instead. Two local functions with one name in one block fail with S017. A local class, enum, type alias, interface, and namespace stay rejected. An assignment to an instance or static method fails with \"cannot be assigned: a method is a function, not a field\" (§209). A field of a function type is the assignable form.";

const Q34_ASYNC: &str = "Q34/R13/R36/§70/§167/§170 admits `await Context.suspend()`, direct awaits of named async functions and async instance methods, and a held async handle awaited later. A named async function is a function value. A non-capturing async arrow is accepted. Each call returns a started handle. An async expression body follows the named async return rule: a handle body is rejected; use an explicit await (§167 rule 3). An async arrow owns its const captures in a reference environment (§181). Mutable captures and `this` stay rejected. Await accepts a call through a function value in a local, a parameter, a global, a field, or an array element. The named async function and reference class can be generic. Direct calls accept `await f(...)`, `await f<A>(...)`, `await recv.m(...)`, and `await recv.m<A>(...)` (§93). `Promise<T>` is the TypeScript-compatible annotation and storage view of that handle, not a Promise object: constructors and statics other than `Promise.all` do not exist. On a handle, `then(f)`, `then(f, r)`, `catch(r)`, and `finally(f)` create the handle of a compiler-supplied async helper. Their callbacks run in the `node` order, and a direct synchronous callback lambda owns its const captures (§186). A `catch` callback returns the handle value type. Locals, arrays, fields, and globals can hold handles (§70.3 rule 2a). A TaskGroup lives in one const local; a synchronous parameter borrows it. add moves one task count and registers a group reaction at the call. join closes the group and waits for every task, then reports the first failure in reaction order. The scope exit of an unjoined group traps with TaskGroup (33) if unfinished or failed tasks remain. Fields, arrays, results, assignments, and captures cannot hold groups (§170). A generator body cannot use a group: a dropped iterator has no scope exit. Handles can pass to another function. Every created handle must have at least one awaited completion. Copies retain the Context-owned frame, lexical exits release it, and `await` does not change ownership. Under \u{a7}94 an async call runs its callee to that callee's first await or return, and every await suspends its caller, a completed handle included: the await registers a continuation and returns. A completion queues the continuations registered on it, in registration order. Only a host checkpoint runs those continuations, so there is no event loop and no autonomous scheduler. An exception that leaves an async body completes its handle with the Error object, report text, and last throw position (§116.1 rules 1–4). The call returns the handle without raising the exception. An await of that handle raises the same object with the same report text and position. Each later await raises it again without a copy. If no await raises the exception, the last holder's release traps with `TrapKind::UncaughtException` (29), that report text, and that position. An exception that leaves a host-callable async export traps with `TrapKind::UncaughtException` (29), because the export has no script holder (§116.1 rule 5).";

const Q35_WORKERS: &str = "`Worker<In, Out>` runs one directly named synchronous function, module-level or local, on an OS thread with a fresh `Context`. Its exact entry shape is `(inbox: Inbox<In>, outbox: Outbox<Out>): void`; entries cannot capture state. Message classes are monomorphized by the `In`/`Out` pair and may contain sized numerics, booleans, enums, string-literal union aliases, value classes, top-level `string` fields, and top-level `FixedArray<string, N>` fields. Delivery copies every string's bytes into a fresh string owned by the receiving `Context`; other reference, growable-array, function, and nullable fields remain non-transferable. Worker handles and endpoints are context-affine: they cannot be module globals, class fields, array elements, any `Map`/`Set` type argument, or lambda captures, and they can only be created by `Worker.spawn`. Post all independent work before joining when parallel execution matters; `wait` and `poll` return `null` when their channel is empty or closed, and `close` plus `join` provide explicit shutdown.";

const MODULES: &str = "A program may import named exports from sibling source files with a relative `./name` specifier. The entry file and its imported siblings are checked as one program; exports are the host-visible entry surface.";

const COROUTINES: &str = "A `function*` coroutine yields typed values and is driven explicitly through `Generator<T>.next()` or the accepted `for...of` generator path. Suspension is caller- or host-driven; the language does not schedule coroutine steps implicitly. A finished generator traps with `generator-done-value` when the zero of its value type is invalid. The check includes strings, fixed-array elements, and value-class fields. Iterator-result binding patterns are rejected. Use `const r = it.next(); if (r.done) ...` before the member read `r.value`. Nullable values and scalar zero values remain valid (compiler.md §145).";

const MEMORY_MODEL: &str = "Reference allocations belong to a `Context`. `Context.free(value)` releases one allocation explicitly; `Context.collect()` performs an explicitly requested reachability collection. No collection runs implicitly. `Context.bytesOf<T>` returns zero-padded storage bytes for eligible `@ValueType` and `FixedArray` values. `bytesInto` writes that form, and `fromBytes` reconstructs storage without initialization. W001 flags unreleased loop allocations, W002 flags straight-line use after `Context.free`, and W003 flags fresh rooted callback userdata registered in a loop.";

struct Feature {
    title: &'static str,
    prose: &'static str,
    corpus: &'static [&'static str],
}

const FEATURES: &[Feature] = &[
    Feature {
        title: "Shared-location narrowing",
        prose: "A narrowed field or module global carries a runtime null check. A null read traps with `null-narrowing` and message `a narrowed shared location is null`. Its runtime kind is `SharedNullNarrowing` (31). Checked `as` narrowing keeps `NullNarrowing` (4) and message `` `as` narrowing applied to null ``. A narrowed local has no shared-read check.",
        corpus: &["corpus/accept/a274-narrow-again-after-a-call.ts", "corpus/trap/t72-narrowing-boundary-getter.ts"],
    },
    Feature {
        title: "String constants",
        prose: "String literals and static template parts have no language byte limit. Ordinary allocation and runtime string limits still apply. The ship tier uses C literals through 65,000 decoded UTF-8 bytes and file-scope static byte arrays above that threshold. String-alias members use the same data representation. Static addresses and decoded lengths preserve Context-owned literal interning, collection roots, and traps.",
        corpus: &[
            "corpus/accept/a183-long-string-literal.ts",
            "corpus/accept/a204-static-long-string.ts",
        ],
    },
    Feature {
        title: "Scope-exit disposal",
        prose: USING_DECLARATIONS,
        corpus: &[
            "corpus/accept/a199-using-nullable.ts",
            "corpus/accept/a200-using-nullable-async.ts",
            "corpus/accept/a201-using-nullable-switch.ts",
            "corpus/reject/r132-await-using.ts",
            "corpus/reject/r133-using-without-dispose.ts",
        ],
    },
    Feature {
        title: "Sized numerics",
        prose: SIZED_NUMERICS,
        corpus: &[
            "corpus/accept/a02-integer-types.ts",
            "corpus/accept/a03-integer-literals.ts",
            "corpus/accept/a202-shift-count-spellings.ts",
            "corpus/accept/a203-shift-count-widths.ts",
            "corpus/accept/a46-narrow-numerics.ts",
            "corpus/accept/a49-f16-conversions.ts",
            "corpus/reject/r08-bare-number.ts",
            "corpus/reject/r09-int-literal-overflow.ts",
            "corpus/reject/r36-f16-arithmetic.ts",
        ],
    },
    Feature {
        title: "Value and reference classes",
        prose: VALUE_REFERENCE_CLASSES,
        corpus: &[
            "corpus/accept/a04-value-struct.ts",
            "corpus/accept/a05-nominal-identity.ts",
            "corpus/accept/a15-manual-lifetime.ts",
            "corpus/accept/a21-methods.ts",
            "corpus/accept/a141-valuetype-align.ts",
            "corpus/reject/r06-structural-substitution.ts",
            "corpus/reject/r462-write-through-value-temporary.ts",
            "corpus/reject/r07-value-class-extends.ts",
            "corpus/reject/r135-valuetype-align-below-natural.ts",
            "corpus/reject/r136-valuetype-align-not-in-set.ts",
        ],
    },
    Feature {
        title: "Named accessors",
        prose: NAMED_ACCESSORS,
        corpus: &[
            "corpus/accept/a144-accessor.ts",
            "corpus/accept/a176-compound-through-accessor.ts",
            "corpus/accept/a179-static-read-accessor.ts",
            "corpus/reject/r141-value-class-write-accessor.ts",
            "corpus/reject/r142-readonly-accessor-write.ts",
            "corpus/reject/r145-accessor-write-as-value.ts",
            "corpus/reject/r173-compound-write-as-value.ts",
        ],
    },
    Feature {
        title: "Nullish operators",
        prose: NULLISH_OPERATORS,
        corpus: &[
            "corpus/accept/a177-nullish.ts",
            "corpus/reject/r177-nullish-non-nullable-left.ts",
            "corpus/reject/r178-optional-chain-unbound.ts",
        ],
    },
    Feature {
        title: "Nullable arrays",
        prose: NULLABLE_ARRAYS,
        corpus: &[
            "corpus/accept/a400-nullable-array.ts",
            "corpus/reject/r459-nullable-array-of-handles.ts",
            "corpus/reject/r460-map-get-counted-array.ts",
        ],
    },
    Feature {
        title: "Nullable strings",
        prose: NULLABLE_STRINGS,
        corpus: &[
            "corpus/accept/a402-nullable-string.ts",
            "corpus/reject/r461-nullable-string-template.ts",
        ],
    },
    Feature {
        title: "Undefined results",
        prose: UNDEFINED_RESULTS,
        corpus: &[
            "corpus/accept/a373-undefined-result.ts",
            "corpus/accept/a375-array-pop-shift-undefined.ts",
            "corpus/reject/r424-undefined-strict-null-compare.ts",
            "corpus/reject/r428-undefined-local-annotation.ts",
            "corpus/reject/r435-map-get-nullable-value.ts",
            "corpus/reject/r439-array-pop-unnarrowed.ts",
        ],
    },
    Feature {
        title: "Generic function inference",
        prose: GENERIC_FUNCTIONS,
        corpus: &[
            "corpus/accept/a324-generic-inference/main.ts",
            "corpus/reject/r330-generic-inference-conflict.ts",
            "corpus/reject/r331-generic-inference-return-only.ts",
            "corpus/reject/r332-generic-inference-only-null.ts",
        ],
    },
    Feature {
        title: "Generic construction and contextual inference",
        prose: GENERIC_CONTEXT_INFERENCE,
        corpus: &[
            "corpus/accept/a377-generic-construction-inference.ts",
            "corpus/accept/a378-generic-construction-nested-context.ts",
            "corpus/accept/a397-contextual-result-inference.ts",
            "corpus/accept/a398-inference-promise-array.ts",
            "corpus/reject/r440-generic-constructor-conflicting-candidates.ts",
            "corpus/reject/r441-generic-constructor-no-candidate.ts",
        ],
    },
    Feature {
        title: "Generic methods",
        prose: GENERIC_METHODS,
        corpus: &[
            "corpus/accept/a178-generic-method.ts",
            "corpus/reject/r179-generic-method-without-type-args.ts",
            "corpus/reject/r180-generic-method-on-generic-class.ts",
            "corpus/accept/a187-async-generic-method.ts",
            "corpus/reject/r186-async-generic-method-without-type-args.ts",
            "corpus/reject/r187-async-generic-method-on-value-class.ts",
        ],
    },
    Feature {
        title: "Q33 descriptors",
        prose: Q33_DESCRIPTORS,
        corpus: &[
            "corpus/accept/a92-descriptor-literals.ts",
            "corpus/accept/a117-descriptor-literal-nullable-member.ts",
            "corpus/accept/a118-absence-capable-member.ts",
            "corpus/reject/r90-descriptor-missing-required.ts",
            "corpus/reject/r91-descriptor-excess-member.ts",
            "corpus/reject/r92-literal-for-unmarked-class.ts",
            "corpus/reject/r93-descriptor-optional-without-default.ts",
            "corpus/reject/r94-descriptor-method.ts",
            "corpus/reject/r95-descriptor-new.ts",
            "corpus/reject/r137-descriptor-align.ts",
            "corpus/reject/r116-object-literal-nullable-class.ts",
            "corpus/reject/r117-explicit-undefined-member.ts",
            "corpus/reject/r118-unnarrowed-absence-read.ts",
        ],
    },
    Feature {
        title: "Q32 string-literal unions",
        prose: Q32_LITERAL_UNIONS,
        corpus: &[
            "corpus/accept/a91-string-literal-union.ts",
            "corpus/reject/r87-literal-union-nonmember.ts",
            "corpus/reject/r88-literal-union-inline.ts",
            "corpus/reject/r89-literal-union-cross-alias.ts",
            "corpus/accept/a115-switch-literal-union.ts",
            "corpus/accept/a116-exhaustive-switch-returns.ts",
            "corpus/accept/a399-join-string-alias.ts",
            "corpus/reject/r112-switch-alias-missing-member.ts",
            "corpus/reject/r113-switch-alias-non-member.ts",
            "corpus/reject/r114-switch-alias-duplicate-member.ts",
        ],
    },
    Feature {
        title: "Divergence flow",
        prose: DIVERGENCE_FLOW,
        corpus: &[
            "corpus/accept/a116-exhaustive-switch-returns.ts",
            "corpus/accept/a383-exhaustive-enum-switch.ts",
            "corpus/reject/r115-unreachable-as-value.ts",
            "corpus/trap/t47-unreachable-reached.ts",
            "corpus/trap/t112-enum-switch-non-member-value.ts",
        ],
    },
    Feature {
        title: "Constructor parameter properties",
        prose: PARAMETER_PROPERTIES,
        corpus: &[
            "corpus/accept/a379-constructor-parameter-properties.ts",
            "corpus/accept/a380-value-class-parameter-properties.ts",
            "corpus/reject/r442-parameter-property-read-by-field-initializer.ts",
            "corpus/reject/r443-parameter-property-value-class-instance.ts",
        ],
    },
    Feature {
        title: "Logical assignment",
        prose: LOGICAL_ASSIGNMENT,
        corpus: &[
            "corpus/accept/a381-logical-assignment.ts",
            "corpus/accept/a382-logical-assignment-value-projection.ts",
            "corpus/reject/r444-logical-or-assignment-on-string.ts",
            "corpus/reject/r445-nullish-assignment-on-non-nullable.ts",
        ],
    },
    Feature {
        title: "Multi-argument calls",
        prose: MULTI_ARGUMENT_CALLS,
        corpus: &[
            "corpus/accept/a384-multi-argument-calls.ts",
            "corpus/accept/a385-multi-argument-push-receiver.ts",
            "corpus/reject/r16-math-variadic-max.ts",
        ],
    },
    Feature {
        title: "Type aliases",
        prose: TYPE_ALIASES,
        corpus: &[
            "corpus/accept/a386-transparent-type-alias/main.ts",
            "corpus/accept/a386-transparent-type-alias/lib.ts",
            "corpus/accept/a387-type-alias-generic-instance.ts",
            "corpus/reject/r446-type-alias-refers-to-itself.ts",
            "corpus/reject/r447-type-alias-rejected-annotation.ts",
        ],
    },
    Feature {
        title: "do…while",
        prose: DO_WHILE,
        corpus: &[
            "corpus/accept/a388-do-while.ts",
            "corpus/accept/a389-do-while-test-narrows-the-body.ts",
            "corpus/reject/r448-do-while-continue-skips-assignment.ts",
        ],
    },
    Feature {
        title: "Erased forms",
        prose: ERASED_FORMS,
        corpus: &[
            "corpus/accept/a390-trivial-erasures.ts",
            "corpus/reject/r449-satisfies-mismatch.ts",
            "corpus/reject/r450-template-nested-array.ts",
        ],
    },
    Feature {
        title: "Interfaces and implements",
        prose: INTERFACES,
        corpus: &[
            "corpus/accept/a394-interface-constraint/main.ts",
            "corpus/accept/a394-interface-constraint/shapes.ts",
            "corpus/accept/a395-interface-async-member.ts",
            "corpus/reject/r451-class-implements.ts",
            "corpus/reject/r455-interface-value-type.ts",
            "corpus/reject/r456-class-does-not-implement.ts",
            "corpus/reject/r457-constraint-needs-implements.ts",
        ],
    },
    Feature {
        title: "Private names",
        prose: PRIVATE_NAMES,
        corpus: &[
            "corpus/accept/a391-private-names.ts",
            "corpus/reject/r452-private-name-outside-its-class.ts",
        ],
    },
    Feature {
        title: "Static method and local function values",
        prose: FUNCTION_VALUES,
        corpus: &[
            "corpus/accept/a392-static-method-and-local-function-values.ts",
            "corpus/accept/a393-local-function-in-a-class-body.ts",
            "corpus/reject/r453-local-function-reads-a-local.ts",
            "corpus/reject/r454-local-function-declared-twice.ts",
            "corpus/reject/r458-method-assignment.ts",
        ],
    },
    Feature {
        title: "Q34 async",
        prose: Q34_ASYNC,
        corpus: &[
            "corpus/accept/a93-async-chain.ts",
            "corpus/accept/a94-async-two-roots.ts",
            "corpus/accept/a95-interop-async-await.ts",
            "corpus/accept/a110-async-method-receiver.ts",
            "corpus/accept/a111-interop-async-method-poll.ts",
            "corpus/accept/a143-async-generic/main.ts",
            "corpus/accept/a181-operation-in-every-owner.ts",
            "corpus/accept/a336-async-function-values.ts",
            "corpus/accept/a337-task-group.ts",
            "corpus/accept/a357-promise-then-chains.ts",
            "corpus/accept/a358-promise-catch-finally.ts",
            "corpus/accept/a359-promise-then-captures.ts",
            "corpus/reject/r404-catch-callback-type.ts",
            "corpus/reject/r383-unjoined-task-group.ts",
            "corpus/reject/r384-async-task-group-result.ts",
            "corpus/reject/r385-task-group-field.ts",
            "corpus/reject/r386-task-group-generator-body.ts",
            "corpus/trap/t84-dropped-unfinished-task-group.ts",
            "corpus/trap/t85-dropped-failed-task-group.ts",
            "corpus/trap/t86-add-to-closed-task-group.ts",
            "corpus/reject/r379-async-arrow-capture.ts",
            "corpus/reject/r380-dropped-indirect-async-handle.ts",
            "corpus/reject/r381-async-arrow-result-annotation.ts",
            "corpus/reject/r382-async-arrow-handle-body.ts",
            "corpus/reject/r96-new-promise.ts",
            "corpus/reject/r97-promise-combinator.ts",
            "corpus/reject/r98-promise-static.ts",
            "corpus/reject/r99-await-outside-async.ts",
            "corpus/reject/r100-floating-async-call.ts",
            "corpus/reject/r101-async-static-method.ts",
            "corpus/reject/r102-async-generator-method.ts",
            "corpus/reject/r103-async-valuetype-method.ts",
            "corpus/reject/r105-floating-async-method-call.ts",
        ],
    },
    Feature {
        title: "Q35 workers",
        prose: Q35_WORKERS,
        corpus: &[
            "corpus/accept/a112-worker-echo.ts",
            "corpus/accept/a113-worker-parallel.ts",
            "corpus/accept/a182-worker-string-message.ts",
            "corpus/reject/r106-capturing-lambda-worker-entry.ts",
            "corpus/reject/r107-async-worker-entry.ts",
            "corpus/reject/r109-worker-module-global.ts",
            "corpus/reject/r110-new-worker.ts",
            "corpus/reject/r111-worker-in-map-value.ts",
            "corpus/reject/r182-worker-reference-field.ts",
            "corpus/reject/r183-worker-growable-array-field.ts",
            "corpus/reject/r281-affine-new-map.ts",
        ],
    },
    Feature {
        title: "Modules",
        prose: MODULES,
        corpus: &[
            "corpus/accept/a19-modules/main.ts",
            "corpus/accept/a19-modules/math.ts",
        ],
    },
    Feature {
        title: "Coroutines",
        prose: COROUTINES,
        corpus: &[
            "corpus/accept/a20-coroutine-generator.ts",
            "corpus/accept/a79-for-of-generator.ts",
            "corpus/accept/a180-for-of-generator-only.ts",
            "corpus/accept/a310-generator-done-reference-controls.ts",
            "corpus/accept/a311-generator-done-scalar-zero.ts",
            "corpus/accept/a312-generator-done-value-zero.ts",
            "corpus/trap/t75-generator-done-value-member.ts",
            "corpus/trap/t76-generator-done-value-store.ts",
            "corpus/trap/t77-generator-done-fixed-reference.ts",
            "corpus/trap/t78-generator-done-string.ts",
            "corpus/reject/r300-generator-result-pattern-field.ts",
            "corpus/reject/r301-generator-done-destructuring.ts",
        ],
    },
    Feature {
        title: "Memory model",
        prose: MEMORY_MODEL,
        corpus: &[
            "corpus/accept/a15-manual-lifetime.ts",
            "corpus/accept/a16-explicit-collect.ts",
            "corpus/accept/a90-callback-userdata-rooted.ts",
            "corpus/accept/a142-bytes-of.ts",
            "corpus/accept/a244-registration-userdata-reclaimed.ts",
            "corpus/reject/r138-bytes-of-reference-class.ts",
            "corpus/reject/r139-bytes-of-string-element.ts",
            "corpus/warn/w01-loop-allocation-unreleased.ts",
            "corpus/warn/w02-use-after-free.ts",
            "corpus/warn/w03-fresh-callback-userdata-loop.ts",
            "corpus/trap/t22-double-delete-q6.ts",
            "corpus/trap/t23-use-after-delete-q6.ts",
            "corpus/trap/t46-callback-userdata-freed.ts",
            "corpus/trap/t51-bytes-into-range.ts",
            "corpus/trap/t60-registration-fire-after-release.ts",
        ],
    },
];

#[derive(Debug)]
struct Pin {
    file: String,
    code: String,
    line: usize,
}

#[derive(Debug)]
struct HeaderField {
    key: String,
    value: String,
    line: usize,
}

/// The metadata in a corpus source's initial comment block.
#[derive(Debug)]
#[non_exhaustive]
pub struct CorpusHeader {
    /// The reason the reference interpreter cannot run this entry, if any.
    pub interpreter_exclusion: Option<String>,
    /// Whether the debug interpreter sweep omits this cost benchmark.
    pub benchmark: bool,
    corpus: String,
    purpose: String,
    exercises: String,
    questions: String,
    fields: Vec<HeaderField>,
}

impl CorpusHeader {
    fn guidance(&self) -> impl Iterator<Item = &HeaderField> {
        self.fields.iter().filter(|field| {
            !matches!(
                field.key.as_str(),
                "corpus"
                    | "purpose"
                    | "exercises"
                    | "questions"
                    | "warning"
                    | "interpreter"
                    | "cost"
            )
        })
    }
}

/// Renders the generated current-state language reference.
///
/// # Errors
///
/// Fails when a harness pin cannot be parsed, a stable code has no pin, a
/// pinned corpus file is invalid, or a curated corpus link does not exist.
pub fn render_language_reference(repository_root: &Path) -> io::Result<String> {
    let reject_pins = reject_pins(repository_root)?;
    let warn_pins = warn_pins(repository_root)?;
    let mut out = String::new();

    writeln!(out, "<!-- DO NOT EDIT. Generated by `{GENERATED_BY}`. -->").expect("write to String");
    writeln!(out, "\n# subscript language reference").expect("write to String");
    writeln!(out, "\n## Current-state surface summary").expect("write to String");
    writeln!(out, "\n{SURFACE_SUMMARY}").expect("write to String");

    writeln!(out, "\n## Rejection rules").expect("write to String");
    writeln!(
        out,
        "\nEach excerpt comes from the first `(file, code, line)` pin for that code in `compiler/tests/corpus_reject.rs`; S100 therefore uses its first table entry. Excerpts include two source lines before and after the pinned line when available."
    )
    .expect("write to String");
    for code in RuleCode::ALL {
        let pin = reject_pins
            .iter()
            .find(|pin| pin.code == code.as_str())
            .ok_or_else(|| invalid(format!("{} has no reject-corpus pin", code)))?;
        render_code_entry(
            &mut out,
            repository_root,
            "reject",
            code.as_str(),
            code.explanation(),
            pin,
        )?;
    }

    writeln!(out, "\n## Listed diagnostic examples (§182)").expect("write to String");
    for (name, entry) in [
        (
            "TaskGroup in a generator body",
            crate::check::diagnostic_text::GROUP,
        ),
        ("Date method value", crate::check::diagnostic_text::DATE),
        (
            "Function array map result",
            crate::check::diagnostic_text::FUNCTION_MAP,
        ),
        (
            "Async array map result",
            crate::check::diagnostic_text::ASYNC_MAP,
        ),
        ("Promise.then", crate::check::diagnostic_text::THEN),
        ("Promise.catch", crate::check::diagnostic_text::CATCH),
        ("Promise.finally", crate::check::diagnostic_text::FINALLY),
        (
            "Async receiver capture",
            crate::check::diagnostic_text::RECEIVER,
        ),
        (
            "Promise reaction receiver capture",
            crate::check::diagnostic_text::REACTION_RECEIVER,
        ),
        (
            "Async handle return",
            crate::divergence::Divergence::AsyncReturnHandle.entry(),
        ),
        (
            "Mutable async capture",
            crate::divergence::Divergence::AsyncArrowCapture.entry(),
        ),
        (
            "Await of a synchronous call",
            crate::divergence::Divergence::AwaitIndirectCall.entry(),
        ),
        (
            "Generic function value",
            crate::divergence::Divergence::GenericFunctionValue.entry(),
        ),
    ] {
        writeln!(out, "\n### {name}\n\nTypeScript:\n\n```typescript\n{}\n```\n\nsubscript:\n\n```typescript\n{}\n```\n\n{} ({})", entry.ts, entry.subscript, entry.why, entry.collision).expect("write to String");
    }

    writeln!(out, "\n## Warning rules").expect("write to String");
    writeln!(
        out,
        "\nWarnings do not change acceptance unless the CLI is run with `--deny-warnings`. Each excerpt comes from the first `(file, code, line)` pin in `compiler/tests/corpus_warn.rs`."
    )
    .expect("write to String");
    for code in WarnCode::ALL {
        let pin = warn_pins
            .iter()
            .find(|pin| pin.code == code.as_str())
            .ok_or_else(|| invalid(format!("{} has no warning-corpus pin", code)))?;
        render_code_entry(
            &mut out,
            repository_root,
            "warn",
            code.as_str(),
            code.explanation(),
            pin,
        )?;
    }

    writeln!(out, "\n## Feature guide").expect("write to String");
    for feature in FEATURES {
        writeln!(out, "\n### {}", feature.title).expect("write to String");
        writeln!(out, "\n{}", feature.prose).expect("write to String");
        write!(out, "\nCorpus: ").expect("write to String");
        for (index, relative) in feature.corpus.iter().enumerate() {
            let path = repository_root.join(relative);
            if !path.is_file() {
                return Err(invalid(format!(
                    "feature {} links missing corpus file {}",
                    feature.title,
                    path.display()
                )));
            }
            if index > 0 {
                write!(out, ", ").expect("write to String");
            }
            write!(out, "[`{relative}`](../{relative})").expect("write to String");
        }
        writeln!(out, ".").expect("write to String");
    }

    Ok(out)
}

/// Renders the generated four-arm corpus index.
///
/// # Errors
///
/// Fails when a corpus entry lacks a structured header, names the wrong
/// source path, or lacks its required accept/trap expected-output file.
pub fn render_corpus_index(repository_root: &Path) -> io::Result<String> {
    let mut out = String::new();
    writeln!(out, "<!-- DO NOT EDIT. Generated by `{GENERATED_BY}`. -->").expect("write to String");
    writeln!(out, "\n# subscript corpus index").expect("write to String");
    writeln!(
        out,
        "\nThis index is derived from the structured header comments on every TypeScript corpus source."
    )
    .expect("write to String");

    for arm in ["accept", "reject", "warn", "trap"] {
        render_corpus_table(&mut out, repository_root, arm)?;
    }
    Ok(out)
}

fn render_code_entry(
    out: &mut String,
    repository_root: &Path,
    arm: &str,
    code: &str,
    explanation: &str,
    pin: &Pin,
) -> io::Result<()> {
    let relative = format!("corpus/{arm}/{}", pin.file);
    let path = repository_root.join(&relative);
    let source = read(&path)?;
    let header = parse_header(&path, &source)?;
    let expected_name = format!("{arm}/{}", pin.file.trim_end_matches(".ts"));
    if header.corpus != expected_name {
        return Err(invalid(format!(
            "{}: corpus header `{}` does not match `{expected_name}`",
            path.display(),
            header.corpus
        )));
    }
    let lines = source.lines().collect::<Vec<_>>();
    if pin.line == 0 || pin.line > lines.len() {
        return Err(invalid(format!(
            "{}:{}: harness pin is outside the {}-line file",
            path.display(),
            pin.line,
            lines.len()
        )));
    }
    let start = pin.line.saturating_sub(3);
    let end = usize::min(pin.line + 2, lines.len());

    writeln!(out, "\n### {code}").expect("write to String");
    writeln!(out, "\n{explanation}").expect("write to String");
    writeln!(
        out,
        "\nPinned corpus: [`{relative}`](../{relative}), line {}.",
        pin.line
    )
    .expect("write to String");

    let guidance = header.guidance().collect::<Vec<_>>();
    if !guidance.is_empty() {
        writeln!(out, "\nHeader guidance:\n\n```text").expect("write to String");
        for field in guidance {
            writeln!(out, "// {}: {}", field.key, field.value).expect("write to String");
        }
        writeln!(out, "```").expect("write to String");
    }

    writeln!(out, "\n```ts").expect("write to String");
    for line in &lines[start..end] {
        writeln!(out, "{line}").expect("write to String");
    }
    writeln!(out, "```").expect("write to String");
    Ok(())
}

fn render_corpus_table(out: &mut String, repository_root: &Path, arm: &str) -> io::Result<()> {
    let arm_dir = repository_root.join("corpus").join(arm);
    let mut paths = Vec::new();
    collect_typescript_files(&arm_dir, &mut paths)?;
    paths.sort_by_key(|path| {
        crate::repository_relative(repository_root, path).expect("path below repository root")
    });
    if paths.is_empty() {
        return Err(invalid(format!(
            "{} has no TypeScript corpus entries",
            arm_dir.display()
        )));
    }

    let title = match arm {
        "accept" => "Accept",
        "reject" => "Reject",
        "warn" => "Warn",
        "trap" => "Trap",
        _ => return Err(invalid(format!("unknown corpus arm {arm}"))),
    };
    writeln!(out, "\n## {title}").expect("write to String");
    let has_expected = matches!(arm, "accept" | "trap");
    if arm == "accept" {
        writeln!(
            out,
            "\n| Entry | Purpose | Exercises | Questions | Expected output | Interpreter |\n|---|---|---|---|---|---|"
        )
        .expect("write to String");
    } else if has_expected {
        writeln!(
            out,
            "\n| Entry | Purpose | Exercises | Questions | Expected output |\n|---|---|---|---|---|"
        )
        .expect("write to String");
    } else {
        writeln!(
            out,
            "\n| Entry | Purpose | Exercises | Questions |\n|---|---|---|---|"
        )
        .expect("write to String");
    }

    for path in paths {
        let relative = crate::repository_relative(repository_root, &path)
            .ok_or_else(|| invalid(format!("path outside repository root: {}", path.display())))?;
        let source = read(&path)?;
        let header = parse_header(&path, &source)?;
        let expected_corpus_name = relative
            .strip_prefix("corpus/")
            .and_then(|value| value.strip_suffix(".ts"))
            .ok_or_else(|| invalid(format!("invalid corpus source path {relative}")))?;
        if header.corpus != expected_corpus_name {
            return Err(invalid(format!(
                "{}: corpus header `{}` does not match `{expected_corpus_name}`",
                path.display(),
                header.corpus
            )));
        }
        write!(
            out,
            "| [`{}`](../{}) | {} | {} | {} |",
            escape_table(&header.corpus),
            relative,
            escape_table(&header.purpose),
            escape_table(&header.exercises),
            escape_table(&header.questions),
        )
        .expect("write to String");
        if has_expected {
            let expected = expected_path(&arm_dir, &path)?;
            let expected_relative = crate::repository_relative(repository_root, &expected)
                .ok_or_else(|| {
                    invalid(format!(
                        "path outside repository root: {}",
                        expected.display()
                    ))
                })?;
            write!(
                out,
                " [`{}`](../{}) |",
                escape_table(&expected_relative),
                expected_relative
            )
            .expect("write to String");
        }
        if arm == "accept" {
            let interpreter = match &header.interpreter_exclusion {
                Some(reason) => format!("no — {reason}"),
                None => "yes".to_string(),
            };
            write!(out, " {}", escape_table(&interpreter)).expect("write to String");
            if header.benchmark {
                write!(out, "; cost: benchmark").expect("write to String");
            }
            write!(out, " |").expect("write to String");
        }
        writeln!(out).expect("write to String");
    }
    Ok(())
}

fn reject_pins(repository_root: &Path) -> io::Result<Vec<Pin>> {
    let path = repository_root.join("compiler/tests/corpus_reject.rs");
    let source = read(&path)?;
    let mut pins = parse_pin_table(&path, &source, "EXPECTED", "RuleCode::")?;
    pins.extend(parse_pin_table(
        &path,
        &source,
        "REGEX_EXPECTED",
        "RuleCode::",
    )?);
    Ok(pins)
}

fn warn_pins(repository_root: &Path) -> io::Result<Vec<Pin>> {
    let path = repository_root.join("compiler/tests/corpus_warn.rs");
    let source = read(&path)?;
    parse_pin_table(&path, &source, "EXPECTED", "WarnCode::")
}

fn parse_pin_table(
    path: &Path,
    source: &str,
    table_name: &str,
    code_prefix: &str,
) -> io::Result<Vec<Pin>> {
    let declaration = format!("const {table_name}:");
    let declaration_start = source.find(&declaration).ok_or_else(|| {
        invalid(format!(
            "{}: missing pin table declaration `{declaration}`",
            path.display()
        ))
    })?;
    let after_declaration = &source[declaration_start..];
    let array_start = after_declaration.find("= &[").ok_or_else(|| {
        invalid(format!(
            "{}: malformed pin table `{table_name}`",
            path.display()
        ))
    })? + 4;
    let after_start = &after_declaration[array_start..];
    let array_end = after_start.find("]; ").or_else(|| after_start.find("];\n"));
    let array_end = array_end
        .or_else(|| after_start.find("];"))
        .ok_or_else(|| {
            invalid(format!(
                "{}: unterminated pin table `{table_name}`",
                path.display()
            ))
        })?;
    let table = &after_start[..array_end];

    let mut pins = Vec::new();
    let mut cursor = 0;
    while let Some(quote_offset) = table[cursor..].find('"') {
        let file_start = cursor + quote_offset + 1;
        let file_end_offset = table[file_start..].find('"').ok_or_else(|| {
            invalid(format!(
                "{}: unterminated filename in `{table_name}`",
                path.display()
            ))
        })?;
        let file_end = file_start + file_end_offset;
        let file = &table[file_start..file_end];
        cursor = file_end + 1;
        if !file.ends_with(".ts") {
            continue;
        }

        let code_start_offset = table[cursor..].find(code_prefix).ok_or_else(|| {
            invalid(format!(
                "{}: `{table_name}` entry for {file} has no {code_prefix} code",
                path.display()
            ))
        })?;
        let code_start = cursor + code_start_offset + code_prefix.len();
        let code_len = table[code_start..]
            .chars()
            .take_while(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        if code_len == 0 {
            return Err(invalid(format!(
                "{}: `{table_name}` entry for {file} has an empty code",
                path.display()
            )));
        }
        let code_end = code_start + code_len;
        let after_code = &table[code_end..];
        let comma = after_code.find(',').ok_or_else(|| {
            invalid(format!(
                "{}: `{table_name}` entry for {file} has no pinned line",
                path.display()
            ))
        })?;
        let after_comma = &after_code[comma + 1..];
        let line_text = after_comma.trim_start();
        let line_len = line_text
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        let line = line_text[..line_len].parse::<usize>().map_err(|_| {
            invalid(format!(
                "{}: `{table_name}` entry for {file} has an invalid pinned line",
                path.display()
            ))
        })?;
        pins.push(Pin {
            file: file.to_string(),
            code: table[code_start..code_end].to_string(),
            line,
        });
        cursor = code_end + comma + 1 + (after_comma.len() - line_text.len()) + line_len;
    }

    if pins.is_empty() {
        return Err(invalid(format!(
            "{}: pin table `{table_name}` is empty",
            path.display()
        )));
    }
    Ok(pins)
}

/// Reads the structured header and its optional interpreter and cost facts.
///
/// # Errors
///
/// Fails for missing required fields or malformed execution facts. Execution errors name the source line.
pub fn parse_header(path: &Path, source: &str) -> io::Result<CorpusHeader> {
    let mut fields: Vec<HeaderField> = Vec::new();
    let mut current: Option<usize> = None;

    for (line_index, line) in source.lines().enumerate() {
        let Some(comment) = line.strip_prefix("//") else {
            break;
        };
        let content = comment.strip_prefix(' ').unwrap_or(comment);
        if content.starts_with(char::is_whitespace) {
            append_continuation(&mut fields, current, content.trim());
            continue;
        }
        let mut parsed_field = false;
        for segment in content.split(" // ") {
            if matches!(
                segment.split_whitespace().next(),
                Some("interpreter" | "cost")
            ) {
                return Err(invalid(format!(
                    "{}:{}: malformed execution header key",
                    path.display(),
                    line_index + 1
                )));
            }
            if let Some((key, value)) = segment.split_once(':') {
                let key = key.trim();
                if !key.is_empty() {
                    fields.push(HeaderField {
                        key: key.to_string(),
                        value: value.trim().to_string(),
                        line: line_index + 1,
                    });
                    current = Some(fields.len() - 1);
                    parsed_field = true;
                }
            }
        }
        if parsed_field {
            continue;
        }
        append_continuation(&mut fields, current, content.trim());
    }

    let required = |name: &str| -> io::Result<String> {
        let matches = fields
            .iter()
            .filter(|field| field.key == name)
            .collect::<Vec<_>>();
        if matches.len() != 1 || matches[0].value.trim().is_empty() {
            return Err(invalid(format!(
                "{}: expected exactly one non-empty `// {name}:` header field",
                path.display()
            )));
        }
        Ok(matches[0].value.clone())
    };

    let mut interpreter_exclusion = None;
    let mut benchmark = false;
    for field in &fields {
        let malformed = || {
            invalid(format!(
                "{}:{}: malformed or duplicate `// {}:` header field",
                path.display(),
                field.line,
                field.key
            ))
        };
        match field.key.as_str() {
            "interpreter" => {
                let reason = field.value.strip_prefix("no — ").ok_or_else(malformed)?;
                if reason.trim().is_empty() || interpreter_exclusion.is_some() {
                    return Err(malformed());
                }
                interpreter_exclusion = Some(reason.to_string());
            }
            "cost" => {
                if field.value != "benchmark" || benchmark {
                    return Err(malformed());
                }
                benchmark = true;
            }
            _ => {}
        }
    }

    Ok(CorpusHeader {
        interpreter_exclusion,
        benchmark,
        corpus: required("corpus")?,
        purpose: required("purpose")?,
        exercises: required("exercises")?,
        questions: required("questions")?,
        fields,
    })
}

fn append_continuation(fields: &mut [HeaderField], current: Option<usize>, value: &str) {
    if value.is_empty() {
        return;
    }
    if let Some(field) = current.and_then(|index| fields.get_mut(index)) {
        if !field.value.is_empty() {
            field.value.push(' ');
        }
        field.value.push_str(value);
    }
}

fn collect_typescript_files(directory: &Path, output: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("read {}: {error}", directory.display()),
        )
    })? {
        let entry = entry.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("read {} entry: {error}", directory.display()),
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_typescript_files(&path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("ts") {
            output.push(path);
        }
    }
    Ok(())
}

fn expected_path(arm_dir: &Path, source_path: &Path) -> io::Result<PathBuf> {
    let beside_source = source_path.with_extension("expected");
    if beside_source.is_file() {
        return Ok(beside_source);
    }
    let relative = source_path.strip_prefix(arm_dir).map_err(|_| {
        invalid(format!(
            "{} is not beneath {}",
            source_path.display(),
            arm_dir.display()
        ))
    })?;
    let entry_name = relative
        .components()
        .next()
        .ok_or_else(|| invalid(format!("invalid corpus path {}", source_path.display())))?
        .as_os_str();
    let shared = arm_dir.join(entry_name).with_extension("expected");
    if shared.is_file() {
        return Ok(shared);
    }
    Err(invalid(format!(
        "{}: no expected-output file beside the source or entry directory",
        source_path.display()
    )))
}

fn escape_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn read(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
        .map_err(|error| io::Error::new(error.kind(), format!("read {}: {error}", path.display())))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_headers_parse_facts_and_report_malformed_lines() {
        let source = "// corpus: accept/test\n// purpose: Test headers.\n// exercises: headers\n// questions: none\n";
        let path = Path::new("test.ts");
        let header = parse_header(
            path,
            &format!("{source}// interpreter: no — needs a host\n// cost: benchmark\n"),
        )
        .expect("parse execution facts");
        assert_eq!(
            header.interpreter_exclusion.as_deref(),
            Some("needs a host")
        );
        assert!(header.benchmark);
        let defaults = parse_header(path, source).expect("parse optional defaults");
        assert_eq!(defaults.interpreter_exclusion, None);
        assert!(!defaults.benchmark);
        let continuation = parse_header(
            path,
            "// corpus: accept/test\n// purpose: Test headers.\n// costly operations need a host.\n// exercises: headers\n// questions: none\n",
        )
        .expect("parse a continuation whose first word is not an execution key");
        assert_eq!(
            continuation.purpose,
            "Test headers. costly operations need a host."
        );
        for malformed in [
            "interpreter: yes",
            "interpreter: no —",
            "interpreter no — needs a host",
            "cost: cheap",
            "cost benchmark",
        ] {
            let error = parse_header(path, &format!("{source}// {malformed}\n"))
                .expect_err("reject malformed execution fact");
            assert!(error.to_string().contains("test.ts:5:"), "{error}");
        }
        for duplicate in ["interpreter: no — needs a host", "cost: benchmark"] {
            let error = parse_header(path, &format!("{source}// {duplicate}\n// {duplicate}\n"))
                .expect_err("reject duplicate execution fact");
            assert!(error.to_string().contains("test.ts:6:"), "{error}");
        }
    }

    #[test]
    fn generated_ai_references_are_byte_identical() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let generated = root.join("generated-docs");
        let language = render_language_reference(&root).expect("render language reference");
        let corpus = render_corpus_index(&root).expect("render corpus index");
        let committed_language =
            fs::read(generated.join("language-reference.md")).expect("read language reference");
        let committed_corpus =
            fs::read(generated.join("corpus-index.md")).expect("read corpus index");
        assert_eq!(language.as_bytes(), committed_language.as_slice());
        assert_eq!(corpus.as_bytes(), committed_corpus.as_slice());
    }
}
