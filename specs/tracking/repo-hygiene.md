# Repository hygiene — sweep record

The rule is CLAUDE.md, "No local or sibling paths in committed files",
plus the privacy rule above it. This file records each full sweep and the
owner's rulings on what the rule does and does not cover, so a later sweep
does not re-litigate a settled item.

## Sweep 2026-07-28 — clean

Run over 573 tracked files, all 265 commits' messages, and the history's
blobs. Everything below returned zero.

| Checked | Scope | Result |
|---|---|---|
| `/Users/`, `/home/…`, `C:\…` absolute paths | tracked files | 0 |
| `/private/tmp`, `/var/folders`, `/opt/homebrew`, `/Applications/` | tracked files | 0 |
| relative paths escaping the repository root | tracked files | 0 |
| predecessor / sibling project names | tracked files | 0 |
| private keys, API keys, tokens, passwords | tracked files | 0 |
| email addresses in file content | tracked files | 0 |
| absolute paths and forbidden names | every commit message | 0 |
| absolute paths and forbidden names added or removed in any blob | all 265 commits, by pickaxe | 0 |

Every `../../` in tracked source lands **inside** the repository — a crate
reaching the root for a sibling directory, which is the repo-relative form
the rule requires. `examples/host/build.sh`, added in this sweep's session,
resolves every path from `dirname $0` and embeds none.

## Sweep 2026-07-29 — clean (incremental)

Run over the 36 commits since the 2026-07-28 sweep (boundary `54a0f84`,
via `git log -p` so content added and later removed is still seen), plus
the full current tree and all commit messages. Same pattern set as
2026-07-28 — absolute/local paths, machine and user names, predecessor
and sibling project names, credentials, email addresses in content —
plus a tracked-filename check (`.env`, key material, credential files).
Everything returned zero; the only matches were this file's own pattern
table. The two rulings below were reported as accepted, not raised.

## Rulings — not violations

Both were raised in the sweep and settled by the owner on 2026-07-28.

**1. `infosia` appears in five places, all as the forked `regress`
dependency's GitHub URL.** The rule *requires* this form: an external
project is cited by upstream URL and pinned by commit, and a fork is
expected to persist (CLAUDE.md non-goals, "Upstreaming to external
projects"). The organisation name coinciding with the owner's email domain
does not make a public repository URL personal data, and removing it would
break the forking rule it satisfies.

**2. The git author identity is in every commit's metadata.** That is
metadata, not file content, and it is unavoidable in git short of
deliberate anonymisation. Rewriting history to remove it is not proposed:
the cost is high, the history has already been rewritten once, and the
identity is the owner's own.

Neither item is a finding. A future sweep reports them as accepted rather
than raising them again.
