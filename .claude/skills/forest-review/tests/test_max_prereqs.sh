#!/usr/bin/env bash
# Runs max-prereqs.sh against a fake repo root and stub docker binaries on PATH and checks each prerequisite line.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script="$here/../scripts/max-prereqs.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failures=0

fail() {
    echo "FAIL: $*"
    failures=$((failures + 1))
}

expect_line() {
    grep -qxF -- "$2" <<<"$1" || fail "$3: missing line '$2' in: $(paste -sd '|' <<<"$1")"
}

image=filecoin/lotus-all-in-one:v9.9.9-calibnet
repo="$tmp/repo"
mkdir -p "$repo/scripts/tests/api_compare" "$tmp/snaps" "$tmp/empty-snaps" "$tmp/unnamed-snaps" "$tmp/mainnet-snaps"
printf '# comment\nFOREST_IMAGE=x\nLOTUS_IMAGE="%s" # pinned by CI\nCHAIN=calibnet\n' "$image" >"$repo/scripts/tests/api_compare/.env"
: >"$repo/scripts/tests/api_compare/docker-compose.yml"
touch -t 202601010000 "$tmp/snaps/forest_snapshot_calibnet_2026-01-01_height_300.forest.car.zst"
touch -t 202603010000 "$tmp/snaps/forest_snapshot_calibnet_2025-12-01_height_100.forest.car.zst"
touch -t 202605010000 "$tmp/snaps/forest_snapshot_mainnet_2026-05-01_height_900.forest.car.zst"
touch -t 202607010000 "$tmp/snaps/zz_calibnet.car.zst.part"
: >"$tmp/empty-snaps/notes.txt"
: >"$tmp/empty-snaps/calibnet.car.zst.part"
: >"$tmp/unnamed-snaps/snap.car"
: >"$tmp/mainnet-snaps/forest_snapshot_mainnet_1.car.zst"

mkdir -p "$tmp/ok" "$tmp/bad" "$tmp/nodaemon" "$tmp/none"
cat >"$tmp/ok/docker" <<STUB
#!/usr/bin/env bash
case "\$1" in
info) exit 0 ;;
compose) [ "\$2" = version ] && echo "Docker Compose version v2.30.0" ;;
image) [ "\$2 \$3" = "inspect $image" ] || exit 1 ;;
*) exit 1 ;;
esac
STUB
printf '#!/usr/bin/env bash\necho "permission denied on the socket" >&2\nexit 1\n' >"$tmp/bad/docker"
cat >"$tmp/nodaemon/docker" <<'STUB'
#!/usr/bin/env bash
[ "$1 $2" = "compose version" ] && echo "Docker Compose version v2.30.0" && exit 0
exit 1
STUB
chmod +x "$tmp/ok/docker" "$tmp/bad/docker" "$tmp/nodaemon/docker"
for tool in sed head find sort tail; do
    ln -s "$(command -v "$tool")" "$tmp/none/$tool"
done

run() {
    local bin=$1
    shift
    FOREST_REVIEW_REPO_ROOT="$repo" PATH="$bin" "$BASH" "$script" "$@"
}

out=$(run "$tmp/ok:$PATH" --snapshot-dir "$tmp/snaps") || fail "ok: exited non-zero"
expect_line "$out" "docker: ok" ok
expect_line "$out" "api_compare stack: ok" ok
expect_line "$out" "lotus image: ok ($image)" ok
expect_line "$out" "snapshot: ok ($tmp/snaps/forest_snapshot_calibnet_2025-12-01_height_100.forest.car.zst)" ok
[ "$(wc -l <<<"$out")" -eq 4 ] || fail "ok: expected 4 lines, got $(wc -l <<<"$out")"

out=$(run "$tmp/bad:$PATH" --snapshot-dir "$tmp/empty-snaps") || fail "bad: exited non-zero"
expect_line "$out" "docker: missing (docker info failed: permission denied on the socket)" bad
expect_line "$out" "api_compare stack: missing (install the Docker compose plugin)" bad
expect_line "$out" "lotus image: missing (docker pull $image)" bad
expect_line "$out" "snapshot: missing (no *.car or *.car.zst file in $tmp/empty-snaps; fetch one with forest-tool snapshot fetch --chain calibnet)" bad

out=$(run "$tmp/ok:$PATH" --snapshot-dir "$tmp/unnamed-snaps") || fail "unnamed: exited non-zero"
expect_line "$out" "snapshot: ok ($tmp/unnamed-snaps/snap.car)" unnamed

out=$(cd "$tmp" && run "$tmp/ok:$PATH" --snapshot-dir unnamed-snaps) || fail "relative: exited non-zero"
expect_line "$out" "snapshot: ok ($tmp/unnamed-snaps/snap.car)" relative

out=$(run "$tmp/ok:$PATH" --snapshot-dir "$tmp/mainnet-snaps") || fail "mainnet: exited non-zero"
expect_line "$out" "snapshot: missing (forest_snapshot_mainnet_1.car.zst is not a calibnet snapshot; the api_compare stack runs calibnet)" mainnet

out=$(run "$tmp/nodaemon:$PATH") || fail "nodaemon: exited non-zero"
expect_line "$out" "docker: missing (install Docker and start its daemon)" nodaemon
expect_line "$out" "api_compare stack: ok" nodaemon
expect_line "$out" "lotus image: missing (docker pull $image)" nodaemon
expect_line "$out" "snapshot: missing (pass --snapshot-dir <dir> holding a .car or .car.zst snapshot)" nodaemon

out=$(run "$tmp/none") || fail "no docker: exited non-zero"
expect_line "$out" "docker: missing (install Docker and start its daemon)" none
expect_line "$out" "api_compare stack: missing (install the Docker compose plugin)" none
expect_line "$out" "lotus image: missing (docker pull $image)" none
[ "$(wc -l <<<"$out")" -eq 4 ] || fail "none: expected 4 lines, got $(wc -l <<<"$out")"

rm "$repo/scripts/tests/api_compare/docker-compose.yml" "$repo/scripts/tests/api_compare/.env"
out=$(run "$tmp/ok:$PATH") || fail "no stack files: exited non-zero"
expect_line "$out" "api_compare stack: missing (no scripts/tests/api_compare/docker-compose.yml at this revision)" no-stack
expect_line "$out" "lotus image: missing (no LOTUS_IMAGE in scripts/tests/api_compare/.env)" no-stack

status=0
run "$tmp/ok:$PATH" --bogus >/dev/null 2>&1 || status=$?
[ "$status" -eq 2 ] || fail "unknown argument: expected exit 2, got $status"

if [ "$failures" -ne 0 ]; then
    echo "$failures failure(s)"
    exit 1
fi
echo "ok"
