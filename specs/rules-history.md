# CLAUDE.md — rules history

`CLAUDE.md` states each rule without a date or an origin. This file
holds, per rule, the owner decision date and the evidence or the case
that produced it. An entry is added when a rule lands; the text of the
rule itself is not repeated here. Nothing here is a current rule.

| Rule (CLAUDE.md) | Decided | Evidence and origin |
|---|---|---|
| Non-goal: upstreaming to external projects | 2026-07-27 | A principle, not a scheduling decision. Acceptance and timing of an upstream patch are outside this project's control, and a patch shaped for upstream's other users is a different and larger patch than the one this project needs. |
| Non-goal: standalone runtime — threads removed from the list | 2026-08-02 | The standard library provides Workers (Q35): runtime-owned threads with per-Context isolation and copy-only messaging. The host still owns its main loop. |
| Compiler and oracle: the ship tier is C emission | 2026-07-23 (plan §8 Rev 2) | Cranelift AOT measured 23× a hand-written C baseline; emitted C measured 1.05× with no trap checks and 1.35× with the checks the language requires (`specs/tracking/p4-performance.md`, `specs/tracking/p19-trap-parity.md`). |
| Compiler and oracle: the Cranelift AOT tier is deleted | 2026-08-27 | It was retained as a cross-check and was not one: it shared `lower/func.rs` with the dev JIT, so it was one lowering with a second output sink and could not catch a defect that lowering held. No shipping path used it: `device-link.sh` uses emitted C with the platform clang, and no shipping target lacks a C compiler. The §68 reference interpreter, written from the LIR contract alone, is the independent witness. 350 lines were Cranelift-only; the 2,018 lines of link and tool detection the C tier also needs stayed. |
| Compiler and oracle: an external implementation is a divergence detector, never an oracle | 2026-08-26 | `collisions.md` states in prose where this language differs from JavaScript, and nothing checks that the list is complete. A divergence this project did not decide is a defect that reads as a decision. |
| Writing style: Simplified Technical English | 2026-08-04 | Owner decision. |
| Writing style: a rule carries no date and no origin | 2026-09-12 | `CLAUDE.md` is read on every turn. Thirteen dated notes and their narrative were about a third of it (`specs/tracking/context-size-2026-09-12.md`). |
| Core principles 8–12 | 2026-08-26 | The §68 migration review (`specs/tracking/`, arc of 2026-08-26). |
| Core principle 13: a change needs a stated problem | 2026-09-08 | Of the 26 contract sections after §65, three came from a downstream request. A claim that a downstream request is required was wrong. |
| Core principle 14: a record is not a reason | 2026-09-09 | §95: three divergences from TypeScript were each an exception inside a rule that otherwise followed ECMA, and the first review refused all three by citing the record. |
| Code conventions: formatting under the pinned toolchain | 2026-08-09 | Owner decision. |
| Code conventions: 2,000 lines per Rust source file | 2026-09-12 | `specs/tracking/context-size-2026-09-12.md`; contract §5.y. |
| Workflow: step 0, measure first | 2026-09-08 | Owner decision. |
| Workflow: two review rounds are the limit for a defect class | 2026-08-26 | Owner decision. |
| Hygiene: run `tools/hygiene.sh` once at the end of every Phase Review | 2026-08-30 | Owner decision; the asked shape is one working-tree scan, not a history scan. |
