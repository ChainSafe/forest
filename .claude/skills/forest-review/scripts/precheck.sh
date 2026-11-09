#!/usr/bin/env bash
# Mechanical checks on the lines a diff adds. Usage: precheck.sh --base <rev> --out <dir>
# Writes <dir>/precheck.txt as `path:line: <check-id>: <message>` (heuristics as `hint:<check-id>`).
# Also writes <dir>/routing.txt (lens names), <dir>/roots.txt (surface TAB entry path touched) and <dir>/bounds.txt (existing limits in src/).
# Exits 0 after a scan whatever it finds; exits 2 on bad arguments or an unresolvable base.
set -euo pipefail

SKIP_PATHS_RE='^\.forest-review/|(^|/)Cargo\.lock$|^docs/openrpc-specs/|^docs/docs/users/reference/cli\.md$|^\.claude/skills/forest-review/tests/fixtures/'
TEST_PATH_RE='(^|/)(tests|benches|test_utils|benchmark_private)/|(^|/)(tests|test_utils|test_helpers)\.rs$|_tests?\.rs$|_tests/'
DECODE_PATH_RE='decod|pars|codec|cbor|rlp|deserial'
DECODE_FN_RE='decod|pars|deserial|from_bytes|from_slice|from_cbor|unmarshal|^read_'
ENV_DOC=docs/docs/users/reference/env_variables.md

# Places that legitimately pin an actor or FVM version; entries ending in `/` are directory prefixes.
VERSIONED_PATH_ALLOW=(
    src/shim/
    src/state_migration/
    src/interpreter/
    src/rpc/registry/
    src/lotus_json/
    src/rpc/methods/state.rs
    src/rpc/types/sector_impl.rs
)

PARITY_PREFIXES=(
    src/chain/
    src/chain_sync/
    src/fil_cns/
    src/beacon/
    src/networks/
    src/lotus_json/
    src/message/
    src/interpreter/
    src/state_manager/
    src/message_pool/
    src/blocks/
    src/shim/
    src/state_migration/
    src/libp2p
    src/rpc/
    src/eth/
)
PERF_PREFIXES=(
    src/rpc/
    src/eth/
    src/chain/
    src/chain_sync/
    src/interpreter/
    src/blocks/
    src/message_pool/
    src/db/
    src/state_manager/
    src/libp2p
)
BOUNDS_RE='const MAX_[A-Z_]+|max_[a-z_]*(size|len|count|range|bytes)|_LIMIT([^A-Za-z0-9_]|$)'

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
EM_DASH=$(printf '\342\200\224')
US=$'\037'

usage() {
    echo "usage: precheck.sh --base <rev> --out <dir>" >&2
    exit 2
}

parse_args() {
    base=""
    out=""
    while [[ $# -gt 0 ]]; do
        case $1 in
        --base)
            [[ $# -ge 2 ]] || usage
            base=$2
            shift 2
            ;;
        --out)
            [[ $# -ge 2 ]] || usage
            out=$2
            shift 2
            ;;
        *) usage ;;
        esac
    done
    [[ -n $base && -n $out ]] || usage
    if ! git rev-parse --verify --quiet "$base^{commit}" >/dev/null; then
        echo "precheck: cannot resolve base '$base'" >&2
        exit 2
    fi
    mkdir -p "$out"
    out=$(cd "$out" && pwd)
}

# Prints `path US line` for every line added since the base, untracked files included.
added_lines() {
    diff_added_lines
    untracked_added_lines
}

untracked_added_lines() {
    local path
    while IFS= read -r -d '' path; do
        [[ $path =~ $SKIP_PATHS_RE ]] && continue
        [[ -f $path && ! -L $path ]] || continue
        PC_PATH="$path" awk -v US="$US" '{ print ENVIRON["PC_PATH"] US NR }' "./$path"
    done < <(git ls-files --others --exclude-standard -z)
}

diff_added_lines() {
    git -c core.quotePath=false diff --unified=0 --find-renames --diff-filter=AMR \
        --no-color --no-ext-diff --no-textconv --no-relative --src-prefix=a/ --dst-prefix=b/ "$base" |
        PC_SKIP="$SKIP_PATHS_RE" awk -v US="$US" '
            BEGIN { skip = ENVIRON["PC_SKIP"] }
            /^diff --git / { path = ""; in_hunk = 0; next }
            !in_hunk && /^\+\+\+ / {
                p = substr($0, 5)
                sub(/\t$/, "", p)
                if (p ~ /^"/) { p = substr(p, 2, length(p) - 2) }
                path = (p ~ /^b\//) ? substr(p, 3) : ""
                if (path ~ skip) { path = "" }
                next
            }
            /^@@ / {
                in_hunk = 1
                split($3, a, ",")
                n = substr(a[1], 2) + 0
                next
            }
            in_hunk && /^\+/ { if (path != "") { print path US n }; n++; next }
            in_hunk && /^ / { n++ }
        '
}

# Prints one record per added line of $1:
# path US line US is_test US in_decode US comment US prose US code US text
classify_file() {
    PC_PATH="$1" PC_TEST_RE="$TEST_PATH_RE" PC_DPATH_RE="$DECODE_PATH_RE" PC_DFN_RE="$DECODE_FN_RE" \
        awk -v US="$US" '
        function hashes(k,   h) { h = ""; while (k-- > 0) { h = h "#" }; return h }

        # Blanks string and char literals, strips comments; sets CODE (text minus comments) and CMT (comment text).
        function sanitize(s,   out, i, n, c, j, term, prev) {
            out = ""; CODE = ""; CMT = ""; i = 1; n = length(s)
            while (i <= n) {
                c = substr(s, i, 1)
                if (in_block) {
                    if (substr(s, i, 2) == "*/") { in_block = 0; CMT = CMT "*/"; i += 2 } else { CMT = CMT c; i++ }
                    continue
                }
                if (in_str) {
                    if (raw_hashes >= 0) {
                        term = "\"" hashes(raw_hashes)
                        if (substr(s, i, length(term)) == term) {
                            in_str = 0; out = out "\""; CODE = CODE term; i += length(term)
                        } else { CODE = CODE c; i++ }
                        continue
                    }
                    if (c == "\\") { CODE = CODE substr(s, i, 2); i += 2; continue }
                    if (c == "\"") { in_str = 0; out = out "\"" }
                    CODE = CODE c; i++
                    continue
                }
                if (substr(s, i, 2) == "//") { CMT = CMT substr(s, i); break }
                if (substr(s, i, 2) == "/*") { in_block = 1; CMT = CMT "/*"; i += 2; continue }
                prev = (i > 1) ? substr(s, i - 1, 1) : ""
                if (c == "r" && (prev == "" || prev == "b" || prev !~ /[A-Za-z0-9_]/) && match(substr(s, i), /^r#*"/)) {
                    raw_hashes = RLENGTH - 2; in_str = 1; out = out "\""
                    CODE = CODE substr(s, i, RLENGTH); i += RLENGTH
                    continue
                }
                if (c == "\"") { in_str = 1; raw_hashes = -1; out = out "\""; CODE = CODE c; i++; continue }
                if (c == "\x27") {
                    if (substr(s, i + 1, 1) == "\\") {
                        j = index(substr(s, i + 3), "\x27")
                        if (j) { out = out "\x27 \x27"; CODE = CODE substr(s, i, 3 + j); i += 3 + j; continue }
                    } else if (substr(s, i + 2, 1) == "\x27") {
                        out = out "\x27 \x27"; CODE = CODE substr(s, i, 3); i += 3
                        continue
                    }
                }
                out = out c; CODE = CODE c; i++
            }
            return out
        }

        # Returns the end position of the last test-gating attribute in s, or 0.
        function test_attr_end(s,   rest, off, a, end) {
            end = 0; off = 0; rest = s
            while (match(rest, /#\[[^]]*\]/)) {
                a = substr(rest, RSTART, RLENGTH)
                if ((a ~ /^#\[cfg\(/ && a ~ /(^|[^A-Za-z0-9_])test([^A-Za-z0-9_]|$)/ && a !~ /not\(test\)/) ||
                    a ~ /^#\[((tokio|async_std)::)?test([](]|$)/ ||
                    a ~ /^#\[(rstest|quickcheck|serial|serial_test::serial)([](]|$)/) {
                    end = off + RSTART + RLENGTH
                }
                off += RSTART + RLENGTH - 1
                rest = substr(rest, RSTART + RLENGTH)
            }
            return end
        }

        # Tracks brace depth from a test attribute to the end of the item it gates; returns 1 for test lines.
        function track_test(san,   line_test, astart, i, n, c) {
            if (san ~ /^[ \t]*#!\[cfg\(test\)\]/) { file_test = 1 }
            line_test = test_active || pending
            astart = test_active ? 0 : test_attr_end(san)
            if (astart) { pending = 1; pend_depth = depth; paren = 0; angle = 0; in_where = 0; line_test = 1 }
            n = length(san)
            for (i = 1; i <= n; i++) {
                c = substr(san, i, 1)
                if (c == "{") {
                    if (pending && i >= astart && paren == 0 && depth == pend_depth) {
                        test_active = 1; test_depth = depth; pending = 0
                    }
                    depth++
                } else if (c == "}") {
                    depth--
                    if (test_active && depth == test_depth) { test_active = 0 }
                    if (pending && depth < pend_depth) { pending = 0 }
                } else if (pending && i >= astart) {
                    if (c == "(" || c == "[") { paren++ }
                    else if (c == ")" || c == "]") { paren-- }
                    else if (c == "<") { angle++ }
                    else if (c == ">" && angle > 0 && substr(san, i - 1, 1) != "-" && substr(san, i - 1, 1) != "=") { angle-- }
                    else if (c == ";" && paren == 0 && depth == pend_depth) { pending = 0 }
                    else if (c == "," && paren == 0 && angle == 0 && !in_where && depth == pend_depth) { pending = 0 }
                    else if (c == "w" && substr(san, i, 5) == "where" && (i == 1 || substr(san, i - 1, 1) !~ /[A-Za-z0-9_]/) && substr(san, i + 5, 1) !~ /[A-Za-z0-9_]/) { in_where = 1 }
                }
            }
            return line_test || file_test
        }

        function split_hash(s,   i) {
            CODE = s; CMT = ""
            if (match(s, /(^|[ \t])#/)) {
                i = RSTART + RLENGTH - 1
                CODE = substr(s, 1, i - 1); CMT = substr(s, i)
            }
        }

        function split_slash(s,   i) {
            CODE = s; CMT = ""
            if (match(s, /(^|[^:])\/\//)) {
                i = (RSTART == 1 && substr(s, 1, 2) == "//") ? 1 : RSTART + 1
                CODE = substr(s, 1, i - 1); CMT = substr(s, i)
            }
        }

        BEGIN {
            path = ENVIRON["PC_PATH"]
            base = path; sub(/.*\//, "", base)
            if (path ~ /\.rs$/) { style = "rs" }
            else if (path ~ /\.(go|js|jsx|ts|tsx|c|h|cc|cpp|proto|sol|java|kt)$/) { style = "slash" }
            else if (path ~ /\.(md|mdx)$/) { style = "md" }
            else if (path ~ /\.(sh|bash|zsh|py|toml|ya?ml|cfg|ini|env|nix|tf|mk)$/ || base ~ /^(Dockerfile|Makefile|\.)/ || path ~ /\.dockerfile$/) { style = "hash" }
            else { style = "plain" }
            path_test = path ~ ENVIRON["PC_TEST_RE"]
            path_decode = path ~ ENVIRON["PC_DPATH_RE"]
            dfn_re = ENVIRON["PC_DFN_RE"]
        }
        NR == FNR { split($0, f, US); if (f[1] == path) { want[f[2]] = 1 }; next }
        {
            text = $0; is_test = path_test; in_decode = path_decode
            if (style == "rs") {
                san = sanitize(text)
                if (match(san, /(^|[^A-Za-z0-9_])fn[ \t]+[A-Za-z_][A-Za-z0-9_]*/)) {
                    fn_name = substr(san, RSTART, RLENGTH); sub(/.*fn[ \t]+/, "", fn_name)
                }
                if (track_test(san)) { is_test = 1 }
                if (fn_name ~ dfn_re) { in_decode = 1 }
                prose = CMT
            } else if (style == "slash") { split_slash(text); prose = CMT }
            else if (style == "hash") { split_hash(text); prose = CMT }
            else if (style == "md") { CODE = ""; CMT = ""; prose = text }
            else { CODE = text; CMT = ""; prose = "" }
            if (FNR in want) {
                print path US FNR US is_test US in_decode US CMT US prose US CODE US text
            }
        }
    ' "$work/added" "./$1"
}

collect_records() {
    added_lines >"$work/added"
    : >"$work/records"
    local path
    while IFS= read -r path; do
        [[ -f $path && ! -L $path ]] || continue
        grep -Iq . "$path" || continue
        classify_file "$path" >>"$work/records"
    done < <(cut -d "$US" -f1 "$work/added" | awk '!seen[$0]++')
}

# scan <check-id> <scope: any|test|nontest|decode> <field: comment|prose|code|text> <path-re> <line-re> <message>
# PC_SKIP_PREFIXES (newline-separated path prefixes) exempts paths; PC_NOT_RE exempts matching lines.
scan() {
    PC_ID="$1" PC_SCOPE="$2" PC_FIELD="$3" PC_PATH_RE="$4" PC_RE="$5" PC_MSG="$6" \
        awk -F "$US" '
        BEGIN {
            col["comment"] = 5; col["prose"] = 6; col["code"] = 7; col["text"] = 8
            c = col[ENVIRON["PC_FIELD"]]; scope = ENVIRON["PC_SCOPE"]
            np = split(ENVIRON["PC_SKIP_PREFIXES"], skip, "\n")
            not_re = ENVIRON["PC_NOT_RE"]
        }
        $1 !~ ENVIRON["PC_PATH_RE"] { next }
        scope == "test" && $3 != 1 { next }
        (scope == "nontest" || scope == "decode") && $3 == 1 { next }
        scope == "decode" && $4 != 1 { next }
        {
            for (i = 1; i <= np; i++) { if (skip[i] != "" && index($1, skip[i]) == 1) { next } }
            if (not_re != "" && $c ~ not_re) { next }
            if ($c ~ ENVIRON["PC_RE"]) { print $1 ":" $2 ": " ENVIRON["PC_ID"] ": " ENVIRON["PC_MSG"] }
        }
    ' "$work/records"
}

check_context_format() {
    # A bare `e`/`err` receiver is an anyhow::Error value, where the formatting is not wasted.
    PC_NOT_RE='(^|[^A-Za-z0-9_.)?])(e|err|error)\.context\(format!' \
        scan context-format any code '\.rs$' '\.context\(format!' \
        'eager format! in .context(); use .with_context(|| format!(...))'
}

check_allow_indexing() {
    scan allow-indexing nontest code '\.rs$' '#!?\[(allow|expect)\([^]]*clippy::indexing_slicing' \
        'indexing_slicing allowed; prefer .get() or show why the index is in bounds'
}

check_file_allow() {
    scan file-allow nontest code '\.rs$' '^#!\[allow\(' \
        'file-level #![allow(...)] in non-test code; scope it to the item that needs it'
}

check_matches_lotus() {
    local finding path rest line
    scan matches-lotus any comment '.' '([Mm]atches|[Ss]ame as) [Ll]otus' \
        'parity claim without a pinned Lotus permalink on it or the next line; cite a tag or SHA link' |
        while IFS= read -r finding; do
            path=${finding%%:*}
            rest=${finding#*:}
            line=${rest%%:*}
            if ! awk -v at="$line" '
                (NR == at || NR == at + 1) && /lotus\/(blob|tree)\// && !/lotus\/(blob|tree)\/(master|main)/ { found = 1 }
                END { exit !found }
            ' "$path"; then
                echo "$finding"
            fi
        done
}

check_latest_tag() {
    scan latest-tag any code '(^|/)\.github/workflows/[^/]+\.ya?ml$|(^|/)Dockerfile[^/]*$|\.dockerfile$' \
        ':latest([^A-Za-z0-9_.-]|$)' \
        'unpinned :latest tag; pin a version or digest'
}

check_lotus_branch_link() {
    scan lotus-branch-link any text '.' 'github\.com/filecoin-project/lotus/(blob|tree)/(master|main)([/#?)]|$)' \
        'Lotus link on a moving branch; pin it to a release tag or commit SHA'
}

check_em_dash() {
    scan em-dash any prose '.' "$EM_DASH" \
        'em-dash in a comment or docs prose; use a comma, colon, parentheses, or period'
}

check_changelog_placeholder() {
    scan changelog-placeholder any text '(^|/)CHANGELOG\.md$' '#[Xx][Xx][Xx]' \
        'placeholder issue number; use the real PR or issue number'
}

check_string_assert() {
    scan string-assert test code '\.rs$' \
        'assert!\(.*(\.to_string\(\)|unwrap_err\(\)|[Ee]rr|[Mm]sg|[Mm]essage).*\.contains\(|(unwrap_err\(\)|[Ee]rr[A-Za-z0-9_]*)(\.to_string\(\))?\.contains\(' \
        'test matches an error message string; downcast the typed error or assert behavior'
}

check_test_sleep() {
    scan test-sleep test code '\.rs$' '(^|[^A-Za-z0-9_])sleep\(' \
        'sleep in a test; wait on the condition or use paused time'
}

check_versioned_path() {
    PC_SKIP_PREFIXES=$(printf '%s\n' "${VERSIONED_PATH_ALLOW[@]}") \
        scan versioned-path nontest code '\.rs$' 'fil_actor_[A-Za-z0-9_]+::v[0-9]+|fvm_shared[234]::|(^|[^A-Za-z0-9_])fvm[234]::' \
        'versioned actor/FVM path outside version-pinning code; go through the src/shim wrappers'
}

check_if_bail() {
    awk -F "$US" '
        function report(path, line) { print path ":" line ": if-bail: if-block holding only bail!; use anyhow::ensure!(condition, ...)" }
        function reset() { state = 0 }
        $1 !~ /\.rs$/ { reset(); next }
        {
            code = $7
            if ($1 != path || $2 != prev + 1) { reset() }
            path = $1; prev = $2
            if (code ~ /^[ \t]*if[ \t]/ && code !~ /^[ \t]*if[ \t]+let[ \t]/ && code ~ /\{[ \t]*(anyhow::)?bail!\(.*\)[ \t]*;?[ \t]*\}[ \t]*$/) {
                report($1, $2); reset(); next
            }
            if (state == 3) {
                if (code ~ /^[ \t]*\}[ \t]*$/) { report(path, if_line) }
                reset()
            }
            if (state == 2) {
                if (code ~ /\)[ \t]*;?[ \t]*$/) { state = 3 }
                next
            }
            if (state == 1) {
                if (code ~ /^[ \t]*(anyhow::)?bail!\(/) {
                    state = (code ~ /\)[ \t]*;?[ \t]*$/) ? 3 : 2
                    next
                }
                reset()
            }
            if (code ~ /^[ \t]*if[ \t]/ && code !~ /^[ \t]*if[ \t]+let[ \t]/ && code ~ /\{[ \t]*$/) { state = 1; if_line = $2 }
        }
    ' "$work/records"
}

check_undocumented_env() {
    PC_DOC="$ENV_DOC" awk -F "$US" '
        BEGIN {
            doc = ENVIRON["PC_DOC"]
            while ((getline line < doc) > 0) {
                while (match(line, /FOREST_[A-Z0-9_]*[A-Z0-9]/)) {
                    documented[substr(line, RSTART, RLENGTH)] = 1
                    line = substr(line, RSTART + RLENGTH)
                }
            }
        }
        $1 !~ /\.rs$/ || $3 == 1 { next }
        {
            rest = $7
            while (match(rest, /"FOREST_[A-Z0-9_]*[A-Z0-9]"/)) {
                name = substr(rest, RSTART + 1, RLENGTH - 2)
                rest = substr(rest, RSTART + RLENGTH)
                if (name ~ /_HEIGHT$/ || (name in documented) || (name in seen)) { continue }
                seen[name] = 1
                print $1 ":" $2 ": undocumented-env: " name " is not in " doc
            }
        }
    ' "$work/records"
}

hint_wire_capacity() {
    awk -F "$US" '
        function literal(a) { return a ~ /^[ \t]*([0-9][0-9_]*([ui](8|16|32|64|size))?|[A-Z][A-Z0-9_]*)[ \t]*$/ }
        $1 !~ /\.rs$/ || $3 == 1 || $4 != 1 { next }
        {
            hit = 0; rest = $7
            while (match(rest, /with_capacity\([^)]*\)/)) {
                arg = substr(rest, RSTART + 14, RLENGTH - 15)
                rest = substr(rest, RSTART + RLENGTH)
                if (!literal(arg)) { hit = 1 }
            }
            rest = $7
            while (match(rest, /vec!\[[^];]*;[^]]*\]/)) {
                arg = substr(rest, RSTART, RLENGTH - 1); sub(/^[^;]*;/, "", arg)
                rest = substr(rest, RSTART + RLENGTH)
                if (!literal(arg)) { hit = 1 }
            }
            if (hit) { print $1 ":" $2 ": hint:wire-capacity: input-sized allocation in a decode path; check the size is bounded before allocating" }
        }
    ' "$work/records"
}

hint_as_cast() {
    scan hint:as-cast decode code '\.rs$' '[ \t)]as[ \t]+(u8|u16|u32|usize|i32|i64)([^A-Za-z0-9_]|$)' \
        'as cast in a decode path; check for truncation or sign change, prefer try_from'
}

hint_permission() {
    scan hint:permission nontest code '\.rs$' 'const[ \t]+PERMISSION[ \t]*:' \
        'RPC method permission set or changed; confirm the level is right'
}

# Prints every changed path since the base, deletions and untracked files included.
changed_paths() {
    git -c core.quotePath=false diff --name-only --no-renames --no-relative -z "$base"
    git ls-files --others --exclude-standard -z
}

path_has_prefix() {
    local path=$1 prefix
    shift
    for prefix in "$@"; do
        [[ $path == "$prefix"* ]] && return 0
    done
    return 1
}

write_routing() {
    local parity=0 perf=0 path
    while IFS= read -r -d '' path; do
        path_has_prefix "$path" "${PARITY_PREFIXES[@]}" && parity=1
        path_has_prefix "$path" "${PERF_PREFIXES[@]}" && perf=1
    done < <(changed_paths)
    {
        echo adversarial
        echo correctness
        echo design
        echo ops
        [[ $parity -eq 1 ]] && echo parity
        [[ $perf -eq 1 ]] && echo perf
        echo tests
    } >"$out/routing.txt"
}

write_roots() {
    local entry_points=${FOREST_REVIEW_ENTRY_POINTS:-$SCRIPT_DIR/../references/entry-points.md}
    : >"$out/roots.txt"
    [[ -f $entry_points ]] || return 0
    local surface="" line entry path
    while IFS= read -r line; do
        if [[ $line =~ ^##[[:space:]]+(.*[^[:space:]])[[:space:]]*$ ]]; then
            surface=${BASH_REMATCH[1]}
        elif [[ -n $surface && $line =~ ^-[[:space:]]+\`([^\`]+)\` ]]; then
            entry=${BASH_REMATCH[1]}
            while IFS= read -r -d '' path; do
                if [[ $path == "$entry"* ]]; then
                    printf '%s\t%s\n' "$surface" "$entry"
                    break
                fi
            done < <(changed_paths)
        fi
    done <"$entry_points" >>"$out/roots.txt"
}

write_bounds() {
    git grep -nIE --no-recurse-submodules --untracked "$BOUNDS_RE" -- src/ |
        awk '{ i = index($0, ":"); rest = substr($0, i + 1); j = index(rest, ":"); text = substr(rest, j + 1); sub(/^[ \t]+/, "", text); print substr($0, 1, i + j) " " text }' \
            >"$out/bounds.txt" || true
}

run_checks() {
    check_context_format
    check_allow_indexing
    check_file_allow
    check_matches_lotus
    check_latest_tag
    check_lotus_branch_link
    check_em_dash
    check_changelog_placeholder
    check_string_assert
    check_test_sleep
    check_versioned_path
    check_if_bail
    check_undocumented_env
    hint_wire_capacity
    hint_as_cast
    hint_permission
}

main() {
    parse_args "$@"
    cd "$(git rev-parse --show-toplevel)"
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    write_routing
    write_roots
    write_bounds
    collect_records
    run_checks | sort -s -t: -k1,1 -k2,2n >"$out/precheck.txt"
    local hints findings
    hints=$(grep -c ': hint:' "$out/precheck.txt" || true)
    findings=$(($(wc -l <"$out/precheck.txt") - hints))
    echo "precheck: $findings findings, $hints hints"
}

main "$@"
