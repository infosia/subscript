# subscript for Rust embedders

The compiler and runtime are Rust crates, and the `subscript` CLI is
itself a Rust host built on them. A Rust application embeds the
language in-process through the same crate surface the repository's
own gates consume. One thing follows that a C host does not get: the
**development tier lives in your process**. Scripts JIT-compile,
run, and hot-reload inside your application, with no C compiler in
the run path and no generated artifacts on disk.

Read this first, plainly:

- The crates are not published. `subscript-codegen`,
  `subscript-runtime`, and `subscript-cli` set `publish = false`;
  `subscript-compiler` depends on two git forks. Embed through a
  path dependency, or through a git dependency pinned to a commit.
  There is no semver contract yet. The **contracted** host boundary
  remains the C ABI. The Rust surface carries `#![warn(missing_docs)]`
  and a unit test for every public item (core principle 1), but it is
  the same surface the CLI consumes, not a frozen API.
- Script-visible *host data* still crosses a C facade in a Rust
  host: scripts bind C headers (`extern "C"` functions, `#[repr(C)]`
  structs, a mirror from `subscript bind`), and
  `ReloadSession::new_with_native_libraries` links them. The
  [C/C++ tutorial](tutorial-c-cpp.md) covers that side; this one
  covers the pure-Rust part.
- "No C compiler in the run path" refers to execution. A cold `cargo
  build` still compiles a little C from a transitive dependency
  (`psm`, under the parser's `stacker`), like most Rust projects.

The four-step walkthrough below is the committed, test-pinned example
[`examples/rust-host/`](../examples/rust-host/). Every other output
in this document comes from a run against the repository as
committed.

## The crates

| Crate | What you use it for |
|---|---|
| `subscript-compiler` | `SourceFile`, `check_program` (accept or reject, with `Diagnostic`s), `check_warnings`, `render_diagnostics` / `render_warnings` (the CLI's exact output shape), `parse_import_specifiers` |
| `subscript-codegen` | `ReloadSession` (a live, hot-reloadable program), `run_jit` (one shot), `emit_c_files` (the ship tier's C), `NativeLibrary`, and the error and trap types |
| `subscript-runtime` | The `Context` itself and `TrapKind`. `subscript-codegen` links it; you name it only to match on `TrapReport.rule` |

Two dependencies are enough for a host, and the example declares
exactly those:

```toml
[dependencies]
subscript-codegen = { path = "../../codegen" }
subscript-compiler = { path = "../../compiler" }
```

## Building the source set

A program is a `Vec<SourceFile>` with two kinds of entry. The CLI
builds it with the mirrors first (`cli/src/program_loader.rs`).

1. `SourceFile::ambient(name, text)` for every `.d.ts` mirror that
   `subscript bind` generated from your C headers. An ambient file
   is visible to every program file without an import.
2. `SourceFile::new(name, text)` for each `.ts` file. The base name
   without `.ts` is the module stem that an `import` resolves
   against.

```rust
let files = vec![
    SourceFile::new("helpers.ts", "export function doubled(v: i32): i32 { return v * 2; }\n"),
    SourceFile::new(
        "main.ts",
        "import { doubled } from \"./helpers\";\nexport function main(): void { print(`${doubled(21)}`); }\n",
    ),
];
```

That program prints `42`. `parse_import_specifiers(&file)` returns
one file's import specifiers with the same parser the checker uses,
so a host walks the import graph itself. `cli/src/program_loader.rs`
is the reference implementation for on-disk loading.

## A frame-loop host in four steps

The example embeds this script (`logic.ts` — module state, an
`update` entry the host drives, and the `main(): void` export the
loader requires):

```ts
let ticks: i32 = 0;

function doubled(value: i32): i32 {
  return value * 2;
}

export function update(): void {
  ticks += 1;
  print(`tick=${ticks}, helper=${doubled(ticks)}`);
}

export function main(): void {}
```

**1. Check before running** — rejection renders exactly as the CLI
renders it:

```rust
let files = vec![SourceFile::new("logic.ts", include_str!("../logic.ts"))];
check_program(&files).map_err(|diagnostics| render_diagnostics(&files, &diagnostics))?;
```

`ReloadSession::new` checks the program again, so this step is
optional. Keep it when you want the diagnostics in your own error
type instead of a `RunError`.

**2. Start a session and drive frames.** One `ReloadSession` owns one
live Context; `call_export` invokes an entry, `take_output` drains
what `print` wrote:

```rust
let mut session = ReloadSession::new(&files)?;
for _ in 0..3 {
    session.call_export("update")?;
    stdout.extend(session.take_output());
}
```

**3. Hot-swap function bodies mid-run.** `reload` with sources whose
declarations hash identically swaps the bodies and keeps the Context —
the tick counter continues across the swap:

```rust
session.reload(&v2_files)?;   // doubled() now returns value * 10
```

**4. A declaration edit is refused, and the session survives.**
Changing a signature (or a class field, or module state) changes the
declaration hash; `reload` returns
`ReloadError::DeclarationChanged`, naming the declaration, and the
old program keeps running:

```rust
match session.reload(&v3_files) {
    Err(e @ ReloadError::DeclarationChanged { .. }) => eprintln!("{e}"),
    ...
}
```

Running the whole flow (`cargo run -p subscript-example-rust-host`):

```text
tick=1, helper=2
tick=2, helper=4
tick=3, helper=6
tick=4, helper=40      ← V2 swapped in; ticks survived the reload
tick=5, helper=50
tick=6, helper=60      ← the frame after the refused V3
```

with the refusal on stderr:

```text
reload refused: declaration `function doubled` changed; only function bodies can be hot-swapped
```

The integration test (`examples/rust-host/tests/host.rs`) pins this
exact output, so the example's output stays pinned.

## Exports that take arguments

`call_export(name)` calls a zero-argument `void` export.
`call_export_with(name, args)` calls an export that takes parameters
(`compiler.md` §59, §61).

An export is **host-callable** under three conditions: it is
synchronous, it returns `void`, and every parameter is a boundary
scalar (a sized numeric or `boolean`), an opaque handle, or a
wire-mapped (`CEnum`) string alias. `EntryArg` covers exactly those.

```ts
export function setSpeed(id: i32, speed: f32, enabled: boolean): void {
  print(`id=${id} speed=${speed} enabled=${enabled}`);
}

export function main(): void {}
```

```rust
session.call_export_with(
    "setSpeed",
    &[EntryArg::I32(7), EntryArg::F32(1.5), EntryArg::Bool(true)],
)?;
```

`session.take_output()` then holds:

```text
id=7 speed=1.5 enabled=true
```

The call validates the name, the arity, and every argument kind
before any script code runs. Each failure is `RunError::Internal`,
and the Context stays untouched:

```text
internal lowering error: `nope` is not a host-callable export
internal lowering error: `setSpeed` expects 3 argument(s), got 1
internal lowering error: `setSpeed` argument 0 expects i32, got i64
```

A handle crosses as `EntryArg::Handle(*mut c_void)`. At the C level
the parameter is a borrow for the duration of the call. Handle
values are copyable, and the script can wrap and store one; the
borrow discipline above the language stays yours.

An export that is not host-callable remains a legal script export.
It gets no host symbol, and the checker rejects nothing new.

`EntryArg` has one variant per argument kind: `Handle`, and the
boundary scalars `I8`, `U8`, `I16`, `U16`, `I32`, `U32`, `I64`,
`U64`, `F16` (raw binary16 bits), `F32`, `F64`, and `Bool`. A
wire-mapped alias parameter crosses as `EntryArg::I32`; a value
outside the alias table traps before the entry body runs
(`compiler.md` §61).

## The errors a host handles

Two error types reach the host. Both implement `Display` and
`std::error::Error`.

**`RunError`** comes from `ReloadSession::new`, `call_export`,
`call_export_with`, `async_step`, `run_jit`, and `run_c_aot`.

| Variant | What it means | Your next step |
|---|---|---|
| `Rejected(Vec<Diagnostic>)` | The checker refused the program. No Context exists. | Render with `render_diagnostics(&files, &diagnostics)` and stop. |
| `Trap(TrapReport)` | The script faulted. Your process is alive. | Report it and keep the session; the next call runs. |
| `UnresolvedForeignSymbol(String)` | The script calls a C symbol that no supplied `NativeLibrary` registers. | Register the symbol, or correct the mirror. |
| `AbnormalTermination(AbnormalTermination)` | Generated or foreign code ended outside the trap protocol. | Treat it as a bug in the host or in a native library. |
| `Internal(String)` | A backend failure, or a rejected `call_export_with` argument. | Report the message. A lowering failure is a defect of this compiler. |

**`ReloadError`** comes from `ReloadSession::reload` only. In every
failure case the running program and its Context stay untouched.

| Variant | What it means | Your next step |
|---|---|---|
| `Rejected(Vec<Diagnostic>)` | The edited sources do not check. | Show the diagnostics and keep driving the old program. |
| `DeclarationChanged { declaration }` | A signature, class field, enum member, or module variable changed. | Restart the session to pick the edit up. |
| `ScriptOnStack` | Script code is on the stack. | Reload between host calls only. |
| `LiveWorkers` | The Context still owns a worker thread. | Join every worker, then reload. |
| `UnresolvedForeignSymbol(String)` | An edited body calls a symbol that no supplied library registers. | Register the symbol, then reload. |
| `Internal(String)` | A backend failure. | Report the message. |

`reload` takes `&mut self`, so a direct call during a frame does not
compile. The session also checks the Context's script depth, which
`call_export` raises for the duration of a call. `ScriptOnStack`
reports that second check.

### Diagnostics and warnings

`Diagnostic` carries `code: RuleCode`, `message: String`, and
`pos: Pos`. The rule codes are `S001`–`S014`, `S016`–`S019`, and
`S100`. `Pos` displays as `file:line:column`.

`render_diagnostics` produces the CLI's text: the source snippet,
the caret, the rule, and — when the diagnostic carries a
divergence — the TypeScript form beside the subscript form.

```text
error[S007]: bare `number` is rejected; there is no default numeric type — use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, f16, f32, f64)
 --> logic.ts:1:39
  |
1 | export function main(): void { let x: number = 1; }
  |                                       ^
  = rule: Bare `number` is rejected; sized numeric types are mandatory.
  = TypeScript accepts:
  |   const count: number = 3;
  = subscript:
  |   const count: i32 = 3;
  = why: `number` is a 64-bit float with no C width, so every declaration names one of the sized types. (collisions.md C3)
error: 1 error(s)
```

Warnings are separate data and never change acceptance. Run
`check_warnings` on the module that `check_program` returned:

```rust
let module = check_program(&files)?;
let warnings = check_warnings(&module);
if !warnings.is_empty() {
    eprintln!("{}", render_warnings(&files, &warnings));
}
```

`Warning` carries `code: WarnCode`, `message: String`, and
`pos: Pos`. There are four codes:

- `W001` — a reference-class allocation repeats in a loop, and
  neither escapes the iteration nor is released.
- `W002` — a local is used after `Context.free(local)` in the same
  block.
- `W003` — a callback-info aggregate registers freshly allocated
  userdata in a loop.
- `W004` — a value-type copy is written through and never read.

`WarnCode::ALL` lists them, and `WarnCode::explanation()` returns
the one-line rule. `render_warnings` uses the same shape as
`render_diagnostics`:

```text
warning[W004]: `bag` is a value-type parameter copy that is written through but never read
 --> logic.ts:22:3
   |
22 |   bag.pos = new Vec2f(x, x);
   |   ^
   = rule: A value-type copy that is written through and never read leaves its source unchanged.
warning: 1 warning(s)
```

## Traps, from the Rust side

A trap is a runtime fault the language defines (`collisions.md` C6).
It uses no signal, no unwinding, and no SEH. The runtime records the
fault and sets a flag in the Context; generated code returns through
the script call stack, and the driver reads the record. Your process
survives, and no foreign frame is unwound.

`TrapReport` carries four fields:

- `rule: subscript_runtime::TrapKind` — the violated rule, for
  example `DivisionByZero`, `IndexOutOfBounds`, `UseAfterDelete`,
  `AllocationFailure`, `WorkerTrapped`.
- `message: String` — the detail.
- `pos: Pos` — the TypeScript position of the faulting construct.
- `stdout: Vec<u8>` — the exact bytes the program printed before the
  Context stopped.

That last field is the practical one: a frame's output does not
disappear with the fault. `report.stdout` **copies** the sink and
does not drain it, so `take_output()` still returns the same bytes.
Take one copy, never both:

```rust
match session.call_export("update") {
    Ok(()) => stdout.extend(session.take_output()),
    Err(RunError::Trap(report)) => {
        // `report.stdout` holds the same bytes; drain the sink once.
        stdout.extend(session.take_output());
        eprintln!("{}: trap [{}]: {}", report.pos, report.rule, report.message);
    }
    Err(error) => return Err(error),
}
```

For the division-by-zero above, the fields read:

```text
rule:    DivisionByZero        (Display: division-by-zero)
message: integer division by zero
pos:     logic.ts:4:25
stdout:  "before division\n"
```

`TrapReport` also implements `Display`, which joins the position,
the rule, and the message:

```text
logic.ts:4:25: trap [division-by-zero]: integer division by zero
```

A trap ends the call, not the session. `call_export` clears the trap
record on entry, so the next frame runs over the same Context state.
Clearing is reporting-only: a stale coroutine traps again on the next
resume, and a deleted allocation stays deleted.

To match on `report.rule`, add `subscript-runtime` to your
dependencies; `TrapKind` lives there and `TrapReport` does not
re-export it.

**`AbnormalTermination`** is the other failure shape. It carries
`status: String`, `stdout: Vec<u8>`, and `stderr: Vec<u8>`, and it
retains the bytes produced before the process stopped.
`ReloadSession` never produces it, because a session runs in your
process. `run_jit` on Unix runs the program in a forked child, so a
signal in generated or foreign code arrives as
`AbnormalTermination`. `run_c_aot` reports a linked program's
non-trap exit the same way.

## The run entry points beside `run_jit`

Development tier, all in `subscript_codegen`:

- `run_jit(&files)` — check, lower, run the exported `main(): void`,
  and return the exact stdout bytes.
- `run_jit_with_native_libraries(&files, &libraries)` — the same run,
  with your registered C symbol addresses available to foreign calls.
- `run_jit_with_memory_accounting(&files, freed_handle_diagnostics)` —
  returns the stdout bytes and `JitMemoryAccounting { live_bytes,
  reserved_bytes }`, read while the run's Context is still alive.
- `run_jit_with_memory_accounting_and_native_libraries(...)` — both
  of the two above.
- `run_jit_with_alloc_failure(&files, n)` — refuses the `n`-th
  Context allocation, so a host exercises its own allocation-failure
  path. The fault is armed before `subscript_init`.
- `run_jit_with_freed_handle_diagnostics_and_native_libraries(...)` —
  retains and poisons freed allocations, so a use-after-free names
  its site.
- `run_jit_configured(&files, config)` — one `RunConfig` record in
  place of the named combinations above; returns `RunOutput`.
- `jit_compile_time`, `jit_bench`, `jit_bench_with_warmup_floor` —
  measurement entry points for the performance gate, not host API.
- `JIT_OUTPUT_FILE_ENV` — an optional environment override that names
  a parent-owned output file for a JIT run.

On Unix the `run_jit*` helpers run the program in a forked child, so
the output survives a run that does not complete normally. One
consequence matters for a host: a native library's writes to host
memory during such a run are not visible in your process. Use
`ReloadSession` when you need those writes.

Ship tier, same crate:

- `run_c_aot(&files)` — emit C, compile and link it against the
  runtime archive, run it, and return the stdout bytes. It needs a C
  compiler on the machine.
- `run_c_aot_with_native_libraries(&files, &libraries)` — the same,
  and your C sources join the compile.
- `run_c_aot_with_native_libraries_and_host_hooks(&files, &libraries,
  pre, post)` — names two C functions of signature `void
  hook(subscript_rt_context *)`; one runs after initialization and
  before the entry, the other after the run.
- `run_c_aot_with_alloc_failure`,
  `run_c_aot_with_freed_handle_diagnostics_and_native_libraries`, and
  `run_c_aot_configured` — the ship-tier forms of the same options.
  `RunConfig::memory_accounting` is development-tier only and returns
  `RunError::Internal` here.

`RunConfig` holds `native_libraries`, `fail_alloc_after`,
`freed_handle_diagnostics`, `memory_accounting`, `pre_entry_hook`,
and `post_run_hook`. `RunOutput` holds `stdout` and an optional
`memory_accounting`.

`NativeLibrary::new(include_directories, c_sources, symbols)` is
`unsafe`: every symbol address must stay valid for every run that
receives the value, and each address must implement the C signature
that the mirror declares for its name.

## Workers, from a Rust host

A script spawns workers through the standard library (`Worker`,
`Inbox`, `Outbox`; Q35, `compiler.md` §38–§40). The host calls
nothing: there is no worker API on `ReloadSession`.

```ts
class EchoMessage {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

function echo(inbox: Inbox<EchoMessage>, outbox: Outbox<EchoMessage>): void {
  const message: EchoMessage | null = inbox.wait();
  if (message !== null) {
    outbox.post(new EchoMessage(message.value * 2));
  }
}

export function update(): void {
  const worker: Worker<EchoMessage, EchoMessage> = Worker.spawn(echo);
  worker.post(new EchoMessage(37));
  worker.close();
  worker.join();
  const reply: EchoMessage | null = worker.poll();
  if (reply !== null) {
    print(`echo=${reply.value}`);
  }
}

export function main(): void {}
```

One `session.call_export("update")` drives the whole round trip, and
`take_output()` returns `echo=74`.

What changes for the host:

- **Your Context stays single-threaded.** Each worker owns a fresh
  `Context` that the runtime creates, initializes, runs, and
  releases on the worker's own OS thread. A Context never migrates
  between threads (§38.1).
- **Module state is per Context in both tiers**, so a worker's
  module variables are its own (§38.1).
- **Messages cross as copies.** The runtime copies the message
  class's payload bytes, plus each `string` field's bytes, into a
  queue record, then allocates a fresh object in the receiving
  Context (§39.1, §84.1). No pointer crosses a thread.
- **A worker cannot outlive its host call in script.** The checker
  rejects a `Worker`, `Inbox`, or `Outbox` in a module global, a
  class field, an array element, a container type argument, or a
  lambda capture (§40.1).
- **Reload waits for the workers.** If the Context still owns a
  worker at the frame boundary, `reload` returns
  `ReloadError::LiveWorkers`:

  ```text
  reload refused: the Context has live workers; join them before swapping code
  ```

  After every worker joins, a body-only edit swaps normally.
- **A worker fault is loud at the join.** `join` on a worker whose
  Context trapped traps the joining Context, and the host receives
  it as an ordinary `RunError::Trap`:

  ```text
  rule:    WorkerTrapped        (Display: worker-trapped)
  message: worker trapped with division-by-zero at position 3: integer division by zero
  stdout:  "posted\n"
  ```

- **Teardown is automatic.** Releasing the parent Context closes,
  joins, and frees any remaining worker (§39.2).

## The ship path from Rust

`emit_c_files` checks the program and writes the complete artifact
set. `subscript emit` and `subscript build` call it:

```rust
let emitted = emit_c_files(&files, Path::new("out"), "program", true)?;
```

The fourth argument requests the generated host entry. With `true`
the call writes three files:

| File | Contents |
|---|---|
| `out/program.c` | The program translation unit |
| `out/program.alloc.h` | The allocation-metadata header |
| `out/entry.c` | The generated `main`: it creates the Context, runs the initializer, calls the entry, and releases the Context |

Pass `false` when your own `main` drives the Context. The call then
writes no `entry.c`, and it accepts a module with no exported
`main`. `EmittedCFiles` also reports `source_len`, the byte length
of the translation unit.

`EmitCFilesError` has three variants: `Diagnostics(Vec<Diagnostic>)`
for a rejected program, `Emission(String)` for a C-lowering failure,
and `Io { action, path, source }` for a write failure.

Compile the result as C11 and link the runtime archive. Run this
from the repository root. `$OUT` names the directory you passed to
`emit_c_files`, and the archive below is the development profile's:

```sh
$ clang -std=c11 -O2 -Iruntime/include \
    "$OUT/program.c" "$OUT/entry.c" \
    target/debug/libsubscript_runtime.a -o "$OUT/program"
$ "$OUT/program"
hello from the ship tier
```

A host build script gets the same three inputs from the crate, so no
path is hard-coded:

- `runtime_staticlib_path()` — the archive for the running profile.
  It builds the archive when it is missing or older than the runtime
  sources. `SUBSCRIPT_RUNTIME_STATICLIB` (the constant
  `RUNTIME_STATICLIB_ENV`) overrides it.
- `include_directory_arg(style, dir)` — one joined include argument
  (`-Iruntime/include`, or `/I…` for MSVC).
- `runtime_system_libraries(style)` — the host's system libraries.
  Windows lists five import libraries (`kernel32 ntdll userenv
  ws2_32 dbghelp`), non-macOS Unix lists `m dl pthread rt util
  gcc_s c`, and macOS lists none.

`host_c_compiler()` returns the compiler this repository selects,
including MSVC discovery on Windows. `CCompilerStyle` says which
argument spelling to use. From here the
[C/C++ tutorial](tutorial-c-cpp.md) applies unchanged — the emitted
artifact is C either way.

## Smaller pieces a host uses

- **One-shot execution**: `run_jit(&files)` returns the program's
  stdout bytes — what `subscript run` does.
- **`main` by name**: `session.call_main()` calls the exported
  `main(): void`. A session does not require that export
  (`compiler.md` §53).
- **Async**: an async export kicks a root but does not pump it.
  `session.async_pending()` counts the suspended roots, and
  `session.async_step()` polls each root pending at entry once and
  returns the number still pending. A Rust host keeps the same
  explicit control a C host has.
- **A trap during initialization**:
  `ReloadSession::new_capturing_initializer_trap(&files)` returns the
  session and the trap together, so a reload-capable host reports the
  fault and keeps the Context for the next edit. `ReloadSession::new`
  discards the session instead.
- **The declaration fingerprint**: `session.declaration_hash()`
  returns the current `DeclarationHash`.
  `first_difference(&other)` names the first declaration that
  differs, which is how the refusal message gets its name.
- **Watching files** is host logic, not language surface: the CLI's
  reload state machine is 212 lines over `ReloadSession`
  (`cli/src/watch.rs`), and reads as a reference implementation.
  Polling and terminal I/O stay in the command loop.

## Reading on

- [`examples/rust-host/`](../examples/rust-host/) — the complete
  example this tutorial quotes.
- [`docs/tutorial-typescript.md`](tutorial-typescript.md) — the
  language itself, for the people who write the scripts.
- [`specs/blocks/compiler.md`](../specs/blocks/compiler.md) §8.2 —
  the hot-reload contract (what hashes, what swaps, what traps);
  §59 and §61 — host-callable exports; §38–§40 — Workers.
- [`docs/tutorial-c-cpp.md`](tutorial-c-cpp.md) — binding your
  engine's header and the ship-tier link line.
