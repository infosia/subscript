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

## Sweep 2026-08-02 — clean (incremental)

Boundary `d5f3d2b` (the 2026-07-29 record) to `bb78eb6`: 66 commits,
scanned with `git log -p` so content added and later removed is still
seen, plus every commit message in the range, the full current tree, and
the uncommitted working-tree diff. Same pattern set as the two earlier
sweeps.

| Checked | Scope | Result |
|---|---|---|
| `/Users/`, `/home/…`, `C:\…`, `\\?\…`, `%AppData%`, `OneDrive`, the owner's account name | 66 commits' diffs, all their messages, current tree, pending diff | 0 |
| `/private/tmp`, `/private/var`, `/var/folders`, `/opt/homebrew`, `/Applications/` | same | 0 |
| relative paths escaping the repository root | current tree | 0 — the deepest, `../../../` from `benchmarks/src/bin/` and `codegen/tests/native-fixture/`, reaches the repository root and stops |
| private keys, API keys, tokens, passwords | current tree | 0 |
| email addresses in file content | current tree | 0 |
| credential-shaped tracked filenames (`.env`, `*.pem`, `id_rsa`, `*.p12`, `*.keystore`) | current tree | 0 |

`infosia` still appears in exactly the five ruled-on places (the forked
`regress` URL in two `Cargo.toml`s, `Cargo.lock`'s pinned commit,
`stdlib.md`, `p23-regex.md`) plus this file's ruling text. Reported as
accepted under ruling 1, not raised.

Scope limit, stated rather than implied: the project-name check is
pattern-based and can only match names it is given. It confirms no
absolute or escaping path and no new external name of the forms above
entered the history; it is not a proof that some unnamed sibling project
is unmentioned.

## Sweep 2026-08-10 — clean, one item raised (incremental)

Boundary `bb78eb6` (the 2026-08-02 record) to `585e073`: 79
commits, scanned with `git log -p` so content added and later
removed is still seen, plus every commit message in the range, the
full current tree, the staged diff of `585e073`, and the untracked
files it added. Same pattern set as the three earlier sweeps.

| Checked | Scope | Result |
|---|---|---|
| `/Users/`, `/home/…`, `C:\…`, `\\?\…`, `%AppData%`, `OneDrive`, the owner's account name | 79 commits' diffs, all their messages, current tree, the staged and untracked additions | 0 |
| `/private/tmp`, `/private/var`, `/var/folders`, `/opt/homebrew`, `/Applications/`, the session scratch directory | same | 0 |
| relative paths escaping the repository root | current tree | 0 — the deepest, `../../../` from `benchmarks/src/bin/` and `codegen/tests/native-fixture/`, reaches the repository root and stops |
| private keys, API keys, tokens, passwords | current tree | 0 |
| email addresses in file content | current tree | 0 |
| credential-shaped tracked filenames (`.env`, `*.pem`, `id_rsa`, `*.p12`, `*.keystore`) | current tree | 0 |

The `codegen/tests/archive-fixture` crate that `585e073` adds
embeds no path. Its build script resolves the header directory
from `CARGO_MANIFEST_DIR` and the archive from `OUT_DIR`, and
passes both to the test through `cargo:rustc-env`.

`infosia` still appears in exactly the five ruled-on places.
Reported as accepted under ruling 1, not raised.

**Raised, and open.** `bindgen/tests/provenance.rs` spells its
synthetic descriptors with the `SGPU`/`sgpu` prefix, in about 30
places, added inside this range. The prefix is not a name this
project defines anywhere else. The owner must rule whether it
stays. The sweep does not change it.

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

## The sweep is a test — 2026-08-30

*(Owner: "このチェックは自動化されていますか？…同等のものを作ってもよいです".
Until this date every sweep above was run by hand.)*

`compiler/tests/hygiene.rs` runs under `cargo test` on every host,
including the Windows gate, and fails on the first hit. It scans:

1. **Every tracked file, and every untracked file the ignore rules do
   not exclude**, as text; a file that is not valid UTF-8 is skipped
   as binary. `node_modules/`, `target/`, `HANDOFF.md`, and `REPORT.md`
   are outside the scan, because they are never committed.
2. **Every commit message in the whole history** (`git log --all
   --format=%B`).
3. **Every blob in the whole history** for the same patterns, by the
   fastest form the round measures under 30 s on the reference
   machine (`git grep` over `git rev-list --all`, or one `git log -S`
   per pattern); the form and its time are recorded here.

Patterns, each written so that a bare mention of the pattern's own
text — this table, a rule in CLAUDE.md, the test's source — does not
match, because each requires a path component or a trailer value
after it:

| Class | Pattern |
|---|---|
| a home directory | `/Users/<name>`, `/home/<name>`, `C:\Users\<name>`, `~/<component>` |
| a temporary directory of one machine | `/private/tmp/<component>`, `/private/var/<component>`, `/var/folders/<component>`, `/tmp/claude<anything>` |
| a sibling or predecessor checkout | `../subscript-typegpu`, `../yawgpu`, `../ts2das`, the words `ts2das` and `daslang` |
| an agent session trailer or link | `Co-Authored-By: Claude`, `Generated with Claude Code`, `Claude-Session:`, `claude.ai/code/`, `noreply@anthropic.com` |

Not scanned, by the rulings above: the git author identity, and the
`github.com/infosia/` fork URLs.

The test names every hit with its file and line, or its commit hash,
before it fails. A test that builds a violating file in a temporary
directory and runs the scanner over it pins that the scanner fails
(core principle 9); the scanner is one function over a list of paths,
so the test reaches it without touching the repository.
