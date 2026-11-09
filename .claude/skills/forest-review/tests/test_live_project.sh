#!/usr/bin/env bash
# Runs live-project.sh against stub docker binaries on PATH and checks project names, teardown calls, and leftover reporting.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script="$here/../scripts/live-project.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failures=0

fail() {
    echo "FAIL: $*"
    failures=$((failures + 1))
}

round_dir="$tmp/review/Feat__My.Branch/rounds/3"
mkdir -p "$round_dir" "$tmp/clean" "$tmp/stuck" "$tmp/none"
devnet=forest-review-feat__my-branch-devnet
diff=forest-review-feat__my-branch-differential

cat >"$tmp/clean/docker" <<'STUB'
#!/usr/bin/env bash
echo "$*" >>"$DOCKER_LOG"
STUB
cat >"$tmp/stuck/docker" <<'STUB'
#!/usr/bin/env bash
echo "$*" >>"$DOCKER_LOG"
mine=0
[[ " $* " == *" --filter label=com.docker.compose.project=$STUCK_PROJECT "* ]] && mine=1
case "$1 $2" in
"ps -aq") ((mine)) && echo c1 || echo user-container ;;
"ps -a") ((mine)) && echo stuck-forest-1 || echo user-forest ;;
"volume ls") ((mine)) && echo vol1 || echo user-volume ;;
"network ls") ((mine)) || echo user-network ;;
"image ls") ((mine)) || echo user-image ;;
esac
exit 0
STUB
chmod +x "$tmp/clean/docker" "$tmp/stuck/docker"
for tool in basename tr paste; do
    ln -s "$(command -v "$tool")" "$tmp/none/$tool"
done

out=$("$script" name "$round_dir" devnet) || fail "name exited non-zero"
[ "$out" = "$devnet" ] || fail "name: got '$out', expected '$devnet'"

export DOCKER_LOG="$tmp/clean.log"
out=$(PATH="$tmp/clean:$PATH" "$script" down "$round_dir" differential devnet) || fail "clean down exited non-zero"
[ "$out" = "$diff: clean"$'\n'"$devnet: clean" ] || fail "clean down output: $out"
if grep -q '^compose' "$DOCKER_LOG"; then fail "down loaded a compose model"; fi
grep -qxF "volume ls -q --filter label=com.docker.compose.project=$devnet" "$DOCKER_LOG" || fail "volumes not swept by label"
if grep -q ' rm ' "$DOCKER_LOG"; then fail "removed something with nothing left"; fi

export DOCKER_LOG="$tmp/stuck.log" STUCK_PROJECT=$devnet
status=0
out=$(PATH="$tmp/stuck:$PATH" "$script" down "$round_dir" devnet) || status=$?
[ "$status" -eq 1 ] || fail "leftover: expected exit 1, got $status"
[ "$out" = "$devnet: leftover stuck-forest-1 vol1" ] || fail "leftover output: $out"
grep -qxF "rm -f c1" "$DOCKER_LOG" || fail "leftover container not removed"
grep -qxF "volume rm -f vol1" "$DOCKER_LOG" || fail "leftover volume not removed"
if grep -q 'user-' "$DOCKER_LOG"; then fail "touched an object outside the project: $(grep 'user-' "$DOCKER_LOG" | paste -sd '|' -)"; fi
unfiltered=$(grep -E '^(ps|volume ls|network ls|image ls)' "$DOCKER_LOG" | grep -vF -- "--filter label=com.docker.compose.project=$devnet" || true)
[ -z "$unfiltered" ] || fail "listing without the project label filter: $unfiltered"
for listing in 'ps -aq' 'image ls -q' 'volume ls -q' 'network ls -q'; do
    grep -q "^$listing " "$DOCKER_LOG" || fail "no '$listing' listing"
done

status=0
"$script" name "$tmp/missing/rounds/1" devnet >/dev/null 2>&1 || status=$?
[ "$status" -eq 2 ] || fail "missing round dir: expected exit 2, got $status"

out=$(PATH="$tmp/none" "$BASH" "$script" down "$round_dir" devnet) || fail "no docker exited non-zero"
[ "$out" = "$devnet: skipped (docker not available)" ] || fail "no docker output: $out"

status=0
"$script" down "$round_dir" >/dev/null 2>&1 || status=$?
[ "$status" -eq 2 ] || fail "missing check: expected exit 2, got $status"

if [ "$failures" -ne 0 ]; then
    echo "$failures failure(s)"
    exit 1
fi
echo "ok"
