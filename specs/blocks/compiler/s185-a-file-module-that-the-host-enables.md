<!-- §185 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 185. A file module that the host enables

*(Added 2026-10-09.)* Origin: the owner's revision of the
standalone-runtime Non-goal on 2026-10-09 (`CLAUDE.md`,
`specs/rules-history.md`). On 2026-10-09 the owner selected this
section and decided: the host enables the module; the host does the
I/O; files only; a build without the module rejects its use, and a run
without a host provider completes the call with an `Error`; the four
whole-file operations first; the Node.js `fs/promises` shape, under
the provisional default of `stdlib.md` §20. The measurement round at
`e71340a0` is `specs/tracking/s185-file-module-measurement.md`.

Problem: a script cannot read or write a file. A host must bind its own
C functions for each project, with its own names and result forms.

Shapes compared (2026-10-09; `tsc` 5.9.2 `lib.dom.d.ts`, `node`
v24.18.0):

| Shape | Fit |
|---|---|
| Node.js `fs/promises` (`readFile`, `writeFile`) | path-based, as applications address files; returns a `Promise`; runs under `node` |
| File System Access (`navigator.storage.getDirectory()`, handles, writable streams) | an options object literal (S005), a union write argument (S011), a `File`/`Blob`/`WritableStream` hierarchy, and a global `navigator`; `navigator.storage` is `undefined` in `node`, so no divergence detector runs it |
| File/Blob (`file.text()`) | reads a `File` that something else supplies; no path |

### 185.1 Rules

1. **Surface.** `import { readFile, writeFile } from "node:fs/promises"`
   is accepted, with these forms only:
   - `readFile(path: string, encoding: "utf8"): Promise<string>`
   - `readFile(path: string): Promise<u8[]>`
   - `writeFile(path: string, data: string): Promise<void>`
   - `writeFile(path: string, data: u8[]): Promise<void>`

   Another member of the module, another argument form (an options
   object, another encoding), and a default or namespace import are
   rejected with a diagnostic that names the accepted forms. The
   prelude declares the module for `tsc`. A call is an async origin
   (§178 rule 1): `await readFile(…)` is accepted directly.
2. **The build enables the module.** A build option names each
   enabled standard module: `--enable-module node:fs/promises` on the
   CLI, and the same name in the Rust build options. A program that
   imports `node:fs/promises` in a build that does not enable it is
   rejected with a diagnostic that names the option.
3. **The host provides the I/O.** A C struct `subscript_rt_file_provider`
   holds a size field (for later extension), a userdata pointer, a read
   callback, and a write callback. The host sets it on a Context with
   `subscript_rt_ctx_set_file_provider` before the module initializer
   runs; the Rust host API gives the same option before initialization.
   A call creates a §178 source (kind 4) and calls the callback with
   the UTF-8 path bytes and length, the result form (text or bytes),
   the write data and length, and the endpoint. The callback returns at
   once; the path and the write data are valid only during the
   callback, so a host that completes later copies them first.
3a. **The form carries the operation.** The checker resolves each
   accepted call to one standard host operation: read text, read
   bytes, write text, or write bytes. The HIR carries it as a standard
   callee, not as a foreign symbol. The LIR `HostCompletion` names its
   target: a foreign function (§178) or a standard operation; the
   verifier checks the result kind (§184 rule 3) against the
   operation. Both tiers lower a standard operation to one runtime
   entry, after the source is created. The entry reads the Context's
   provider and calls its callback with the operation and its
   arguments; with no provider, it completes the source with the rule
   5 `Error`. No C mirror and no symbol name stands for the module.
4. **Completion.** The host completes each request once, on the Context
   owner thread, with `subscript_rt_complete_string`,
   `subscript_rt_complete_bytes`, `subscript_rt_complete_void`, or
   `subscript_rt_complete_error` (§178, §184). If a completion returns
   `INVALID_UTF8` or `TOO_LARGE`, the source stays pending and the host
   completes it with an `Error`. A request that the host never
   completes stays pending; `subscript_rt_ctx_async_unfinished` counts
   it (§178 rule 11).
5. **No provider.** A call on a Context with no provider completes its
   source with an `Error` whose message names the missing provider.
   The script can catch it. A Worker Context has no provider: the host
   provider runs on the Context owner thread, and a Worker runs on its
   own thread.
5a. **A rejected argument that `tsc` accepts carries its divergence.**
   The prelude declares `u8` as `number`, so `tsc` accepts a number
   array for `data`; it also accepts a string-literal alias for `path`
   and a `string` value for `encoding`. Each such argument is rejected
   with a divergence block that shows the accepted form (§154); an
   argument that `tsc` rejects too has no block.
6. **Path.** The host interprets the path: the runtime does not
   normalize it, resolve it, or choose a directory. The path is the
   script string's UTF-8 bytes with their length; a host that cannot
   represent it completes the request with an `Error`.
7. **Hot reload.** A host source keeps its identity across a reload,
   and a script frame created after the reload can await a source
   created before it. A waiting frame of an old generation follows the
   stale frame rule (§178 rule 10). The measurement found that a new
   frame traps `StaleCoroutine` (12) on a pre-reload source, because
   the stale test does not separate a source with no code from a frame;
   this section separates them.
8. **Tiers.** The dev JIT and C AOT give the same output and traps.
   The interpreter rejects a file call as unsupported, as it rejects
   any host call (§68.7); an entry that reads a file carries
   `interpreter: no`.
9. **Divergences from `node`.** Each is a collision row: the bytes
   result is `u8[]`, not `Buffer`; an `Error` from the host carries a
   message and no `code` (`ENOENT` and so on); a text read of bytes
   that are not UTF-8 is an `Error`, where `node` replaces them with
   U+FFFD; the forms of rule 1 are the only ones.

### 185.2 Acceptance

1. Red first, at the contract pin: accept entry `a355` reads and writes
   text and bytes through a test provider (a missing file, a caught
   `Error`, a `Context.collect()` while a read is pending); reject
   entries for a build without the option, an options-object argument,
   and another module member; a trap or runtime test for the
   no-provider `Error`. `js-comparable` follows the collision rows of
   rule 9.
2. Unit tests: the provider callback receives the path and data bytes
   (built in the test, core principle 9); a provider that completes
   inside the callback and one that completes later; each status of
   rule 4 with the source still pending; a reload test for rule 7 with
   a firing control (an old waiting frame still traps).
3. Cost: record one 1 MiB read and write against the measurement.
4. Goldens: the LIR text golden for `a355`, the generated docs, the
   TypeScript and C tutorials. No other `.expected` output moves.

### 185.3 Sections this one amends

- §178 rule 10: a frame created after a reload can await a source
  created before it.
- `CLAUDE.md` Non-goal "Being a standalone program runtime" (revised
  2026-10-09): this is the first module under it.

### 185.4 Open

1. **A path or encoding of another string type.** A path whose type
   is a string-literal union alias (`type Name = "a.txt" | "b.txt"`)
   is rejected (rule 5a); accepting it needs the alias's runtime
   string. An encoding argument held in a `string` variable shows the
   divergence block, and `tsc` gives TS2345 for it, because the
   checker has no literal string type.
2. **No `node` run of the module.** `a355` is `js-comparable: no`
   (C25): its error lines differ from `node`, and the gate's `node`
   runner gives no scratch directory. The read and write path is not
   compared with `node` yet.
