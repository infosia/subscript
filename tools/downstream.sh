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
# gate's status. The script is not a CI gate.
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

# The script owns the patch file from this point. The two exits above
# change nothing, so they need no cleanup.
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

trap 'trap - EXIT HUP INT TERM; cleanup; exit "$status"' EXIT HUP INT TERM

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
