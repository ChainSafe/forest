#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from state import closed_ids as ledger_closed_ids, read_ledger, stable_id  # noqa: E402

SEVERITIES = ["blocker", "must-fix", "should-fix", "nit"]
RESERVED = {"merged", "skeptic", "mutation", "report"}
PRINT_LIMIT = 15
TABLES = [
    ("adversarial.md", "Input surface", "| Entry point |"),
    ("parity.md", "Deviations from Lotus", "| Behaviour |"),
]


def rank(severity: str) -> int:
    return SEVERITIES.index(severity) if severity in SEVERITIES else len(SEVERITIES)


def read_jsonl(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def write_jsonl(path: Path, recs: list[dict]) -> None:
    path.write_text("".join(json.dumps(r) + "\n" for r in recs))


def closed_ids(round_dir: Path) -> set[str]:
    return ledger_closed_ids(read_ledger(round_dir.parent.parent / "ledger.jsonl"))


def set_also(f: dict, lenses: list[str]) -> None:
    also = [name for name in dict.fromkeys(lenses) if name != f["lens"]]
    if also:
        f["also"] = also


def merge(round_dir: Path) -> list[dict]:
    closed = closed_ids(round_dir)
    by_id: dict[str, dict] = {}
    for path in sorted(round_dir.glob("*.jsonl")):
        if path.stem in RESERVED:
            continue
        for f in read_jsonl(path):
            fid = f["id"]
            if fid in closed:
                continue
            cur = by_id.get(fid)
            if cur is None:
                by_id[fid] = dict(f)
                continue
            lenses = [cur["lens"], *cur.get("also", []), f["lens"], *f.get("also", [])]
            if rank(f["severity"]) < rank(cur["severity"]):
                keep = cur["evidence"]
                cur.update(f)
                cur["evidence"] = keep
            if f["evidence"] and f["evidence"] not in cur["evidence"]:
                cur["evidence"] = f"{cur['evidence']}; {f['evidence']}" if cur["evidence"] else f["evidence"]
            set_also(cur, lenses)
    merged = list(by_id.values())
    write_jsonl(round_dir / "merged.jsonl", merged)
    return merged


def verdicts(round_dir: Path) -> dict[str, str]:
    return {r["id"]: r["verdict"] for r in read_jsonl(round_dir / "skeptic.jsonl") if "verdict" in r}


def duplicate_links(round_dir: Path) -> dict[str, str]:
    return {r["id"]: r["duplicate_of"] for r in read_jsonl(round_dir / "skeptic.jsonl") if r.get("duplicate_of")}


def primary_of(fid: str, links: dict[str, str], live: set[str]) -> str | None:
    target = links.get(fid)
    if target not in live:
        return None
    seen = {fid, target}
    while True:
        nxt = links.get(target)
        if nxt in seen:
            return None
        if nxt not in live:
            return target
        seen.add(nxt)
        target = nxt


def fold_duplicates(findings: list[dict], links: dict[str, str]) -> tuple[list[dict], set[str]]:
    live = {f["id"] for f in findings}
    by_id = {f["id"]: dict(f) for f in findings}
    folded = set()
    for f in findings:
        p = primary_of(f["id"], links, live)
        if p is None:
            continue
        folded.add(f["id"])
        primary = by_id[p]
        if rank(f["severity"]) < rank(primary["severity"]):
            primary["severity"] = f["severity"]
        primary["duplicates"] = [*primary.get("duplicates", []), f["id"]]
        set_also(primary, [*primary.get("also", []), f["lens"], *f.get("also", [])])
    return [by_id[f["id"]] for f in findings if f["id"] not in folded], folded


def candidate_id(r: dict) -> str:
    return stable_id(r["file"], r["mutation_candidate"], "passes-without-fix:" + r["find"])


def result_id(r: dict, known: set[str]) -> str:
    if r.get("id") in known or not all(k in r for k in ("file", "mutation_candidate", "find")):
        return r.get("id")
    return candidate_id(r)


def mutation_findings(round_dir: Path, known: set[str], closed: set[str]) -> list[dict]:
    files = {r["id"]: r["file"] for r in read_jsonl(round_dir / "skeptic.jsonl") if "mutation_candidate" in r}
    out = []
    for r in read_jsonl(round_dir / "mutation.jsonl"):
        fid = result_id(r, known)
        if r.get("result") != "green" or fid is None or fid in known or fid in closed:
            continue
        known.add(fid)
        out.append({
            "id": fid, "lens": "tests", "severity": "should-fix", "file": r.get("file") or files.get(r["id"], "unknown"), "line": 0,
            "symbol": "", "class": "passes-without-fix",
            "claim": f"test passes with the fix reverted: {r.get('note', '')}".strip(),
            "evidence": "mutation.jsonl", "fix": "make the test fail without the fix", "verdict": None, "mitigation": None,
        })
    return out


def extract_table(path: Path, header_prefix: str) -> str:
    if not path.is_file():
        return ""
    block: list[str] = []
    for line in path.read_text().splitlines():
        if line.startswith(header_prefix):
            block = [line]
        elif block and line.startswith("|"):
            block.append(line)
        elif block:
            break
    return "\n".join(block)


def finding_line(f: dict) -> str:
    also = f" (also: {', '.join(f['also'])})" if f.get("also") else ""
    return f"[{f['severity']}] {f['id']} {f['file']}:{f['line']} {f['claim']}{also}"


def ranked_lines(findings: list[dict], report: Path) -> list[str]:
    if len(findings) <= PRINT_LIMIT:
        return [finding_line(f) for f in findings] + [str(report)]
    room = PRINT_LIMIT - sum(rank(f["severity"]) <= rank("must-fix") for f in findings)
    shown, rest = [], []
    for f in findings:
        if rank(f["severity"]) <= rank("must-fix"):
            shown.append(f)
        elif f["severity"] == "should-fix" and room > 0:
            shown.append(f)
            room -= 1
        else:
            rest.append(f)
    if not rest:
        return [finding_line(f) for f in shown] + [str(report)]
    counts = Counter(f["severity"] for f in rest)
    parts = [f"{counts.pop('should-fix', 0)} should-fix", f"{counts.pop('nit', 0)} nit"]
    parts += [f"{n} {sev}" for sev, n in sorted(counts.items())]
    more = f"+{len(rest)} more ({', '.join(parts)}) in {report}"
    return [finding_line(f) for f in shown] + [more]


def finding_section(f: dict, unjudged: bool) -> str:
    parts = [f"### `{f['id']}` {f['severity']} {f['file']}:{f['line']}"]
    if f.get("verdict"):
        parts.append(f"Verdict: {f['verdict']}")
    elif unjudged:
        parts.append("Verdict: none (the skeptic did not judge it)")
    if f.get("duplicates") or f.get("also"):
        raised = "Raised by: " + ", ".join([f["lens"], *f.get("also", [])])
        if f.get("duplicates"):
            raised += "; folded duplicates: " + ", ".join(f"`{d}`" for d in f["duplicates"])
        parts.append(raised)
    for label, field in [("Claim", "claim"), ("Evidence", "evidence"), ("Fix", "fix"), ("Mitigation", "mitigation")]:
        if f.get(field):
            parts.append(f"{label}: {f[field]}")
    return "\n\n".join(parts)


def mutation_section(round_dir: Path, known: set[str]) -> str:
    rows = read_jsonl(round_dir / "mutation.jsonl")
    if not rows:
        return ""
    lines = []
    for r in rows:
        test = f" `{r['mutation_candidate']}`" if r.get("mutation_candidate") else ""
        lines.append(f"- `{result_id(r, known)}` {r['result']}{test}: {r.get('note', '')}")
    return "## Mutation results\n\n" + "\n".join(lines)


def draft_comments(findings: list[dict]) -> str:
    lines = []
    for f in findings:
        if "adversarial" in [f["lens"], *f.get("also", [])]:
            body = f"Could you double-check the bounds and error handling on this input path? (local finding `{f['id']}`)"
        else:
            body = f"{f['claim']} Suggested change: {f['fix']}"
        lines.append(f"- `{f['file']}:{f['line']}`: {body}")
    intro = "Not posted. One comment per finding; input-handling findings are worded neutrally and their detail stays in this report."
    return "## Draft review comments\n\n" + intro + "\n\n" + ("\n".join(lines) or "No comments.")


def render(round_dir: Path, pr: bool = False) -> list[dict]:
    path = round_dir / "merged.jsonl"
    findings = read_jsonl(path) if path.is_file() else merge(round_dir)
    verdict = verdicts(round_dir)
    findings = [{**f, "verdict": verdict.get(f["id"])} for f in findings if verdict.get(f["id"]) != "refuted"]
    findings, folded = fold_duplicates(findings, duplicate_links(round_dir))
    unjudged = {f["id"] for f in findings if f["verdict"] is None} if (round_dir / "skeptic.jsonl").is_file() else set()
    known = {f["id"] for f in findings} | folded
    findings += mutation_findings(round_dir, set(known), closed_ids(round_dir))
    findings.sort(key=lambda f: (rank(f["severity"]), f["file"], f["line"]))
    write_jsonl(round_dir / "report.jsonl", findings)

    sections = ["# Review report", "## Findings"]
    sections += [finding_section(f, f["id"] in unjudged) for f in findings] or ["No findings."]
    for name, title, prefix in TABLES:
        table = extract_table(round_dir / name, prefix)
        if table:
            sections.append(f"## {title}\n\n{table}")
    mutation = mutation_section(round_dir, known)
    if mutation:
        sections.append(mutation)
    readiness = round_dir / "readiness.md"
    if readiness.is_file():
        sections.append("## Readiness\n\n" + readiness.read_text().strip())
    if pr:
        sections.append(draft_comments(findings))
    (round_dir / "report.md").write_text("\n\n".join(sections) + "\n")
    return findings


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="forest-review merge and report assembly")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("merge").add_argument("round_dir")
    r = sub.add_parser("render")
    r.add_argument("round_dir")
    r.add_argument("--pr", action="store_true", help="append draft review comments")
    return p


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    round_dir = Path(args.round_dir)
    try:
        if args.cmd == "merge":
            merge(round_dir)
        else:
            for line in ranked_lines(render(round_dir, args.pr), round_dir / "report.md"):
                print(line)
    except (OSError, KeyError, ValueError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
