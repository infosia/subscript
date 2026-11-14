# Tracking: the reload fingerprint holds the absence of a field (§294)

Contract: `specs/blocks/compiler/s294-the-reload-fingerprint-holds-the-absence-of-a-field.md`.
Origin: §261.4 item 2. On 2026-11-15 the owner told Claude to do each
item of a list of open runtime defects.

## Red

At the contract commit `8260d141`, the unit test
`declaration_hash_changes_for_field_absence` failed: the two hashes
were equal (`7486505763817250055`).

The reload of acceptance item 3, measured at the contract commit: the
old code printed `old:absent`, and reload returned `Ok(())`. The next
call of `main` in the new code, which reads `saved.compare` with no
test for `undefined`, ended the test process with `SIGBUS`. So the
defect stops the process, not only a wrong value.

## Form

- `codegen/src/reload.rs`: the text of a field holds the index, the
  name, the type, and `is_absence_capable`. A comment beside it lists
  the facts of `hir::Field` that do not enter the hash, one reason
  each. `hir::Field` is `#[non_exhaustive]` in another crate, so a
  compiler unit test destructures it with no `..`: a new field fails
  the build there.
- `codegen/src/reload/tests.rs`: `declaration_hash_changes_for_field_absence`
  (rule 2) and `declaration_hash_ignores_field_default` (rule 3).
- `codegen/tests/reload.rs`: `field_absence_reload_is_refused`. Reload
  refuses the new code, and the old code prints `old:absent` again.

## Phase Review

One fresh review: no CRITICAL or MAJOR finding, five MINOR findings;
four fixed, one recorded (§294.4 item 1). See §294.3.
