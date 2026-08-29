#!/bin/sh
# Repository hygiene (CLAUDE.md, "No local or sibling paths in committed
# files" and the privacy rule above it). Run once, at the end of every
# Phase Review:
#
#     tools/hygiene.sh
#
# Scans every tracked file and every untracked file the ignore rules do
# not exclude, and every commit message for an agent session trailer.
# Exit 0 when clean; exit 1 after printing every hit.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

failed=0
file_list=$(mktemp "${TMPDIR:-/tmp}/subscript-hygiene.XXXXXX")
trap 'rm -f "$file_list"' EXIT HUP INT TERM
git ls-files --cached --others --exclude-standard >"$file_list"

# A pattern requires a path component or the form a tool writes, so a
# document that names the pattern (this script, the hygiene record) does
# not match it.
paths='/(Users|home)/[A-Za-z0-9._-]+|/private/(tmp|var)/[A-Za-z0-9._-]+|/var/folders/[A-Za-z0-9._-]+|/tmp/claude[A-Za-z0-9._-]*|(^|[^[:alnum:]])~/[A-Za-z0-9._-]+|[A-Za-z]:[\\/]Users[\\/]'
siblings='\.\./(subscript-typegpu|subscript-gpu|yawgpu|gpuweb|webgpu-native-cts|webgpu-headers|TypeGPU|ts2das|daslang)|(^|[^[:alnum:]])(ts2das|daslang)([^[:alnum:]]|$)'
trailers='Co-Authored-By: Claude <|Generated with \[Claude Code\]|Claude-Session:[[:space:]]*[^[:space:]]|claude\.ai/code/[A-Za-z0-9_-]|<noreply@anthropic\.com>'

while IFS= read -r file; do
    case "$file" in
        tools/hygiene.sh|HANDOFF.md|REPORT.md|node_modules/*|target/*|*/target/*)
            continue
            ;;
    esac
    mode=$(git ls-files -s -- "$file" | sed -n '1s/ .*//p')
    if [ "$mode" = "160000" ]; then
        continue
    fi
    if [ ! -f "$file" ]; then
        continue
    fi
    # Binary files are outside the scan.
    if ! grep -Iq . "$file" 2>/dev/null; then
        continue
    fi
    if grep -nE "$paths" "$file"; then
        echo "hygiene: local path in $file" >&2
        failed=1
    fi
    if grep -nE "$siblings" "$file"; then
        echo "hygiene: sibling or predecessor reference in $file" >&2
        failed=1
    fi
    if grep -nE "$trailers" "$file"; then
        echo "hygiene: agent session trailer in $file" >&2
        failed=1
    fi
done <"$file_list"

# Every commit message, for an agent session trailer. This is the one
# history scan: one `git log` over the messages, not the blobs.
if git log --all --format='%h %B' | grep -nE "$trailers"; then
    echo "hygiene: agent session trailer in a commit message" >&2
    failed=1
fi

exit "$failed"
