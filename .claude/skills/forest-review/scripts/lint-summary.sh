#!/usr/bin/env bash
# Runs `mise <task>` for each task and prints a compact summary. Usage: lint-summary.sh --out <file> <task>...
# <file> receives each task's full output under a `== <task> ==` header; failures also show the first 20 lines on stdout.
# A task mise refuses to run (untrusted config, task absent at this revision) is reported as `skip`, not `fail`.
# Exits 0 whatever the tasks do; exits 2 on bad arguments.
set -euo pipefail

usage() {
    echo "usage: lint-summary.sh --out <file> <task>..." >&2
    exit 2
}

[ "${1:-}" = "--out" ] && [ -n "${2:-}" ] || usage
out=$2
shift 2
[ "$#" -gt 0 ] || usage

: >"$out"
for task in "$@"; do
    log=$(mktemp)
    status=0
    mise "$task" >"$log" 2>&1 || status=$?
    {
        echo "== $task =="
        cat "$log"
    } >>"$out"
    untrusted=$(sed -n '/are not trusted/{s/.*Config files in \(.*\) are not trusted.*/\1/p;q;}' "$log")
    if [ "$status" -eq 0 ]; then
        echo "$task: pass"
    elif [ -n "$untrusted" ]; then
        echo "$task: skip (mise config not trusted; run: mise trust $untrusted)"
    elif grep -qE "no task [^ ]+ found" "$log"; then
        echo "$task: skip (task not defined at this revision)"
    else
        echo "$task: fail"
        head -n 20 "$log"
    fi
    rm -f "$log"
done
