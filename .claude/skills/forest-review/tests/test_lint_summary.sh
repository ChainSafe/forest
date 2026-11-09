#!/usr/bin/env bash
# Runs lint-summary.sh against a stub mise on PATH and checks stdout and the --out file.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script="$here/../scripts/lint-summary.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
failures=0

fail() {
    echo "FAIL: $*"
    failures=$((failures + 1))
}

cat >"$tmp/bin/mise" <<'STUB'
#!/usr/bin/env bash
case "$1" in
lint:ok) echo "all good" ;;
lint:bad)
    for i in $(seq 1 30); do echo "line $i"; done
    exit 1
    ;;
lint:noisy)
    for i in $(seq 1 60); do echo "err $i" >&2; done
    exit 1
    ;;
lint:untrusted)
    echo "mise ERROR error parsing config file: /w/mise.toml" >&2
    echo "mise ERROR Config files in /w/mise.toml are not trusted." >&2
    echo "Trust them with \`mise trust\`." >&2
    exit 1
    ;;
lint:absent)
    echo "mise ERROR no task lint:absent found" >&2
    echo "Available tasks:" >&2
    for i in $(seq 1 60); do echo "  task$i" >&2; done
    exit 1
    ;;
*)
    echo "untrusted config" >&2
    exit 1
    ;;
esac
STUB
chmod +x "$tmp/bin/mise"

stdout=$(PATH="$tmp/bin:$PATH" "$script" --out "$tmp/full.txt" lint:ok lint:bad) || fail "script exited non-zero"

grep -qx 'lint:ok: pass' <<<"$stdout" || fail "missing 'lint:ok: pass'"
grep -qx 'lint:bad: fail' <<<"$stdout" || fail "missing 'lint:bad: fail'"
[ "$(grep -c '^line ' <<<"$stdout")" -eq 20 ] || fail "expected exactly 20 failure lines on stdout"
grep -qx 'line 20' <<<"$stdout" || fail "missing line 20 on stdout"
if grep -qx 'line 21' <<<"$stdout"; then fail "line 21 leaked to stdout"; fi
if grep -q 'all good' <<<"$stdout"; then fail "passing output leaked to stdout"; fi

[ "$(grep -c '^line ' "$tmp/full.txt")" -eq 30 ] || fail "out file lacks the full 30 lines"
grep -qx '== lint:ok ==' "$tmp/full.txt" || fail "out file lacks lint:ok header"
grep -qx '== lint:bad ==' "$tmp/full.txt" || fail "out file lacks lint:bad header"
grep -qx 'all good' "$tmp/full.txt" || fail "out file lacks passing output"

stdout=$(PATH="$tmp/bin:$PATH" "$script" --out "$tmp/full2.txt" lint:refused) || fail "refusing mise made the script exit non-zero"
grep -qx 'lint:refused: fail' <<<"$stdout" || fail "refused task not reported as fail"
grep -qx 'untrusted config' <<<"$stdout" || fail "refusal output not shown"

stdout=$(PATH="$tmp/bin:$PATH" "$script" --out "$tmp/full4.txt" lint:noisy lint:untrusted lint:absent) || fail "skipped tasks made the script exit non-zero"
grep -qx 'lint:noisy: fail' <<<"$stdout" || fail "stderr-only failure not reported as fail"
[ "$(grep -c '^err ' <<<"$stdout")" -eq 20 ] || fail "stderr excerpt not capped at 20 lines"
grep -qx 'lint:untrusted: skip (mise config not trusted; run: mise trust /w/mise.toml)' <<<"$stdout" || fail "untrusted config not reported as skip"
grep -qx 'lint:absent: skip (task not defined at this revision)' <<<"$stdout" || fail "missing task not reported as skip"
if grep -q 'task1' <<<"$stdout"; then fail "missing task dumped mise's task list"; fi
[ "$(wc -l <<<"$stdout")" -eq 23 ] || fail "expected 23 summary lines, got $(wc -l <<<"$stdout")"
grep -qx '  task60' "$tmp/full4.txt" || fail "out file lacks the missing task's full output"

mkdir -p "$tmp/empty"
for tool in mktemp cat head rm sed grep; do
    ln -s "$(command -v "$tool")" "$tmp/empty/$tool"
done
stdout=$(PATH="$tmp/empty" "$BASH" "$script" --out "$tmp/full3.txt" lint:ok) || fail "missing mise made the script exit non-zero"
grep -qx 'lint:ok: fail' <<<"$stdout" || fail "missing mise not reported as fail"

if [ "$failures" -ne 0 ]; then
    echo "$failures failure(s)"
    exit 1
fi
echo "ok"
