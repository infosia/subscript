# Text module: the cost of grapheme and NFC support

Measurement pin: `3cc0aae2`.
Platform: aarch64 macOS. Date: 2026-10-11.
All builds are release builds. Sizes are in bytes.

This is a Step 0 measurement. The prototype adds three runtime functions
and two dependencies. It does not establish a contract. All production
changes and the measurement program were reverted. The remaining
repository change is this note. No commit was made.

The owner decision of 2026-10-11 sets the frame: the binary-size cost
decides the module. If it is added, a build enables it at compile time,
the default is off, and the model is the §185 file module. A host or a
program that does not enable it carries none of the tables.

## 1. Candidates

| Crate | Version | Unicode data | Licence | In the local cargo cache |
|---|---|---|---|---|
| `unicode-segmentation` | 1.13.3 | Unicode 17.0.0 (`src/tables.rs` `UNICODE_VERSION`) | MIT OR Apache-2.0 | yes |
| `icu_normalizer` + `icu_normalizer_data` | 2.2.0 | ICU release-78.1rc, CLDR 48.2.0 (`icu_normalizer_data` README); ICU 78 is Unicode 17 *(docs)* | Unicode-3.0 | yes |
| `icu_segmenter` + `icu_segmenter_data` | 2.3.0 | ICU 78 data | Unicode-3.0 | yes; not measured |
| `unicode-normalization` | — | not read | MIT OR Apache-2.0 *(docs)* | no |

- `unicode-normalization` and its dependency `tinyvec` are not in the
  cache, and the sandbox cannot reach `index.crates.io`. To measure it,
  the owner runs `! cargo fetch` in a crate that declares
  `unicode-normalization = "0.1"`. This round measures
  `icu_normalizer` for NFC.
- The workspace already locks `icu_normalizer` 2.2.0. The path is
  `swc_common` → `url` → `idna` → `idna_adapter` → `icu_normalizer`.
  The compiler, so the `subscript` CLI, links it today. The runtime
  does not.
- `icu_segmenter` 2.3.0 gives graphemes from ICU data. It also needs
  `icu_locale_fallback` and a second `icu_collections`/`icu_provider`
  version beside the 2.2.0 set in the lock. The round did not measure it.
- `node` v24.18.0 reports Unicode 17.0 and ICU 78.3
  (`process.versions`). The two measured crates use the same Unicode
  version as the divergence detector.

## 2. The prototype

- `subscript_rt_text_grapheme_length(ctx, s) -> i32`: the count of
  extended grapheme clusters (`graphemes(true).count()`).
- `subscript_rt_text_slice_graphemes(ctx, s, start, end, pos) -> string`:
  one scan for the byte offsets of clusters `start` and `end`, then one
  Context string copy.
- `subscript_rt_text_normalize_nfc(ctx, s, pos) -> string`:
  `ComposingNormalizerBorrowed::new_nfc().normalize_utf8`. A borrowed
  result (the input is NFC) returns the input handle. An owned result
  is copied into a Context string.

Two forms were built:

- **No switch.** The dependencies are unconditional. The JIT symbol
  table registers the three functions.
- **Cargo feature `text`.** The dependencies are optional. The runtime
  module and the three JIT symbols are behind `cfg(feature = "text")`.
  `subscript-codegen` and `subscript-cli` forward the feature.

The checker does not accept a text API. For (c) and (d), a second C
translation unit calls the runtime functions from a constructor, so
the call is reachable from the program root. This stands for an
emitted call.

## 3. Size

The program for (b), (c), and (d) is a 3-line `main` that prints a
string concatenation and a `slice`. "Ship link" is the link that
`build_c_aot` and `subscript build` run today:
`cc -std=c11 -O2 -fwrapv -ffp-contract=off program.c entry.c libsubscript_runtime.a`.
It passes no dead-code flag. "Dead strip" adds `-Wl,-dead_strip`.
Executables are measured after `strip`.

| Artifact | HEAD | No switch | Feature off | Feature on |
|---|---:|---:|---:|---:|
| (a) `libsubscript_runtime.a` | 21,138,328 | 21,976,496 (+838,168) | 21,138,392 (+64) | 21,976,352 (+838,024) |
| (b) program without the module, ship link | 2,841,952 | 3,404,688 (+562,736) | 2,841,952 (0) | 3,404,688 (+562,736) |
| (b) program without the module, dead strip | 658,352 | 658,368 (+16) | 658,352 (0) | 658,368 (+16) |
| (c) + grapheme count and slice, ship link | — | 3,404,720 (+562,768) | — | 3,404,704 (+562,752) |
| (c) + grapheme count and slice, dead strip | — | 691,616 (+33,264) | — | 691,600 (+33,248) |
| (d) + NFC, ship link | — | 3,404,720 (+562,768) | — | 3,404,704 (+562,752) |
| (d) + NFC, dead strip | — | 741,680 (+83,328) | — | 741,680 (+83,328) |
| (e) `subscript` CLI | 11,792,888 | 11,892,632 (+99,744) | 11,792,888 (0) | 11,876,152 (+83,264) |

Deltas are against HEAD. The (e) raw sizes are 13,902,064 (HEAD),
14,005,680 (no switch), 13,902,064 (feature off), 13,989,184 (feature on).

Facts:

1. **Dead-code removal alone makes "off" free only under a dead-strip
   link.** With `-Wl,-dead_strip`, (b) grows 16 bytes. With the ship
   link of today, (b) grows 562,736 bytes (+19.8%): the linker takes
   whole archive members, and the members carry the tables.
2. The ship link of today carries 2,183,600 bytes that a dead-strip
   link removes from (b) at HEAD (2,841,952 → 658,352). This is
   independent of the text module.
3. A host that links the archive into its own executable decides the
   link flags. The runtime cannot make "off" free for a host that does
   not dead-strip, unless the archive does not contain the code.
4. The dev tier (e) and a Rust host that embeds `subscript-codegen`
   register every runtime symbol by address in `jit/symbols.rs`. That
   reference keeps the code in the binary. Dead-code removal cannot
   remove it.
5. **The cargo feature makes "off" free in every form.** Feature off
   gives the HEAD size for (a), (b) under both links, and (e).
6. The cost of "on" under a dead-strip link: graphemes +33 KB, NFC
   +83 KB. Under the ship link of today: +563 KB for either.
7. The (e) delta (+83 KB to +100 KB) is less than the sum of (c) and
   (d) under dead strip (+117 KB). The CLI already links
   `icu_normalizer` through `idna`.

## 4. Run time

Release, one process, `Context::new_releasing()`. Each cell is the
best of 7 batches (20,000 calls at about 100 bytes, 500 calls at about
10 KB), in ns per call. A `collect` runs between batches, outside the
clock. The slice takes clusters `[0, n/2)`, where `n` is the cluster
count. The reference column is the existing byte `slice` over the same
byte range.

| Input | Bytes | Clusters | `graphemeLength` | `sliceGraphemes(0, n/2)` | byte `slice`, same range | NFC |
|---|---:|---:|---:|---:|---:|---:|
| ASCII | 90 | 90 | 1,045 | 558 | 93 | 204 |
| Japanese BMP | 90 | 30 | 483 | 298 | 140 | 94 |
| Supplementary kanji | 70 | 20 | 442 | 274 | 122 | 87 |
| Combining dakuten (not NFC) | 90 | 15 | 470 | 296 | 137 | 740 |
| Precomposed dakuten (NFC) | 90 | 30 | 436 | 280 | 139 | 93 |
| Emoji ZWJ sequences | 54 | 3 | 370 | 289 | 111 | 69 |
| ASCII | 9,990 | 9,990 | 113,509 | 58,016 | 558 | 20,643 |
| Japanese BMP | 9,990 | 3,330 | 52,044 | 29,682 | 5,675 | 7,584 |
| Supplementary kanji | 9,975 | 2,850 | 62,541 | 34,399 | 5,297 | 10,075 |
| Combining dakuten (not NFC) | 9,990 | 1,665 | 50,804 | 28,810 | 5,673 | 73,185 |
| Precomposed dakuten (NFC) | 9,990 | 3,330 | 46,560 | 26,666 | 5,655 | 7,437 |
| Emoji ZWJ sequences | 9,990 | 555 | 67,414 | 36,489 | 5,112 | 9,974 |

Facts:

1. A cluster scan costs about 5 to 11 ns per byte. ASCII is the
   slowest per byte, because each byte is one cluster.
2. NFC on text that is already NFC costs about 0.7 to 2 ns per byte.
   NFC on text that is not NFC costs about 7.3 ns per byte.
3. The prototype validates UTF-8 (`str::from_utf8`) on each call. That
   cost is inside the numbers.

## 5. Allocation

A counting global allocator counted the allocations of one call.

1. `graphemeLength` allocates nothing, at every input and size.
2. `sliceGraphemes` allocates only its result. At about 10 KB, one
   call makes one allocation of the result bytes plus the 24-byte
   Context string block. At about 100 bytes, the Context arena serves
   the result. One call made a 65,536-byte arena chunk; that is arena
   growth, not the scan. The scan allocates nothing.
3. NFC on text that is already NFC allocates nothing, and returns the
   input handle.
4. NFC on text that is not NFC, at about 10 KB, makes 2 allocations
   (15,009 bytes): the `String` that `normalize_utf8` returns, then the
   Context copy. `icu_normalizer` 2.2.0 also has
   `split_normalized_utf8` (the NFC prefix) and a `core::fmt::Write`
   sink form (`normalize_to`). A contract implementation can use them
   to write once, into the Context string.

## 6. Build time

Clean `cargo build --release -p subscript-runtime`, one run each, in
an empty target directory.

| Form | Wall time | CPU (user) |
|---|---:|---:|
| HEAD | 5.37 s | 20.6 s |
| No switch | 7.57 s | 31.1 s |
| Feature off | 5.45 s | — |
| Feature on | 7.77 s | — |

The dependency adds about 2.2 s wall time to a clean runtime build.
Feature off gives the HEAD time. A clean release build of the CLI took
47.8 s at HEAD, 47.9 s with no switch, and 48.6 s / 51.1 s with the
feature off / on.

## 7. The enable path

`--enable-module` reaches each stage today through these files. The
§185 module is host I/O, so it adds no runtime code. A text module adds
runtime code, so it also needs a build-time switch for the runtime
archive and the JIT symbol table (section 3, fact 5).

| Stage | §185 path today | A text module touches |
|---|---|---|
| Option | `cli/src/lib.rs` parses `--enable-module` for `check`, `emit`, `build`, `run`, `boundary`; `codegen/src/lib.rs` `RunConfig::with_enabled_modules` | no change to the parse |
| Checker | `compiler/src/lib.rs` `STANDARD_MODULES`, `CheckOptions::enabled_modules`; `compiler/src/check/signatures.rs` rejects the import when the module is not enabled | add the specifier to `STANDARD_MODULES`; a gate in `check/signatures.rs` (import form) or `check/expr/method.rs` (method form); new `RejectionSite` rows in `check/rejection_sites.rs`, `rejection.rs`, `rejection_witness_index.rs`, `rejection_witness_sites.rs` |
| Declaration | `prelude/lang.d.ts` `declare module "node:fs/promises"` | `prelude/lang.d.ts` |
| Builtin identity | the HIR standard callee for file operations | `compiler/src/hir/intrinsics.rs` (name, runtime symbol, parameters, signature); `compiler/src/ambient.rs` lists `normalize` as rejected today |
| LIR | `HostCompletion` with a standard operation | `codegen/src/lir.rs` symbol table, `codegen/src/lir/operation_table.rs`, `codegen/src/lir/call.rs` |
| Dev JIT | `codegen/src/jit/symbols.rs` registers `subscript_rt_file_operation`; `codegen/src/jit/protocol.rs` sends `enabled_modules` to the runner process | `codegen/src/lower/mod.rs` import signatures; `codegen/src/jit/symbols.rs` under the feature |
| C AOT | `codegen/src/cemit/graph.rs` emits the call; the emitted C declares each runtime extern at its use | `codegen/src/cemit/` call site; `codegen/src/ship.rs` `build_runtime_staticlib` builds with `--features text` when the module is enabled, and the two archives need two paths |
| Interpreter | `interpreter/instruction.rs` returns Unsupported for a host call | `codegen/src/interpreter/intrinsics.rs` calls the runtime function directly; text is pure computation, so the interpreter can run it |
| Corpus | `codegen/tests/corpus/mod.rs` reads `// enable-module:`; `check/rejection_programs.rs` reads `enable-module=` | entries and goldens |
| Cargo | — | `runtime/Cargo.toml` optional dependencies and feature; `codegen/Cargo.toml`, `cli/Cargo.toml` forward it |

Open in the path: `lower/mod.rs` declares every runtime import when it
builds a module. If a declared import must resolve when no call uses
it, the declaration must also be behind the feature. The round did not
test this.

## 8. Decisions a contract needs

1. **Switch form.** Dead strip alone does not meet "carries none of the
   tables" for a host without a dead-strip link or for any JIT host
   (section 3, facts 1, 3, 4). A cargo feature meets it in every
   measured form. The contract chooses the feature, or a separate
   archive, and states how `--enable-module` selects the archive in
   `build_runtime_staticlib` and `subscript build`.
2. **The dev CLI.** Built with the feature, the CLI grows 83 KB, and
   `run --enable-module` works with no rebuild. Built without it, the
   CLI cannot run a program that uses the module. The owner's rule covers shipped
   hosts; the contract states whether it covers the dev CLI.
3. **API form.**
   - NFC: `s.normalize("NFC")` and `s.normalize()` are ES2015 methods.
     `tsc` 5.9.2 accepts both with this project's `lib` (`ES2022`),
     with no prelude change. `node` runs them. `stdlib.md` §8 rejects
     `normalize` today for the missing tables. The method form needs a
     rule for a method that a module enable gates; §185 gates an
     import, not a method.
   - Graphemes: JavaScript has no method. Its form is
     `Intl.Segmenter` with `{ granularity: "grapheme" }` and an
     iterable of segment objects. `tsc` accepts it with `lib: ES2022`;
     it needs an options object literal and segment objects. A named
     import, such as
     `import { graphemeLength, sliceGraphemes } from "subscript:text"`,
     type-checks under `tsc` 5.9.2 with a `declare module` in the
     prelude. `node` cannot load that specifier, so its entries are not
     `js-comparable` and cite a collision id.
   - The contract selects the specifier, the names, and whether
     `sliceGraphemes` follows the `slice` clamping and negative-index
     rules.
4. **Unicode version.** Both measured crates are at Unicode 17, the
   same as `node` v24.18.0. The contract pins the crate versions and
   states the Unicode version. A version change moves goldens.
5. **NFC only, as the first step.** `icu_normalizer` NFC data also
   uses the decomposition tables too *(docs)*, so the NFC size includes
   them. NFD,
   NFKC, and NFKD add more tables. The contract states that the first
   step is NFC, and what `normalize("NFD")` and the other forms do.
6. **Invalid UTF-8.** `normalize_utf8` maps ill-formed bytes to
   U+FFFD. The contract states the rule for a string that is not valid
   UTF-8, or states that such a string cannot reach the function.
7. **The ship link flag.** Independent of this module, a dead-strip
   link removes 2.18 MB from (b) at HEAD. Whether `build_c_aot` and
   `subscript build` pass it is a separate decision.
