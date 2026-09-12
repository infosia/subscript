<!-- §49 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 49. R21 — a host-driven ship-tier form

Owner decision 2026-08-04 (downstream request R21). The finding
behind it is a gate-integrity one, not a feature gap: the ship
tier's runner compiles, links, and **spawns** a program whose
entry is `main`, so a host cannot run code before the script's
entry there. Every suite program therefore had to keep its
long-lived state script-side — and the downstream's P6 review
found a use-after-free that **passed both tiers precisely because
of that inversion**. The runner's shape was selecting for the
defect it should have caught.

This is the fourth instance of one pattern in this exchange
(§44.9, §46, §47): the harness exercised the constructs its own
shape permitted, and the defect lived where that shape could not
reach.

### 49.1 Rule

**A host may run its own code inside the linked ship-tier program,
before the script's entry and after its run.** The generated AOT
entry (§8.1) calls an optional host **pre-entry** function after
`subscript_init` and before `subscript_export_main`, and an
optional **post-run** function after the async pump and before the
Context is released. Both take the Context; neither is bracketed
by `enter_script`/`exit_script`, because they are host code, not
script code.

The hook names are supplied **at build time**, not discovered.
Weak symbols are not portable to `windows-msvc`, which §11c keeps
as a gated configuration, and a link-time default definition would
collide. When no hook is requested the generated entry text is
unchanged, so no existing golden moves — that is the property to
verify, not to assume.

Ownership direction is fixed: the host creates and owns; the
script borrows. Nothing here makes script-owned host state safe,
and no form is added that would.

### 49.2 Surface

The C-AOT runner gains a sibling that takes the two optional
function names alongside the native libraries; the existing
entry points keep their signatures and behavior. The golden
harness can name hooks per corpus entry, so an entry that requires
host-owned state is driven the same way on both tiers.

### 49.3 Corpus

The interop fixture gains a host-owned object with an explicit
create/destroy pair and a borrow-only accessor.
`a128-host-owned-state` (accept): the pre-entry hook creates it,
the script borrows it across **two** separate entry calls so the
lifetime spans more than one call, and the post-run hook destroys
it — byte-identical under both tiers. A companion unit test pins
that a build requesting no hooks emits the entry byte-identically
to the current text.

### 49.4 Parked, now evidenced

The downstream's preferred shape — a session API mirroring
`ReloadSession`, so one harness code path drives both tiers —
requires the ship tier to emit a **loadable library the host
process opens**, not an executable it spawns. That is the
`run --native` loader already parked on the R3 list; R21 is its
first real evidence. Recorded, not scheduled: §49 restores the
gate without it.

### 49.5 Exit criteria (pre-registered)

1. `a128` byte-identical under both tiers, with the borrowed state
   observed across two entry calls.
2. A build requesting no hooks emits the AOT entry unchanged; no
   existing golden moves.
3. Both hooks are optional and independent (pre only, post only,
   both, neither), unit-tested.
4. Full gate green; `tsc` gate green; zero-warning sweep green.
