#!/usr/bin/env bash
# Prints one `<name>: ok|missing (<detail>)` line per prerequisite of the `max` checks. Usage: max-prereqs.sh [--snapshot-dir <dir>]
# The repo root is `git rev-parse --show-toplevel` from the current directory, or FOREST_REVIEW_REPO_ROOT when set (tests).
# The snapshot is the newest `*.car` or `*.car.zst` in the directory whose name carries the stack's CHAIN, else one naming no network.
# Exits 0 whatever is missing; exits 2 on bad arguments.
set -euo pipefail

usage() {
    echo "usage: max-prereqs.sh [--snapshot-dir <dir>]" >&2
    exit 2
}

snapshot_dir=""
while [ "$#" -gt 0 ]; do
    case "$1" in
    --snapshot-dir)
        [ -n "${2:-}" ] || usage
        snapshot_dir=$2
        shift 2
        ;;
    *) usage ;;
    esac
done

root=${FOREST_REVIEW_REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}
stack=scripts/tests/api_compare

env_value() {
    local v
    v=$(sed -n "s/^$1=//p" "$root/$stack/.env" 2>/dev/null | head -n 1 || true)
    v=${v%%#*}
    printf '%s' "${v//[\"\' ]/}"
}

limited() {
    if command -v timeout >/dev/null 2>&1; then
        timeout 15 "$@"
    else
        "$@"
    fi
}

have_docker=0
command -v docker >/dev/null 2>&1 && have_docker=1

if ((have_docker)); then
    status=0
    err=$(limited docker info 2>&1 >/dev/null) || status=$?
    err=${err%%$'\n'*}
    if [ "$status" -eq 0 ]; then
        echo "docker: ok"
    elif [ "$status" -eq 124 ]; then
        echo "docker: missing (docker info timed out after 15s)"
    elif [ -n "$err" ]; then
        echo "docker: missing (docker info failed: $err)"
    else
        echo "docker: missing (install Docker and start its daemon)"
    fi
else
    echo "docker: missing (install Docker and start its daemon)"
fi

if [ ! -f "$root/$stack/docker-compose.yml" ]; then
    echo "api_compare stack: missing (no $stack/docker-compose.yml at this revision)"
elif ((have_docker)) && limited docker compose version >/dev/null 2>&1; then
    echo "api_compare stack: ok"
else
    echo "api_compare stack: missing (install the Docker compose plugin)"
fi

image=$(env_value LOTUS_IMAGE)
if [ -z "$image" ]; then
    echo "lotus image: missing (no LOTUS_IMAGE in $stack/.env)"
elif ((have_docker)) && limited docker image inspect "$image" >/dev/null 2>&1; then
    echo "lotus image: ok ($image)"
else
    echo "lotus image: missing (docker pull $image)"
fi

chain=$(env_value CHAIN)
chain=${chain:-calibnet}
named="" unnamed="" foreign=""
if [ -n "$snapshot_dir" ]; then
    snapshot_dir=$(cd "$snapshot_dir" 2>/dev/null && pwd || printf '%s' "$snapshot_dir")
    for f in "$snapshot_dir"/*.car "$snapshot_dir"/*.car.zst; do
        [ -f "$f" ] || continue
        name=${f##*/}
        if [[ $name == *"$chain"* ]]; then
            [ -z "$named" ] || [ "$f" -nt "$named" ] && named=$f
        elif [[ $name =~ mainnet|calibnet|butterflynet|devnet ]]; then
            foreign=$name
        else
            [ -z "$unnamed" ] || [ "$f" -nt "$unnamed" ] && unnamed=$f
        fi
    done
fi
snapshot=${named:-$unnamed}

if [ -z "$snapshot_dir" ]; then
    echo "snapshot: missing (pass --snapshot-dir <dir> holding a .car or .car.zst snapshot)"
elif [ -n "$snapshot" ]; then
    echo "snapshot: ok ($snapshot)"
elif [ -n "$foreign" ]; then
    echo "snapshot: missing ($foreign is not a $chain snapshot; the api_compare stack runs $chain)"
else
    echo "snapshot: missing (no *.car or *.car.zst file in $snapshot_dir; fetch one with forest-tool snapshot fetch --chain $chain)"
fi
