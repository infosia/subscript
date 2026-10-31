<!-- §214 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 214. The runtime archive builds under the pinned toolchain

*(Added 2026-10-31.)* Origin: §196 Open items 2 and 3. On 2026-10-31
the owner asked for the open work in priority order. The measurement is
in `specs/tracking/runtime-archive-toolchain.md`.

Problem, measured at `4da0d6bb`: `build_runtime_staticlib` starts
`cargo` in the working directory of the caller when `CARGO` is not set.
`cargo --version` gives 1.95.0 in the repository, the version that
`rust-toolchain.toml` pins, and 1.99.1 outside it. So a `subscript
build` from another directory with a stale archive builds the runtime
under a compiler that the repository does not pin.

The freshness check reads the runtime sources and `runtime/Cargo.toml`.
It does not read `Cargo.lock`, so a change of a locked dependency keeps
the old archive.

### 214.1 Rules

1. The command that builds the runtime archive runs in the runtime
   directory. So `rustup` selects the pinned toolchain from any working
   directory of the caller.
2. An archive is fresh only if it is not older than the workspace
   `Cargo.lock`, and not older than each path that the rule at the pin
   reads.
3. Two processes with a stale archive each start `cargo build`, as at
   the pin. `cargo` serializes them, and a process finds no work after
   the first one ends. This section does not change it.

### 214.2 Acceptance

1. A unit test reads the current directory of the build command, and
   gives the runtime directory. A control shows that the test fails
   with no current directory on the command.
2. A unit test extends the freshness test: a `Cargo.lock` newer than
   the archive gives stale, and an older one gives fresh.
3. No golden moves. §196.3 Open cites this section.
