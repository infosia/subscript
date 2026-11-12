<!-- §260 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 260. The downstream script keeps a changed lockfile

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R01.
Selected by the owner on 2026-11-13.

Problem, read at `8d201f36`. `tools/downstream.sh` installs its exit
trap before the setup, and the cleanup runs
`git -C "$downstream" checkout -- Cargo.lock` on each exit (§82.6
item 5). If the downstream `Cargo.lock` has a change that is not staged
when the script starts, the cleanup replaces it with the index
version. The script removes the developer's change on success, on a
gate failure, and on a later setup failure. §82.6 item 2 already
refuses to write over a `.cargo/config.toml` that the script did not
write. It does not protect `Cargo.lock` in the same way.

### 260.1 Rules

1. Before the script writes a file, it reads the state of `Cargo.lock`
   in the downstream checkout. If `Cargo.lock` has a change that is not
   staged, or a staged change, the script prints one line that names the
   file and exits 2. It changes no file.
2. If `Cargo.lock` is clean, the script continues as in §82.6, and the
   cleanup of §82.6 item 5 restores it. The gate changes only the
   lockfile that the script found clean.
3. §82.6 item 5 is revised: the cleanup restores `Cargo.lock` only after
   rule 1 found it clean.
4. If the downstream directory is not a git checkout, or git does not
   track its `Cargo.lock`, the script prints one line that names the
   cause and exits 2. It changes no file. The cleanup cannot restore an
   untracked lockfile, so the script does not start the gate.
5. If a signal stops the script during the gate, the cleanup runs, and
   the script exits with status 128 plus the signal number. §82.6 item 5
   gives the gate's status, and a stopped gate has none.

### 260.2 Acceptance

1. A test builds a temporary git repository as the downstream checkout,
   with a committed `Cargo.lock` and a `tools/gate.sh` stub that changes
   `Cargo.lock` and exits with a given status. It runs
   `tools/downstream.sh` with `SUBSCRIPT_DOWNSTREAM_DIR` set to it.
   1. Red first, at the pin: with a change to `Cargo.lock` that is not
      staged, the script removes the change.
   2. After the change, the script exits 2, the change stays, and no
      `.cargo/config.toml` exists.
   3. A staged change gives the same result.
   4. A control: with a clean `Cargo.lock`, the script runs the stub,
      exits with its status, restores `Cargo.lock`, and removes
      `.cargo/config.toml`.
   5. Rule 4: an untracked `Cargo.lock`, and a directory that is not a
      git checkout, each give exit 2, one line, and no file change.
   6. Rule 5: a `TERM` signal to the script during a stub that waits
      gives exit 143, and the cleanup runs.
2. The test states its cost. It runs `git` and `sh` only, with no
   build.
3. §82.6 item 5 gets a line that cites this section.
4. No corpus entry or golden changes.

### 260.3 Phase Review

One fresh review of the implementation. No CRITICAL finding.

1. MAJOR, realistic, fixed: acceptance item 3 was open. §82.6 item 5
   now cites this section.
2. MINOR, realistic, fixed: a signal during the gate gave exit 0, as at
   the pin. Rule 5 states the status.
3. MINOR, realistic, fixed: the cleanup did not restore an untracked
   `Cargo.lock`, and the script exited 0. Rule 4 refuses it.
4. MINOR, contrived, fixed: a directory that is not a git checkout gave
   the usage text of `git diff` and a wrong line. Rule 4 gives one line.
5. MINOR, contrived, recorded: §260.4 item 1.

### 260.4 Open

1. If the gate stages `Cargo.lock`, the cleanup restores the file from
   the index, so the staged change stays.
