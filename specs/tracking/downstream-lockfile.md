# Downstream lockfile — compiler contract §260

The script must keep staged and unstaged changes to `Cargo.lock`.
The script must reject these changes before it writes a file or installs the cleanup trap.

Red: `cargo test --offline --locked -p subscript-cli --test downstream_script unstaged_change_is_kept` failed.
The script replaced `# developer lockfile` with `# committed lockfile`.
The test took 0.38 seconds. The command took 2.07 seconds, with compilation.

The script now checks the worktree and index before it installs the cleanup trap.
A changed lockfile causes one diagnostic line and exit status 2.
The clean path keeps the gate status and restores the lockfile.

`cargo test --offline --locked -p subscript-cli --test downstream_script`:
4 passed, 0 failed; test time 0.41 seconds; command time 0.92 seconds.
The unstaged and staged cases keep both the file content and the index content.
The clean controls run the stub with exit statuses 0 and 7.
Each case leaves no `.cargo/config.toml` or `.cargo` directory.

Test cost: four temporary git repositories and four shell runs, with no downstream build.
The measured command time includes compilation of the Rust test.

`cargo fmt --check`: passed; command time 2.09 seconds.

`cargo clippy --offline --locked -p subscript-cli --all-targets`: passed; command time 4.59 seconds.
Clippy reported no warning in the changed files.

Rule 4: the script first requires a git checkout and a tracked `Cargo.lock`.
Each git probe sends its output to `/dev/null`.
The rejection gives one cause line and exit status 2 before any write.
Tests cover an untracked lockfile, an absent untracked lockfile, and a directory without a git checkout.

Rule 5: separate POSIX shell traps set statuses 129, 130, and 143 for HUP, INT, and TERM.
The exit trap runs the cleanup with that status.
The TERM test stops the script and the stub's sleep child.
It checks status 143, the restored lockfile, and removal of `.cargo`.
The fixture ignores removal errors in `Drop`.

Test cost: eight temporary git repositories and eight shell runs, with no downstream build.
The TERM stub requests a five-second wait; the test stops that wait early.
`cargo test --offline --locked -p subscript-cli --test downstream_script`:
8 passed, 0 failed; test time 0.31 seconds; command time 0.88 seconds.
`cargo fmt --check`: passed; command time 2.01 seconds.
`cargo clippy --offline --locked -p subscript-cli --all-targets`: passed; command time 0.25 seconds.
Clippy reported no warning in the changed files.
