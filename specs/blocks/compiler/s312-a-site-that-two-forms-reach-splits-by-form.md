<!-- §312 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 312. A site that two forms reach splits by form

*(Added 2026-11-16.)* Origin: §311.4 item 1. The owner selected a
measurement of the multi-witness sites on 2026-11-16.

Problem, measured at `66ec3574` by a review subagent. 264 `Diverges`
sites have two or more witness files. 119 of them have two or more
witnesses that `tsc` accepts. The review ran each of the 546 accepted
witnesses through the debug CLI and compared the example that it shows
with the form of the witness. At 18 sites, a realistic form shows the
example of another form. Example: a lambda that captures a parameter
reaches `MutableLocalCapture` and shows a `let x` to `Cell` example.
The total check of §311 rule 4 reads the example fragment only, so it
cannot find these sites (§311.4 item 1).

A second defect: `import("./other")` in a file that has no static
import of `./other` reaches `NonExpressionCalleeUnavailable` in the
CLI, because the program does not load the target. That site is
`TscRejects` and shows no example. With a static import of the same
module, the CLI shows the `DynamicImportCall` block. `tsc` accepts the
file in a project.

### 312.1 Rules

1. Each site of table 312.1a splits by §311 rule 7. The site that the
   form of column 2 reaches shows an example of that form. The
   `subscript` fragment of column 4 is the intent; the coding agent can
   change the text if the checker or `tsc` rejects it. The total check
   of §311 rule 4 applies to each new site.
2. A dynamic `import(…)` call reaches the `DynamicImportCall` site
   whether the program loads the target or not, and also as the
   operand of `await`. *(Amended 2026-11-16 after the Phase Review:
   `const m = await import("./other")` showed no example.)*
2a. Each generic alias that the source declares, and that does not
   shadow a lib name, reaches the site of row `GenericSourceAlias`.
   Its example declares an alias with no type parameter:
   `type IntList = i32[];`. *(Added 2026-11-16 after the Phase Review:
   the split selected a string-union body only, so `type List<T> =
   T[]` showed the lib-shadow example.)*
3. A test closes the class for new witnesses. It derives, for each
   `Diverges` site, the set of witnesses that `tsc` accepts, from the
   witness index and the recorded `tsc` codes. A site with two or more
   such witnesses must be in a list of reviewed sites, with the list of
   its accepted witnesses. If the derived set and the list differ, the
   test fails and names the site. Each listed witness was read against
   the example of its site. The list holds the sites of this
   measurement after the split.
4. No program changes from accepted to rejected or back. No message
   line or position moves, except the example block of a split site.

Table 312.1a. Sites that split.

| Site | Form that does not fit | Example shown at the pin | Example for the form |
|---|---|---|---|
| MutableLocalCapture | a lambda captures a parameter | `let x` to a `Cell` class | `const v = param; () => v` |
| BlockPendingReadWithoutProgramShadow | a closure reads a later local, with no outer name | an outer shadow | declare the local before the closure |
| FunctionBodyMissing | `declare function f(): void;` in a source file | an overload pair | the declaration in a mirror `.d.ts` |
| DefaultExportDeclaration | `export * from "./other"` | a default function to a named one | `export { n } from "./other";` |
| GenericSourceAlias | a user alias `type List<T> = T[]` or `type A<T> = "a" \| "b"` | a lib shadow `type Array<T, U>` | an alias with no type parameter (rule 2a) |
| MirrorModuleDeclaration | `declare module "x" { … }` in a mirror | `declare namespace` | `declare const` / `declare function` at the top level of the mirror |
| UnknownNamespaceConstructor | `new WeakMap<K, V>()`, `new WeakSet<K>()`, `new WeakRef<K>(…)` | `new Object()` | `new Map<K, V>()` |
| ArrayElementDomain | `xs.includes(v)` on a `@ValueType` array | a generator `forEach` | a `for…of` loop that compares fields |
| CallbackFewerParametersUnadapted | a parameter function value passed to `forEach` | `let f` to `const f` | `const f = fv; xs.forEach(f);` |
| FunctionValueAdapterNeedsConstName | a parameter function value passed to a wider callback place | `let f` to `const f` | `const f = fv; run(f);` |
| VoidPlaceDropsAsyncHandle | `xs.forEach(async (x) => …)` | a change of the declared callback type | `for (const x of xs) { await work(x); }` |
| ErasedAssignableTypeMismatch | `const y: E = n` with `n: i32` | an array `map` conversion | a `switch` that gives a member |
| PromiseReactionResult | `f().finally(asyncG)` | a `.catch` result type | `try { await f(); } finally { await g(); }` |
| PromiseReactionParameter | `.catch((e: string) => …)` | a nullable `then` parameter | `.catch((e: Error): i32 => 0)` |
| ErrorMemberOutsideSurface | a write `e.cause = …` | a read of `e.message` | the cause in a binding of its own |
| TypeofOperator | `const t = typeof 1` | a `typeof x === "string"` guard | a string literal |
| FileModuleImportForm | `export { writeFile } from "node:fs/promises"` | a namespace import to a named import | a named import and an exported wrapper function |
| TextModuleImportForm | the same re-export form for the text module | the same | the same |

### 312.2 Acceptance

1. Red first: at the pin, a test runs one witness of column 2 for each
   row of table 312.1a and fails, because the shown example is the
   example of column 3. The test of rule 2 fails at the pin for the
   CLI path that does not load the target.
2. After the change, these tests pass, and the total check of §311
   rule 4 passes.
3. The test of rule 3 fails if a witness that `tsc` accepts is added to
   a listed site, or a site gets a second accepted witness, and the
   list does not change. A negative control in the test shows this.
4. The test of rule 3 states its cost, measured with
   `--test-threads=1`.

### 312.3 Phase Review

At the contract pin, the table test fails for the 18 rows, and the
rule 2 test fails for the path that does not load the target. After
the change, both pass. The test of rule 3 holds 128 reviewed sites and
costs 7.7 ms (`--test-threads=1`); its negative control fails as
stated. The review compared the CLI output of 923 `check` runs over
`corpus/accept` and `corpus/reject` with a pin build: 910 are
byte-identical, and in the other 13 only the example block differs.
The count of accepted files and of errors does not change.

One fresh review found one MAJOR finding, realistic, and six MINOR
findings.

1. MAJOR: the split selected a generic alias with a string-union body
   only, so `type List<T> = T[]` showed the lib-shadow example. Rule 2a
   is added.
2. MINOR, fixed: `new WeakSet<K>()` and `new WeakRef<K>(…)` showed the
   `new Object()` example; `await import("./other")` showed no example
   (older than §312; rule 2 is amended).
3. MINOR, recorded in 312.4: `r510`, and three contrived forms.

### 312.4 Open

1. Contrived forms that show the example of another form, measured at
   `66ec3574`: `===` on function values (ErasedAssignableEquality),
   `Promise<void, i32>` (VoidTypeOutsideResult), `g().toString()` on a
   generator (CoroutineReturnOrThrowCall), `const held = Math`
   (FormMathUsedAsAValue), a default parameter in a `then` callback
   (PromiseReactionParameter), and the forms of witnesses `a-s191`,
   `a-s058`, `a-s081`, `a-s116`.
2. Two local lambdas that call each other, with a statement between
   them (`r510`), show "declare the local before the closure". The
   form that works puts the lambdas next to each other. Realistic and
   rare; the pin showed an outer-shadow example.
3. Contrived, from the Phase Review: an `Inbox` parameter captured in a
   closure gets a follow-on `ParameterCapture` block after the
   `ContextAffineCapture` diagnostic; `e.stack = "s"` shows the
   `e.cause` example; a user method named `forEach` selects the array
   callback site by name.
