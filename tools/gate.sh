#!/bin/sh
# The round and landing gates (compiler.md section 85).
compiler_baseline=7
runtime_baseline=18
codegen_baseline=13
set -eu

case "${1:-}" in
    quick|full) shape=$1 ;;
    *) echo 'usage: tools/gate.sh <quick|full>'; exit 2 ;;
esac
if [ "$#" -ne 1 ]; then
    echo 'usage: tools/gate.sh <quick|full>'
    exit 2
fi
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"
CARGO=${CARGO:-cargo}
NODE=${NODE:-node}
TSC=${TSC:-node_modules/.bin/tsc}
CC=${CC:-cc}
GIT=${GIT:-git}
record=
scratch=
interrupted=0
cleanup() {
    if [ -n "$scratch" ]; then rm -rf "$scratch"; fi
}
interrupt() {
    trap '' HUP INT TERM
    trap - 0
    if [ -n "$record" ]; then rm -f "$record"; fi
    cleanup
    exit 1
}
trap cleanup 0
# Defer cleanup until resource creation assigns the paths that it owns.
trap 'interrupted=1' HUP INT TERM
mkdir -p target/gate
# Reserve a timestamp so concurrent runs do not replace a record.
while :; do
    stamp=$(date -u +%Y%m%dT%H%M%SZ)
    candidate=target/gate/$stamp-$shape.md
    if (set -C; : >"$candidate") 2>/dev/null; then
        record=$candidate
        break
    fi
    if [ "$interrupted" -ne 0 ]; then interrupt; fi
    if [ ! -e "$candidate" ]; then
        printf 'cannot create %s\n' "$candidate" >&2
        exit 1
    fi
    sleep 1
done
if [ "$interrupted" -ne 0 ]; then interrupt; fi
scratch=$(mktemp -d "${TMPDIR:-/tmp}/subscript-gate.XXXXXX")
trap interrupt HUP INT TERM
if [ "$interrupted" -ne 0 ]; then interrupt; fi
rev=$("$GIT" rev-parse HEAD)
"$GIT" status --porcelain >"$scratch/status"
dirty_count=$(awk 'END { print NR+0 }' "$scratch/status")
state=clean
if [ "$dirty_count" -ne 0 ]; then state=dirty:$dirty_count; fi
# A missing version probe must not prevent the gate steps or the verdict.
version() {
    "$@" || printf 'version unavailable (exit %s): %s\n' "$?" "$*"
}
{
    printf 'shape: %s\nUTC: %s\nrevision: %s\ndirty: %s\n```text\n' "$shape" "$stamp" "$rev" "$dirty_count"
    cat "$scratch/status"
    printf '```\nhost: %s\n' "$(rustc -vV | sed -n 's/^host: //p')"
    printf '```text\n'
    version rustc -V
    version "$CARGO" -V
    version "$NODE" -v
    version "$TSC" -v
    version "$CC" --version | sed -n '1p'
    printf '```\n'
} >>"$record" 2>&1

verdict_status=0
debug=0/0/0
release=0/0/0
debug_skips=0
release_skips=0
release_ran=0
clippy_ran=0
clippy=0/0/0

# Quote shell arguments only when the plain form is not exact.
command_text() {
    separator=
    for arg do
        printf '%s' "$separator"
        case "$arg" in
            *[!a-zA-Z0-9_./:=+-]*|'')
                printf "'"
                printf '%s' "$arg" | sed "s/'/'\\\\''/g"
                printf "'"
                ;;
            *) printf '%s' "$arg" ;;
        esac
        separator=' '
    done
    printf '\n'
}

run() {
    step=$1
    shift
    step_env=none
    start=$(date +%s)
    command_status=0
    if [ "$step" = release ]; then
        release_ran=1
        step_env=SUBSCRIPT_FULL_INTERPRETER_SWEEP=1
        SUBSCRIPT_FULL_INTERPRETER_SWEEP=1 "$@" >"$scratch/stdout" 2>"$scratch/stderr" || command_status=$?
    else
        if [ "$step" = clippy ]; then clippy_ran=1; fi
        "$@" >"$scratch/stdout" 2>"$scratch/stderr" || command_status=$?
    fi
    seconds=$(( $(date +%s) - start ))
    totals=$(awk '/^test result:/ {
        for (i=1; i<NF; i++) {
            if ($(i+1) == "passed;") p += $i
            if ($(i+1) == "failed;") f += $i
            if ($(i+1) == "ignored;") n += $i
        }
    } END { printf "%d/%d/%d", p, f, n }' "$scratch/stdout")
    grep '^gate-skip:' "$scratch/stdout" >"$scratch/skips" || :
    skip_count=$(awk 'END { print NR+0 }' "$scratch/skips")
    step_failed=0
    if [ "$command_status" -ne 0 ]; then step_failed=1; fi
    case "$step" in
        build)
            if grep -Eq '^[[:space:]]*warning(\[|:)' "$scratch/stdout" "$scratch/stderr"; then
                step_failed=1
            fi
            ;;
        debug|release)
            failed=$(printf '%s\n' "$totals" | cut -d/ -f2)
            if [ "$failed" -ne 0 ]; then step_failed=1; fi
            if [ "$step" = debug ]; then
                debug=$totals
                debug_skips=$skip_count
            else
                release=$totals
                release_skips=$skip_count
                if [ "$skip_count" -ne 0 ]; then step_failed=1; fi
            fi
            ;;
        clippy)
            clippy=$(awk '
                /^warning: `subscript-compiler` \(lib\) generated [0-9]+ warning/ { c=$5 }
                /^warning: `subscript-runtime` \(lib\) generated [0-9]+ warning/ { r=$5 }
                /^warning: `subscript-codegen` \(lib\) generated [0-9]+ warning/ { g=$5 }
                END { printf "%d/%d/%d", c, r, g }
            ' "$scratch/stdout" "$scratch/stderr")
            c=$(printf '%s\n' "$clippy" | cut -d/ -f1)
            r=$(printf '%s\n' "$clippy" | cut -d/ -f2)
            g=$(printf '%s\n' "$clippy" | cut -d/ -f3)
            if [ "$c" -gt "$compiler_baseline" ] || [ "$r" -gt "$runtime_baseline" ] || [ "$g" -gt "$codegen_baseline" ]; then
                step_failed=1
            fi
            ;;
    esac
    {
        printf '\n## %s\ncommand: ' "$step"
        command_text "$@"
        printf 'environment: %s\nwall seconds: %s\nexit status: %s\n' "$step_env" "$seconds" "$command_status"
        printf 'tests: %s\ngate-skip count: %s\n```text\n' "$totals" "$skip_count"
        cat "$scratch/skips"
        printf '```\nstdout:\n```text\n'
        cat "$scratch/stdout"
        printf '\n```\nstderr:\n```text\n'
        cat "$scratch/stderr"
        printf '\n```\n'
    } >>"$record"
    cat "$scratch/stdout"
    cat "$scratch/stderr" >&2
    if [ "$step_failed" -ne 0 ]; then verdict_status=1; fi
    return "$step_failed"
}

finish() {
    "$GIT" status --porcelain >"$scratch/final-status"
    awk '
        substr($0, 1, 2) ~ /[MD]/ {
            path=substr($0, 4)
            if (path ~ /^(corpus\/|codegen\/tests\/lir-goldens\/)/ &&
                (path ~ /\.expected$/ || path ~ /(^|\/)golden(s)?(\/|\.|$)/ || path ~ /^codegen\/tests\/lir-goldens\//)) print
        }
    ' "$scratch/final-status" >"$scratch/goldens"
    moved=$(awk 'END { print NR+0 }' "$scratch/goldens")
    {
        printf '\n## Modified or deleted goldens\n```text\n'
        cat "$scratch/goldens"
        printf '```\n'
    } >>"$record"
    verdict="gate $shape $rev $state debug $debug"
    if [ "$release_ran" -ne 0 ]; then verdict="$verdict release $release"; fi
    verdict="$verdict skips $debug_skips"
    if [ "$release_ran" -ne 0 ]; then verdict="$verdict/$release_skips"; fi
    if [ "$clippy_ran" -ne 0 ]; then verdict="$verdict clippy $clippy"; fi
    verdict="$verdict goldens-moved $moved exit $verdict_status"
    printf '%s\n' "$verdict" >>"$record"
    printf '%s\n%s\n' "$record" "$verdict"
    exit "$verdict_status"
}

run fmt "$CARGO" fmt --check || finish
run build "$CARGO" build --offline --locked --workspace --all-targets || finish
if ! run debug "$CARGO" test --offline --locked --workspace --no-fail-fast; then
    if [ "$shape" = quick ]; then finish; fi
fi
if [ "$shape" = full ]; then
    run release "$CARGO" test --offline --locked --workspace --no-fail-fast --release || :
    run clippy "$CARGO" clippy --offline --locked --workspace --all-targets || :
    run tsc "$TSC" -p tsconfig.json || :
    run hygiene tools/hygiene.sh || :
fi
finish
