# The runtime archive builds under the pinned toolchain — `compiler.md` §214

Status: contract.

## Measurement at the pin `4da0d6bb`

| Directory | `cargo --version` |
|---|---|
| repository | 1.95.0 |
| `$TMPDIR` | 1.99.1 |

`rust-toolchain.toml` selects 1.95.0 by the working directory. The
default toolchain of the host is `stable`.
