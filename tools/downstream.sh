#!/bin/sh
# The downstream gate against this working tree
# (specs/blocks/compiler.md §82.6). The script patches the downstream
# checkout to this repository's crates, runs the gate, and removes the
# patch:
#
#     SUBSCRIPT_DOWNSTREAM_DIR=<downstream checkout> tools/downstream.sh [options]
#
# SUBSCRIPT_DOWNSTREAM_DIR gives the downstream checkout. The script has
# no default path. Every option goes to the downstream tools/gate.sh.
# Exit 2 for a setup error. In every other case the exit status is the
# gate's status, or 128 plus the signal number (§260). The script is not a CI gate.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

downstream=${SUBSCRIPT_DOWNSTREAM_DIR:-}
if [ -z "$downstream" ] || [ ! -f "$downstream/tools/gate.sh" ]; then
    echo "usage: SUBSCRIPT_DOWNSTREAM_DIR=<downstream checkout> tools/downstream.sh [options]" >&2
    exit 2
fi

config="$downstream/.cargo/config.toml"
if [ -e "$config" ]; then
    echo "downstream: $config exists; the script writes no file over it" >&2
    exit 2
fi

# Compiler contract §260: require a checkout and a tracked lockfile before writes.
if ! git -C "$downstream" rev-parse --show-toplevel >/dev/null 2>&1; then
    echo "downstream: directory is not a git checkout" >&2
    exit 2
fi
if ! git -C "$downstream" ls-files --error-unmatch -- Cargo.lock >/dev/null 2>&1; then
    echo "downstream: git does not track Cargo.lock" >&2
    exit 2
fi

# Compiler contract §260: reject lockfile changes before writes or cleanup.
if ! git -C "$downstream" diff --quiet -- Cargo.lock >/dev/null 2>&1 ||
    ! git -C "$downstream" diff --cached --quiet -- Cargo.lock >/dev/null 2>&1; then
    echo "downstream: Cargo.lock has changes; the script writes no file" >&2
    exit 2
fi

# Compiler contract §260: cleanup restores only a lockfile that was clean.
created_dir=0
owns_config=0
status=1

cleanup() {
    if [ "$owns_config" -eq 1 ]; then
        rm -f "$config"
    fi
    if [ "$created_dir" -eq 1 ]; then
        rmdir "$downstream/.cargo" 2>/dev/null || :
    fi
    git -C "$downstream" checkout -- Cargo.lock || :
}

trap 'trap - 0 HUP INT TERM; cleanup; exit "$status"' 0
trap 'status=129; exit "$status"' HUP
trap 'status=130; exit "$status"' INT
trap 'status=143; exit "$status"' TERM

if [ ! -d "$downstream/.cargo" ]; then
    mkdir -p "$downstream/.cargo"
    created_dir=1
fi

owns_config=1
cat >"$config" <<EOF
[patch."https://github.com/infosia/subscript.git"]
subscript-compiler = { path = "$repo_root/compiler" }
subscript-codegen  = { path = "$repo_root/codegen" }
subscript-bindgen  = { path = "$repo_root/bindgen" }
EOF

head_hash=$(git -C "$repo_root" rev-parse --short HEAD)
if [ -z "$(git -C "$repo_root" status --porcelain)" ]; then
    tree_state=clean
else
    tree_state=dirty
fi
echo "downstream: subscript $head_hash $tree_state"

status=0
(CDPATH= cd -- "$downstream" && tools/gate.sh "$@") || status=$?
exit "$status"
