<!-- §194 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 194. A JSON mismatch names its path

*(Added 2026-10-23.)* Origin: a review of the parse diagnostics of
scriptc (<https://github.com/vercel-labs/scriptc>), whose checked cast
reports `expected number at $.port, got string` *(docs)*. The owner
selected the change on 2026-10-23. The measurement is in
`specs/tracking/json-mismatch-path.md`.

Problem, measured at the pin `3231e6d7`: §115.7 rule 5 gives one
message for each target type. Four different failures of one `Person`
document print the same line:

    nested range: JSON.parse: document does not match Person
    nested kind: JSON.parse: document does not match Person
    nested missing: JSON.parse: document does not match Person
    root kind: JSON.parse: document does not match Person

The golden of `a255` shows the same fact: `missing field` and
`wrong kind` print the same text. JSON reaches a script as a save
file, a config, or a message (stdlib §13.4). For these inputs, a
mismatch is an honest mistake, and invariant 6 asks for a clear error.
The message names the type, but it does not name the location.

Code facts at the pin: the checker synthesizes one validator for each
type that the target reaches (`compiler/src/check/json.rs`,
`synthesize_json_parser`). A validator returns `bool` and keeps no
record of where it failed. The runtime document (`runtime/src/json.rs`,
`JsonDocument`) stores the nodes in a flat list with no parent links.
The three engines call the same runtime functions for each `JsonFn`.

### 194.1 The message

1. The `TypeError` message of §115.7 rule 5 becomes:

       JSON.parse: document does not match <T> at <path>: expected <U>, found <F>

   `<T>` is the target type as the source spells it, as today. `<path>`
   is the location of the first failure (rule 3). `<U>` is the declared
   type of that location. `<F>` is what the document holds there.
2. **The path.** `$` is the root. A class field adds `.` and the field
   name as the class declares it. An array element adds `[` and the
   decimal index and `]`. The depth limit (128) bounds the path, so the
   message has no truncation.
3. **The first failure.** Validation is depth first. It visits fields
   in declaration order and elements in index order. The message reports
   the first location that fails in that order. A document with two bad
   fields reports the field that the class declares first.
4. **`<U>`** is the type of the location, spelled as `type_name` spells
   it in diagnostics. The type of a field is its declared type. A
   nullable field gives `Person | null`, not `Person`.
5. **`<F>`** is one of these texts:

   | The document holds | `<F>` |
   |---|---|
   | a value of the wrong kind | `null`, `boolean`, `number`, `string`, `array`, or `object` |
   | no member for a declared field | `no field` |
   | a number outside the range of an integer target, or a finite number that overflows `f32` | `a number out of range` |
   | a number with a fraction for an integer target | `a non-integer number` |
   | a string with a lone surrogate | `a string with a lone surrogate` |
   | an array with the wrong length for a `FixedArray` | `an array of length <N>` |

   A number that overflows `f64` (`1e400`) gives `a number out of
   range`, because stdlib §13.3 rejects an infinity.
6. The messages are this project's (§115.7 rule 6). An entry that
   prints one is not `js-comparable` (C6).

### 194.2 The form

1. **A validator records the failure where it fails.** When a check of
   a validator fails, the validator calls the runtime before it returns
   `false`. The leaf record carries the node, the label `<U>`, and the
   numeric target for a numeric check. A missing field records the
   field name with no node.
2. **A parent adds its segment on the way out.** A class validator
   records `.name` when the validator of a field returns `false`. An
   array validator records `[i]` when the validator of an element
   returns `false`. The runtime stores the segments innermost first and
   reverses them when it builds the path.
3. **A nullable validator relabels its own node.** When the inner
   validator returns `false` and the leaf record holds the node of the
   nullable validator, the label becomes the nullable type (rule 4).
4. **The success path executes no record call.** Each record call is
   on a branch that returns `false`. A successful parse runs the same
   runtime calls as at the pin.
5. **The record lives in the Context's parser state, not in the
   document.** `ParseBegin` clears it. The root wrapper builds the
   message before `ParseEnd`, then releases the document, then raises.
   §115.7 rule 7 does not change: no partial value is visible.
6. **The runtime builds `<F>` and the path.** `<F>` comes from the node
   and the numeric target. The message is a Context-allocated string,
   as the `SyntaxError` message of §115.7 rule 3 is.
7. **Every tier.** Each new `JsonFn` member maps to one runtime
   function. The dev tier, the ship tier, and the interpreter call
   that function and give the same output.

### 194.3 What moves

- §115.7 rule 5: the message text. The failure classes do not change.
- The goldens `a255` and `e05`, the two witnesses in
  `compiler/src/api_reference.rs`, `generated-docs/api-reference.md`
  (regenerated, never edited), and `docs/tutorial-typescript.md`.
- `compiler/tests/exceptions.rs`: the root wrapper no longer holds the
  whole message as a literal, so the assertion reads the new form.
- The LIR text snapshot: each module declares the five new runtime
  functions. The snapshot holds no validator body, so it shows no
  record call. *(Corrected at the landing; the first text predicted
  record calls in the snapshot.)*

Any other moved golden is a stop.

### 194.4 Corpus (pre-registered)

- `a255-json-parse-direct`: the golden moves. The new lines are:

      depth 128: TypeError: JSON.parse: document does not match i32[] at $[0]: expected i32, found array
      missing field: TypeError: JSON.parse: document does not match Config at $.count: expected i32, found no field
      wrong kind: TypeError: JSON.parse: document does not match Config at $.count: expected i32, found string
      contextual target: TypeError: JSON.parse: document does not match Config at $: expected Config, found array
      element: TypeError: JSON.parse: document does not match i32[] at $[2]: expected i32, found string
      out of range: TypeError: JSON.parse: document does not match u8 at $: expected u8, found a number out of range
      not an integer: TypeError: JSON.parse: document does not match i32 at $: expected i32, found a non-integer number
      f32 overflow: TypeError: JSON.parse: document does not match f32 at $: expected f32, found a number out of range
      lone surrogate: TypeError: JSON.parse: document does not match string at $: expected string, found a string with a lone surrogate
      fixed length: TypeError: JSON.parse: document does not match FixedArray<i16, 3> at $: expected FixedArray<i16, 3>, found an array of length 2

- `a372-json-mismatch-path`, with a hand-checked golden: a nested
  number out of range (`$.friend.age`), a nullable field of the wrong
  kind (`$.friend`, `expected Person | null, found number`), a nested
  missing field (`$.friend.age`, `found no field`), an element of an
  array of classes (`$[1].name`), a `FixedArray` field inside a class,
  two bad fields (the field declared first is reported), a duplicate
  key (the last occurrence is the one that fails), and a successful
  parse after each failure. `js-comparable: no`, with C6.

### 194.5 Exit criteria

1. `a255` and `a372` fail at `3231e6d7` and pass after the landing on
   the dev JIT, the ship C, the interpreter, and the golden,
   byte-exact.
2. `tsc` exits 0 over both entries.
3. A compiler test walks the synthesized validators of every family. It
   finds each record call in a branch that ends in `return false`. A
   runtime unit test finds that `ParseBegin` clears an earlier record.
   A second unit test builds each `<F>` class and each path segment
   kind directly in the runtime. *(Corrected at the landing: a runtime
   test of a valid document cannot fail, because no validator runs.)*
4. Success-path cost on the ship tier: a loop of 200,000 successful
   `JSON.parse<Config>` calls, measured at the pin and after. If the
   median is more than 5% slower, the landing stops and reports. Both
   numbers go in the tracking note, with the cost of the new tests.
5. `tools/gate.sh full` is green.
6. The Phase Review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
