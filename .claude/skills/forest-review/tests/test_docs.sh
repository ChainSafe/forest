#!/usr/bin/env bash
# Structural checks for every Markdown file of the forest-review skill.
set -euo pipefail

skill_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
repo_root=$(git -C "$skill_dir" rev-parse --show-toplevel)
fixtures=$skill_dir/tests/fixtures
refs=$skill_dir/references
em_dash=$'\xe2\x80\x94'
bt='`'
failures=0

fail() {
    echo "FAIL: $*"
    failures=$((failures + 1))
}

path_exists() {
    local p=$1
    [[ -e $repo_root/$p || -e $skill_dir/$p ]] && return 0
    [[ -n $(git -C "$repo_root" ls-files -- "$p" | head -1) ]]
}

strip_location() {
    local p=${1%%#*}
    if [[ $p =~ ^(.*[^:]):[0-9].*$ ]]; then
        p=${BASH_REMATCH[1]}
    fi
    printf '%s' "$p"
}

check_em_dash_and_markers() {
    local f=$1 hits
    hits=$(grep -nF "$em_dash" "$f" || true)
    [[ -z $hits ]] || fail "$f: em-dash: $hits"
    hits=$(grep -nwE 'TODO|TBD' "$f" || true)
    [[ -z $hits ]] || fail "$f: placeholder marker: $hits"
}

check_repo_paths() {
    local f=$1 token p
    while IFS= read -r token; do
        token=${token#\`}
        token=${token%\`}
        [[ $token =~ ^(src/|scripts/|docs/|f3-sidecar/|\.github/) ]] || continue
        [[ $token =~ [[:space:]\<\{\$] ]] && continue
        p=$(strip_location "$token")
        path_exists "$p" || fail "$f: path does not exist: $token"
    done < <(grep -oE "${bt}[^${bt}]+${bt}" "$f" || true)
}

check_contents_list() {
    local f=$1 lines head
    [[ $f == "$skill_dir/SKILL.md" ]] && return 0
    lines=$(wc -l <"$f")
    ((lines > 100)) || return 0
    head=$(awk '
        NR == 1 && /^---$/ { fm = 1; next }
        fm && /^---$/ { fm = 0; next }
        fm { next }
        /^# / || /^[[:space:]]*$/ { next }
        { print; if (++n == 2) exit }
    ' "$f")
    [[ $head =~ ^(\#\#[[:space:]]+)?Contents:?$'\n'-[[:space:]] ]] ||
        fail "$f: $lines lines but does not start with a contents list (heading, then a list item)"
}

check_known_deviations() {
    local f=$refs/known-deviations.md entry="" has_link=0 has_verified=0 line ref
    [[ -f $f ]] || return 0
    finish_entry() {
        [[ -z $entry ]] && return 0
        ((has_link)) || fail "$f: entry '$entry' has no Lotus permalink"
        ((has_verified)) || fail "$f: entry '$entry' has no verified: YYYY-MM-DD line"
    }
    while IFS= read -r line; do
        if [[ $line =~ ^\#\#[[:space:]]+(.+)$ ]]; then
            finish_entry
            entry=${BASH_REMATCH[1]}
            has_link=0
            has_verified=0
            continue
        fi
        [[ $line =~ verified:[[:space:]]*[0-9]{4}-[0-9]{2}-[0-9]{2} ]] && has_verified=1
        [[ $line == *github.com/filecoin-project/lotus/* ]] && has_link=1
    done <"$f"
    finish_entry
    while IFS= read -r ref; do
        ref=${ref##*/}
        [[ $ref =~ ^(v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?|[0-9a-f]{40})$ ]] ||
            fail "$f: Lotus permalink not pinned to a tag or 40-hex SHA: $ref"
    done < <(grep -oE 'github\.com/filecoin-project/lotus/(blob|tree)/[^/[:space:])]+' "$f" || true)
}

check_entry_points() {
    local f=$refs/entry-points.md headings expected line p
    [[ -f $f ]] || return 0
    headings=$(grep -E '^## ' "$f" | sed 's/^## //' | paste -sd '|' - || true)
    expected='p2p|RPC|Files and FFI|F3 sidecar|drand'
    [[ $headings == "$expected" ]] || fail "$f: surfaces are '$headings', expected '$expected'"
    while IFS= read -r line; do
        [[ $line =~ ^-[[:space:]]+\`([^\`]+)\` ]] || continue
        p=${BASH_REMATCH[1]}
        path_exists "$p" || fail "$f: entry does not exist: $p"
        if [[ -d $repo_root/$p && $p != */ ]]; then
            fail "$f: directory entry must end with /: $p"
        fi
    done <"$f"
}

check_prompts() {
    local f=$refs/prompts.md heading bad
    [[ -f $f ]] || return 0
    for heading in 'Lens agent' 'No-checklist reviewer' 'PR and commit checker' 'Skeptic' 'Mutation runner'; do
        grep -qxF "## $heading" "$f" || fail "$f: missing heading '## $heading'"
    done
    bad=$(grep -oE '\{[a-z_]+\}' "$f" | sort -u | grep -vxE '\{(round_dir|lens_file|worktree|lotus_rev|lotus_master|lotus_src|skill_dir)\}' || true)
    [[ -z $bad ]] || fail "$f: unknown placeholders: $(echo "$bad" | paste -sd ' ' -)"
    bad=$(grep -nF "\${CLAUDE_SKILL_DIR}" "$f" || true)
    [[ -z $bad ]] || fail "$f: subagents do not expand \${CLAUDE_SKILL_DIR}, use {skill_dir}: $bad"
    grep -qF '{lens_file}' "$f" || fail "$f: {lens_file} unused"
    local ph
    for ph in '{lotus_rev}' '{lotus_master}' '{lotus_src}'; do
        grep -qF "$ph" "$f" || fail "$f: $ph unused"
    done
    local complete
    complete=$(awk '/^## / { name = substr($0, 4) } /\{round_dir\}/ && name { seen[name] = 1 } /\{worktree\}/ && name { wt[name] = 1 } END { for (n in seen) if (n in wt) print n }' "$f")
    for heading in 'Lens agent' 'No-checklist reviewer' 'PR and commit checker' 'Skeptic' 'Mutation runner'; do
        grep -qxF "$heading" <<<"$complete" || fail "$f: template '$heading' must use {round_dir} and {worktree}"
    done
}

check_max() {
    local f=$refs/max.md heading bad body complete
    [[ -f $f ]] || return 0
    for heading in 'Prerequisites and consent' 'Dispatch and teardown' 'Differential check' 'Devnet repro' 'Fuzz runner'; do
        grep -qxF "## $heading" "$f" || fail "$f: missing heading '## $heading'"
    done
    body=$(awk '/^```text$/ { t = 1; next } /^```$/ { t = 0; next } t' "$f")
    bad=$(grep -oE '\{[a-z_]+\}' <<<"$body" | sort -u | grep -vxE '\{(round_dir|worktree|lotus_rev|lotus_master|lotus_src|skill_dir)\}' || true)
    [[ -z $bad ]] || fail "$f: unknown placeholders: $(echo "$bad" | paste -sd ' ' -)"
    bad=$(grep -F "\${CLAUDE_SKILL_DIR}" <<<"$body" || true)
    [[ -z $bad ]] || fail "$f: subagents do not expand \${CLAUDE_SKILL_DIR}, use {skill_dir}: $bad"
    complete=$(awk '/^## / { name = substr($0, 4) } /\{round_dir\}/ && name { seen[name] = 1 } /\{worktree\}/ && name { wt[name] = 1 } END { for (n in seen) if (n in wt) print n }' "$f")
    for heading in 'Differential check' 'Devnet repro' 'Fuzz runner'; do
        grep -qxF "$heading" <<<"$complete" || fail "$f: template '$heading' must use {round_dir} and {worktree}"
    done
}

check_report_format() {
    local f=$refs/report-format.md word
    [[ -f $f ]] || return 0
    for word in blocker must-fix should-fix nit confirmed plausible refuted skeptic.jsonl mutation.jsonl mutation_candidate '"find"' '"replace"' passes-without-fix; do
        grep -qF -- "$word" "$f" || fail "$f: does not define '$word'"
    done
}

frontmatter_field() {
    awk -v key="$2" '
        NR == 1 && /^---$/ { fm = 1; next }
        fm && /^---$/ { exit }
        !fm { exit }
        index($0, key ":") == 1 { sub("^" key ":[[:space:]]*", ""); print }
    ' "$1"
}

frontmatter_hooks() {
    awk '
        NR == 1 && /^---$/ { fm = 1; next }
        fm && /^---$/ { exit }
        !fm { exit }
        /^hooks:/ { h = 1; print; next }
        h && /^[^[:space:]]/ { h = 0 }
        h { print }
    ' "$1"
}

check_skill() {
    local f=$skill_dir/SKILL.md agent=$repo_root/.claude/agents/forest-review-lens.md lines chars desc p hooks
    [[ -f $f ]] || {
        fail "missing SKILL.md"
        return 0
    }
    lines=$(wc -l <"$f")
    chars=$(wc -c <"$f")
    ((lines < 200)) || fail "$f: $lines lines, must be under 200"
    ((chars < 20000)) || fail "$f: $chars bytes, must be under 20000"
    [[ $(frontmatter_field "$f" name) == forest-review ]] || fail "$f: frontmatter name is not forest-review"
    desc=$(frontmatter_field "$f" description)
    [[ -n $desc ]] || fail "$f: no single-line description in the frontmatter"
    ((${#desc} < 1024)) || fail "$f: description is ${#desc} characters, must be under 1024"
    [[ $desc != *[\<\>]* ]] || fail "$f: description contains an angle bracket"
    while IFS= read -r p; do
        p=${p#*\}/}
        p=${p%.}
        [[ -e $skill_dir/$p ]] || fail "$f: path does not exist: \${CLAUDE_SKILL_DIR}/$p"
    done < <(grep -oE '\$\{CLAUDE_SKILL_DIR\}/[A-Za-z0-9_./-]+' "$f" | sort -u)
    hooks=$(frontmatter_hooks "$f")
    [[ -n $hooks ]] || fail "$f: no hooks block in the frontmatter"
    [[ $hooks == "$(frontmatter_hooks "$agent")" ]] || fail "$f: hooks block differs from $agent"
}

for name in report-format hygiene entry-points known-deviations prompts max; do
    [[ -f $refs/$name.md ]] || fail "missing references/$name.md"
done

while IFS= read -r -d '' doc; do
    check_em_dash_and_markers "$doc"
    check_repo_paths "$doc"
    check_contents_list "$doc"
done < <(find "$skill_dir" -name '*.md' -not -path "$fixtures/*" -print0 | sort -z)

check_known_deviations
check_entry_points
check_prompts
check_max
check_report_format
check_skill

if ((failures)); then
    echo "test_docs: $failures failure(s)"
    exit 1
fi
echo "test_docs: ok"
