<!-- §54 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 54. Caller link inputs follow the translation units

Downstream report 2026-08-10. The AOT link
commands put the caller's `c_sources` before the emitted
`program.c` and `entry.c`. GNU `ld` resolves an archive against
the symbols that are undefined at the position of the archive. At
that position no symbol is undefined, so `ld` keeps no member of
the archive, and the link fails on every symbol the program calls.

Measured on this host 2026-08-10 (clang 14.0.0, GNU ld 2.38): one
archive that defines `libProbe`, one caller that calls it. The
archive before the caller fails with `undefined reference to
`libProbe``. The archive after the caller links and runs. The
position of the archive is the only variable.

The defect is Linux-only. Apple `ld64` and MSVC `link.exe` resolve
an archive independent of its position, so the macOS and the
Windows gate hide it. A caller that supplies a `.c` file is also
not affected: the driver compiles that file into an object, and an
object contributes its symbols at any position.

### 54.1 Rule

Both AOT link commands order their inputs like this:

```
cc [flags] [-I…] program.c entry.c <caller c_sources> runtime.a <system libs>
```

The caller's link inputs come after the last emitted translation
unit and before the runtime archive. An include-directory argument
is position-independent, so it keeps its present position.

This applies to `run_c_aot_*` (the ship tier) and to
`run_aot_with_native_libraries` (the retained Cranelift-object
cross-check, §11).

### 54.2 What does not change

- No `--start-group`. The caller controls the order inside
  `c_sources`. A caller with a dependency between two of its own
  archives lists them in dependency order.
- The dev tier registers addresses, not link inputs (§23.6). It is
  unaffected.
- Every existing golden is byte-identical. The change moves link
  inputs; it computes nothing.
- The Cranelift-object test helper takes no native library. It
  does not change.

### 54.3 Exit criteria (pre-registered)

1. Test: a **static archive** in the `c_sources` slot, and a
   program that calls a symbol that only that archive defines. The
   test runs the ship C-AOT tier and compares the bytes to a
   committed expectation.
2. The same program runs through `run_aot_with_native_libraries`.
3. The new test fails on `1993578` on Linux. Record the red output
   before you change the source.
4. The archive fixture builds on every ship host. It must not
   spell `_Float16` (§11c constraint 2), so the MSVC gate keeps
   it.
5. `codegen/tests/native_library.rs` stays green; the workspace
   gate stays green; `cargo fmt --check` stays green.
