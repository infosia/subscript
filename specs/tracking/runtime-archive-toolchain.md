# The runtime archive builds under the pinned toolchain — `compiler.md` §214

Status: implemented; Phase Review open.

## Measurement at the pin `4da0d6bb`

| Directory | `cargo --version` |
|---|---|
| repository | 1.95.0 |
| `$TMPDIR` | 1.99.1 |

`rust-toolchain.toml` selects 1.95.0 by the working directory. The
default toolchain of the host is `stable`.

The `subscript` CLI does not reach this path. From `$TMPDIR`,
`subscript build --source` exits 2: "runtime paths cannot be resolved
outside a subscript repository". `cli/src/runtime_paths.rs` sets its own
current directory. The first text of §214 named the CLI, and the coding
agent measured that it was wrong.

Scratch `staticlib` crate, `cargo` of the pin:

| Step | Result |
|---|---|
| `Cargo.lock` with a future time, `cargo build` | no work; archive time unchanged |
| archive set to the present, source changed, `cargo build` | rebuilt; the new symbol is in the archive |
| `rlib` + `staticlib` with a dependent bin, uplifted archive touched | no work; `target/debug/` and `deps/` hold different inodes |

The first row is the reason for §214.1 rule 2a. The other two rows show
that the step does not hide a change from `cargo` or relink a dependent
binary.

From `$TMPDIR` with `CARGO` not set, a codegen test through `run_c_aot`
passes, and the archive time does not change. The toolchain path with a
stale archive has no end-to-end run: a unit test reads the current
directory of the command.
