# subscript for Rust embedders

The compiler and runtime are Rust crates, and the `subscript` CLI is
itself a Rust host built on them. A Rust application can embed the
language in-process through the same crate surface the repository's
own gates consume — which buys one thing no C host has: the
**development tier lives in your process**. Scripts JIT-compile,
run, and hot-reload inside your application, with no C compiler in
the loop and no generated artifacts on disk.

Read this first, plainly:

- The crates are `publish = false`. Embedding means a path or git
  dependency pinned to a commit; there is no semver contract yet.
  The **contracted** host boundary remains the C ABI — the Rust
  surface is well-tested (every public item is documented and
  unit-tested) but it is the same surface the CLI consumes, not a
  frozen API.
- Script-visible *host data* still crosses a C facade even in a Rust
  host: scripts bind C headers (`extern "C"` functions, `#[repr(C)]`
  structs, a mirror from `subscript bind`), and
  `ReloadSession::new_with_native_libraries` links them. The
  [C/C++ tutorial](tutorial-c-cpp.md) covers that side; this one
  covers the pure-Rust part.
- "No C compiler in the loop" refers to the run path. A cold `cargo
  build` still compiles a little C from transitive dependencies (the
  parser's stack-growth crate), like most Rust projects.

Everything below is the committed, test-pinned example
[`examples/rust-host/`](../examples/rust-host/); the outputs shown
are from running it.

## The crates

| Crate | What you use it for |
|---|---|
| `subscript-compiler` | `SourceFile`, `check_program` (accept/reject with `Diagnostic`s), `check_warnings`, `render_diagnostics` / `render_warnings` (the CLI's exact output shape) |
| `subscript-codegen` | `run_jit` (one-shot), `ReloadSession` (a live, hot-reloadable program), `emit_c_files` (the ship tier's C) |
| `subscript-runtime` | The `Context` itself; usually reached through the two crates above |

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
exact output, so the tutorial and the example cannot drift apart.

## Smaller pieces you may want

- **One-shot execution**: `run_jit(&files)` returns the program's
  stdout bytes — what `subscript run` does.
- **Warnings as data**: after a successful `check_program`, pass the
  module to `check_warnings` for `W001`/`W002`/`W003` values
  (`render_warnings` for the CLI's text form).
- **The ship tier**: `emit_c_files(&files, out_dir, "program", true)`
  writes the C translation unit your release build compiles like any
  other source; `subscript link-flags` answers what to link. From
  here the [C/C++ tutorial](tutorial-c-cpp.md) applies unchanged —
  the emitted artifact is C either way.
- **Watching files** is host logic, not language surface: the CLI's
  watch loop is ~150 lines over `ReloadSession` (`cli/src/watch.rs`)
  and reads as a reference implementation.

## Reading on

- [`examples/rust-host/`](../examples/rust-host/) — the complete
  example this tutorial quotes.
- [`specs/blocks/compiler.md`](../specs/blocks/compiler.md) §8.2 —
  the hot-reload contract (what hashes, what swaps, what traps).
- [`docs/tutorial-c-cpp.md`](tutorial-c-cpp.md) — binding your
  engine's header and the ship-tier link line.
