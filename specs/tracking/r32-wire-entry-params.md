# R32 — a wire-mapped alias in an entry signature

Status: **landed 2026-08-17** against `specs/blocks/compiler.md`
§61. Origin: downstream request R32. Contract `db52280`,
implementation `707e97d` (hashes after the rebase onto the
windows-msvc fix `2ff2205`).

## The request

The downstream's first R30 consumer declared
`init(instance, device, format: GPUTextureFormat)`; the checker
rejected the export because `GPUTextureFormat` is a `CEnum`
wire-mapped alias. R30's response invited the widening with
evidence. The downstream does not ask for plain string-literal
unions in entry signatures.

## Findings on this host, at `1f875da`

- The report reproduces with the wire-enum mirrors: the export
  fails with S100 "has a string-literal union alias in its
  boundary signature". The rejection site treats wired and
  unwired aliases alike.
- The validation machinery exists in both directions at the bind
  boundary (R23/R24, §52): `validate_wire_alias` and
  `subscript_rt_trap_wire_enum` on the ship tier, the same trap
  symbol on the dev tier, pinned by `t48`/`t49`.
- An entry parameter is the same direction as a bound-function
  return: C data enters script. The same validation applies.

## What landed

R30's host-callable subset widens: every parameter is a boundary
scalar, an opaque handle, or a wire-mapped alias. The host passes
the wire value as `int32_t` (`EntryArg::I32` on the dev tier).
Both tiers validate before the entry body runs: the ship wrapper
runs `validate_wire_alias` at the parameter's trap site; the
reload adapter compares against the wire table and calls
`subscript_rt_trap_wire_enum` on a miss, then returns without the
entry call. A plain alias in an exported signature and any alias
in an exported return stay rejected. In-script calls never pass
the wrapper, so nothing changes on the script-internal path.

Corpus: `a140-wire-entry-param` (wire `23` enters as `"m1"`,
byte-exact on both tiers), `t50-wire-entry-unknown-value` (wire
`12345` traps with kind 24 and the alias name; the body never
prints; both tiers), `r134-plain-alias-entry-param` (S100 stays).
The drive hooks live in `wire-enum.c` only, on the a137
weak-fallback pattern; no header or mirror moved.

## Red, at the contract pin

All three corpus entries failed with the §61 S100 ("exported
function `configure` has a string-literal union alias in its
boundary signature").

## Gates (this host, at `707e97d`)

- `cargo test --offline --workspace`: 55 suites, 963 passed, 0
  failed, 1 ignored, exit 0. The same counts in the release
  profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden, `.expected`, header, and mirror
  byte-identical. New: a140's golden (140 total) and t50
  (trap corpus 50).
