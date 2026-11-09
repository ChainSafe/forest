#!/usr/bin/env bash
# Builds a throwaway repo from fixtures/precheck/{base,change} and checks precheck.sh output line by line.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
precheck="$here/../scripts/precheck.sh"
fixtures="$here/fixtures/precheck"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
repo="$tmp/repo"
out_dir="$tmp/out"
report="$out_dir/precheck.txt"
failures=0

g() {
    git -C "$repo" -c user.name=t -c user.email=t@t "$@"
}

fail() {
    echo "FAIL: $*"
    failures=$((failures + 1))
}

has_prefix() {
    awk -v p="$1" 'index($0, p) == 1 { found = 1 } END { exit !found }' "$report"
}

expect() {
    has_prefix "$1" || fail "missing '$1'"
}

reject() {
    if has_prefix "$1"; then
        fail "unexpected '$1'"
    fi
}

mkdir -p "$repo"
g init -q -b main
g config commit.gpgsign false
g config tag.gpgsign false
cp -R "$fixtures/base/." "$repo/"
g add -A
g commit -q -m base
base=$(g rev-parse HEAD)

cp -R "$fixtures/change/." "$repo/"
g mv src/old_name.rs src/new_name.rs
g rm -q src/gone.rs
mkdir -p "$repo/assets"
printf 'x\000y https://github.com/filecoin-project/lotus/blob/master/a.go\n' >"$repo/assets/blob.bin"
mkdir -p "$repo/.claude/skills/forest-review/tests/fixtures/x"
printf 'pub fn f() { sleep(1); }\n// Fixture \342\200\224 never scanned.\n' >"$repo/.claude/skills/forest-review/tests/fixtures/x/a.rs"
g add -A
g add -f .claude
g commit -q -m change
printf 'Uncommitted \342\200\224 still scanned.\n' >>"$repo/docs/guide.md"
printf 'pub fn u() {}\n// Untracked \342\200\224 still scanned.\n' >"$repo/src/untracked.rs"
printf '{"see": "https://github.com/filecoin-project/lotus/blob/master/a.go"}\n' >"$repo/docs/openrpc-specs/new.json"
printf 'Ignored \342\200\224 never scanned.\n' >"$repo/ignored.md"
mkdir -p "$repo/.forest-review/pr-1/worktree/src"
printf '// Review state \342\200\224 never scanned.\n' >"$repo/.forest-review/pr-1/worktree/src/x.rs"
echo ignored.md >>"$repo/.git/info/exclude"

set +e
stdout=$(cd "$repo" && bash "$precheck" --base "$base" --out "$out_dir")
status=$?
set -e
[[ $status -eq 0 ]] || fail "exit status $status"
[[ -f $report ]] || {
    echo "FAIL: no $report"
    exit 1
}

expected=(
    "src/foo/mod.rs:9: context-format:"
    "src/foo/mod.rs:57: context-format:"
    "src/foo/mod.rs:25: allow-indexing:"
    "src/lib.rs:1: file-allow:"
    "src/foo/mod.rs:10: matches-lotus:"
    ".github/workflows/ci.yml:5: latest-tag:"
    "Dockerfile:1: latest-tag:"
    "src/foo/mod.rs:11: lotus-branch-link:"
    "src/foo/mod.rs:14: em-dash:"
    ".github/workflows/ci.yml:6: em-dash:"
    "CHANGELOG.md:5: em-dash:"
    "docs/guide.md:3: em-dash:"
    "docs/guide.md:4: em-dash:"
    "src/untracked.rs:2: em-dash:"
    "CHANGELOG.md:4: changelog-placeholder:"
    "src/foo/mod.rs:60: string-assert:"
    "tests/it.rs:7: string-assert:"
    "src/foo/mod.rs:58: test-sleep:"
    "tests/it.rs:5: test-sleep:"
    "src/foo/mod.rs:78: test-sleep:"
    "src/foo/mod.rs:84: test-sleep:"
    "src/foo/mod.rs:91: test-sleep:"
    "src/foo/mod.rs:101: test-sleep:"
    "src/foo/mod.rs:111: test-sleep:"
    "src/foo/mod.rs:16: versioned-path:"
    "src/foo/mod.rs:119: if-bail:"
    "src/foo/mod.rs:122: if-bail:"
    "src/foo/mod.rs:128: if-bail:"
    "src/foo/mod.rs:17: undocumented-env:"
    "src/foo/mod.rs:32: hint:wire-capacity:"
    "src/foo/mod.rs:35: hint:wire-capacity:"
    "src/foo/mod.rs:34: hint:as-cast:"
    "src/foo/mod.rs:37: hint:as-cast:"
    "src/foo/mod.rs:46: hint:permission:"
)
for e in "${expected[@]}"; do
    expect "$e"
done

# Unchanged base lines.
reject "src/foo/mod.rs:3:"
reject "CHANGELOG.md:3:"
# Lotus links pinned to a tag or a SHA.
reject "src/foo/mod.rs:12:"
reject "src/foo/mod.rs:13:"
# Em-dash in a string literal, not a comment.
reject "src/foo/mod.rs:15:"
# Devnet height override and a documented name.
reject "src/foo/mod.rs:18:"
reject "src/foo/mod.rs:19:"
# sleep and string asserts outside test code, including after the test module closes.
reject "src/foo/mod.rs:20:"
reject "src/foo/mod.rs:21:"
reject "src/foo/mod.rs:67:"
# Literal capacity and a non-decode function.
reject "src/foo/mod.rs:33:"
reject "src/foo/mod.rs:42:"
# Versioned path and env name inside the test module.
reject "src/foo/mod.rs:61:"
reject "src/foo/mod.rs:62:"
# Parity claim next to a pinned link; .context on an error value, which is not eager.
reject "src/foo/mod.rs:71:"
reject "src/foo/mod.rs:73:"
# Env name in a test-gated generic helper; code after the generic test items.
reject "src/foo/mod.rs:85:"
reject "src/foo/mod.rs:96:"
reject "src/foo/mod.rs:115:"
# Inner (non file-level) allow, test-file allow, non-error contains, ubuntu-latest runner.
reject "src/lib.rs:5:"
reject "tests/it.rs:1:"
reject "tests/it.rs:8:"
reject ".github/workflows/ci.yml:4:"
for path in src/shim/ src/state_migration/ src/interpreter/fvm3.rs src/rpc/registry/ src/lotus_json/ \
    Cargo.lock docs/openrpc-specs/ docs/docs/users/reference/cli.md \
    src/new_name.rs src/old_name.rs src/gone.rs assets/ ignored.md .claude/skills/forest-review/tests/fixtures/ .forest-review/; do
    reject "$path"
done

while IFS= read -r line; do
    known=0
    for e in "${expected[@]}"; do
        if [[ $line == "$e"* ]]; then
            known=1
            break
        fi
    done
    [[ $known -eq 1 ]] || fail "unexpected line: $line"
done <"$report"

[[ $stdout == "precheck: 29 findings, 5 hints" ]] || fail "stdout was '$stdout'"

set +e
(cd "$repo" && bash "$precheck" --base no-such-ref --out "$tmp/bad") >/dev/null 2>&1
bad_status=$?
set -e
[[ $bad_status -eq 2 ]] || fail "unresolved base exited $bad_status, want 2"

# Routing, entry roots and bound inventory, each from a throwaway repo.
entry_points="$tmp/entry-points.md"
# shellcheck disable=SC2016
printf '# Entry points\n\n## F3 sidecar\n\n- `f3-sidecar/`\n\n## RPC\n\n- `src/rpc/`\n' >"$entry_points"

scenario() {
    local dir="$tmp/sc-$1"
    shift
    mkdir -p "$dir/repo/src"
    printf 'pub const MAX_FOO: usize = 4;\n' >"$dir/repo/src/limits.rs"
    printf '# readme\n' >"$dir/repo/README.md"
    git -C "$dir/repo" init -q -b main
    git -C "$dir/repo" config commit.gpgsign false
    git -C "$dir/repo" config tag.gpgsign false
    git -C "$dir/repo" -c user.name=t -c user.email=t@t add -A
    git -C "$dir/repo" -c user.name=t -c user.email=t@t commit -q -m base
    local f
    for f in "$@"; do
        mkdir -p "$dir/repo/$(dirname "$f")"
        echo "x" >>"$dir/repo/$f"
    done
    (cd "$dir/repo" && FOREST_REVIEW_ENTRY_POINTS="$entry_points" bash "$precheck" --base HEAD --out "$dir/out") >/dev/null
    sc_out="$dir/out"
}

always=$'adversarial\ncorrectness\ndesign\nops\ntests'

scenario docs README.md
[[ $(cat "$sc_out/routing.txt") == "$always" ]] || fail "README-only routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"
[[ ! -s $sc_out/roots.txt ]] || fail "README-only roots not empty"

scenario lock Cargo.lock
[[ $(cat "$sc_out/routing.txt") == "$always" ]] || fail "Cargo.lock-only routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"

scenario rpc src/rpc/methods/eth.rs
want=$'adversarial\ncorrectness\ndesign\nops\nparity\nperf\ntests'
[[ $(cat "$sc_out/routing.txt") == "$want" ]] || fail "rpc routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"
grep -qxF $'RPC\tsrc/rpc/' "$sc_out/roots.txt" || fail "rpc root missing"

scenario shim src/shim/x.rs
want=$'adversarial\ncorrectness\ndesign\nops\nparity\ntests'
[[ $(cat "$sc_out/routing.txt") == "$want" ]] || fail "shim routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"

scenario beacon src/beacon/x.rs
want=$'adversarial\ncorrectness\ndesign\nops\nparity\ntests'
[[ $(cat "$sc_out/routing.txt") == "$want" ]] || fail "beacon routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"

scenario chain src/chain/store.rs
want=$'adversarial\ncorrectness\ndesign\nops\nparity\nperf\ntests'
[[ $(cat "$sc_out/routing.txt") == "$want" ]] || fail "chain routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"

scenario blocks src/blocks/x.rs
[[ $(cat "$sc_out/routing.txt") == "$want" ]] || fail "blocks routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"

scenario db src/db/x.rs
want=$'adversarial\ncorrectness\ndesign\nops\nperf\ntests'
[[ $(cat "$sc_out/routing.txt") == "$want" ]] || fail "db routing: $(tr '\n' ' ' <"$sc_out/routing.txt")"

scenario f3 f3-sidecar/x.go
[[ $(cat "$sc_out/roots.txt") == $'F3 sidecar\tf3-sidecar/' ]] || fail "f3 roots: $(cat "$sc_out/roots.txt")"
grep -qxF 'src/limits.rs:1: pub const MAX_FOO: usize = 4;' "$sc_out/bounds.txt" || fail "bounds missing MAX_FOO: $(cat "$sc_out/bounds.txt")"

scenario noentry README.md
rm -f "$sc_out/roots.txt"
(cd "$tmp/sc-noentry/repo" && FOREST_REVIEW_ENTRY_POINTS="$tmp/missing.md" bash "$precheck" --base HEAD --out "$sc_out") >/dev/null
[[ -f $sc_out/roots.txt && ! -s $sc_out/roots.txt ]] || fail "missing entry-points should give empty roots.txt"

if [[ $failures -gt 0 || -n ${SHOW_REPORT:-} ]]; then
    echo "--- precheck.txt"
    cat "$report"
fi
if [[ $failures -gt 0 ]]; then
    echo "$failures failure(s)"
    exit 1
fi
echo "ok: ${#expected[@]} expected lines, no extras"
