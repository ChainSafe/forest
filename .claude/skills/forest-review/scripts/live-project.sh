#!/usr/bin/env bash
# Names and tears down the review-owned compose projects of the `max` live checks.
# Usage: live-project.sh name <round_dir> <check> | live-project.sh down <round_dir> <check>...
# The project is `forest-review-<key>-<check>`, <key> being the basename of <round_dir>/../.. lowercased with characters outside [a-z0-9_-] replaced by `-`.
# `down` removes every container, volume, network, and image labelled with each project (no compose file is loaded, so none can widen it), then prints `<project>: clean`, `<project>: leftover <names>`, or `<project>: skipped (docker not available)`; exits 1 on any leftover.
set -euo pipefail

usage() {
    echo "usage: live-project.sh name <round_dir> <check> | live-project.sh down <round_dir> <check>..." >&2
    exit 2
}

project() {
    local key
    key=$(basename "$key_dir")
    key=$(printf '%s' "$key" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9_-' '-')
    printf 'forest-review-%s-%s\n' "$key" "$1"
}

labelled() {
    docker "$@" --filter "label=com.docker.compose.project=$p" 2>/dev/null || true
}

remove_all() {
    local id
    while IFS= read -r id; do
        if [ -n "$id" ]; then
            docker "$@" "$id" >/dev/null 2>&1 || true
        fi
    done
}

[ "$#" -ge 3 ] || usage
cmd=$1
key_dir=$(cd "$2/../.." 2>/dev/null && pwd) || {
    echo "live-project.sh: no round directory at $2" >&2
    exit 2
}
shift 2

case "$cmd" in
name)
    [ "$#" -eq 1 ] || usage
    project "$1"
    ;;
down)
    status=0
    for check in "$@"; do
        p=$(project "$check")
        if ! command -v docker >/dev/null 2>&1; then
            echo "$p: skipped (docker not available)"
            continue
        fi
        labelled ps -aq | remove_all rm -f
        labelled volume ls -q | remove_all volume rm -f
        labelled network ls -q | remove_all network rm
        labelled image ls -q | remove_all image rm -f
        left=$(
            {
                labelled ps -a --format '{{.Names}}'
                labelled volume ls -q
                labelled network ls --format '{{.Name}}'
                labelled image ls --format '{{.Repository}}:{{.Tag}}'
            } | paste -sd ' ' -
        )
        if [ -z "$left" ]; then
            echo "$p: clean"
        else
            echo "$p: leftover $left"
            status=1
        fi
    done
    exit "$status"
    ;;
*) usage ;;
esac
