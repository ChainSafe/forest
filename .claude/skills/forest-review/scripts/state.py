#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

CLOSED = {"accepted", "fixed", "rejected", "deferred"}
STATUSES = {"open"} | CLOSED
NEEDS_REASON = {"rejected", "deferred"}
STATE_DIR = ".forest-review"


def git(*args, check=True):
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {r.stderr.strip()}")
    return r


def stable_id(file: str, symbol: str, cls: str) -> str:
    return hashlib.sha1(f"{file}|{symbol}|{cls}".encode()).hexdigest()[:10]


def state_root() -> Path:
    out = git("worktree", "list", "--porcelain").stdout
    for line in out.splitlines():
        if line.startswith("worktree "):
            return Path(line[len("worktree "):]) / STATE_DIR
    raise RuntimeError("no worktree found")


def head_sha() -> str:
    return git("rev-parse", "HEAD").stdout.strip()


def branch_key(mode: str, pr: int | None) -> str:
    if mode == "pr" and pr is not None:
        return f"pr-{pr}"
    branch = git("symbolic-ref", "--short", "-q", "HEAD", check=False).stdout.strip()
    if branch:
        return branch.replace("/", "__")
    return "detached-" + git("rev-parse", "--short", "HEAD").stdout.strip()


class BaseRefError(RuntimeError):
    pass


def base_sha(base: str | None) -> str:
    tried = [base] if base else ["origin/main", "main", "master"]
    for ref in tried:
        r = git("merge-base", "HEAD", ref, check=False)
        if r.returncode == 0:
            return r.stdout.strip()
    raise BaseRefError(f"no merge-base with HEAD for: {', '.join(tried)}; pass a valid --base")


def tree_sha() -> str:
    out = git("stash", "create").stdout.strip()
    return out or head_sha()


def is_ancestor(old: str, new: str) -> bool:
    return git("merge-base", "--is-ancestor", old, new, check=False).returncode == 0


def next_round(key_dir: Path) -> int:
    rounds = key_dir / "rounds"
    nums = [int(p.name) for p in rounds.iterdir() if p.name.isdigit()] if rounds.is_dir() else []
    return max(nums, default=0) + 1


def previous_head(key_dir: Path, n: int) -> str | None:
    meta = key_dir / "rounds" / str(n - 1) / "meta.json"
    if not meta.is_file():
        return None
    return json.loads(meta.read_text()).get("head_sha")


def leftover_backups(key_dir: Path) -> list[str]:
    backups = key_dir / "backups"
    if not backups.is_dir():
        return []
    return sorted(str(p.relative_to(backups)) for p in backups.rglob("*") if p.is_file())


def init_round(mode: str, pr: int | None, base: str | None, effort: str) -> dict:
    root = state_root()
    key = branch_key(mode, pr)
    key_dir = root / key
    left = leftover_backups(key_dir)
    if left:
        print("\n".join(left))
        print(f"leftover backups in {key_dir / 'backups'}; restore or remove them first", file=sys.stderr)
        sys.exit(3)
    base_commit = base_sha(base)
    n = next_round(key_dir)
    head = head_sha()
    prev = previous_head(key_dir, n)
    full_pass = not (mode == "delta" and prev is not None and is_ancestor(prev, head))
    round_dir = key_dir / "rounds" / str(n)
    round_dir.mkdir(parents=True)
    out = {
        "root": str(root),
        "key": key,
        "round": n,
        "round_dir": str(round_dir),
        "base_sha": base_commit,
        "head_sha": head,
        "tree_sha": tree_sha(),
        "full_pass": full_pass,
    }
    meta = {
        **out,
        "mode": mode,
        "effort": effort,
        "created": datetime.now(timezone.utc).isoformat(),
    }
    (round_dir / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    return out


def ledger_path(key: str) -> Path:
    return state_root() / key / "ledger.jsonl"


def read_ledger(path: Path) -> dict[str, dict]:
    entries: dict[str, dict] = {}
    if path.is_file():
        for line in path.read_text().splitlines():
            if line.strip():
                rec = json.loads(line)
                entries[rec["id"]] = rec
    return entries


def append_ledger(path: Path, recs: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a") as f:
        for rec in recs:
            f.write(json.dumps(rec) + "\n")


def closed_ids(ledger: dict[str, dict]) -> set[str]:
    out = set()
    for fid, rec in ledger.items():
        if rec.get("status") in CLOSED:
            out.add(fid)
            out.update(rec.get("duplicates", []))
    return out


def finding_id(f: dict) -> str:
    return f.get("id") or stable_id(f["file"], f.get("symbol", ""), f["class"])


def ledger_upsert(key: str, findings_file: str) -> int:
    path = ledger_path(key)
    known = read_ledger(path)
    folded = {d for rec in known.values() for d in rec.get("duplicates", [])}
    new = []
    for line in Path(findings_file).read_text().splitlines():
        if not line.strip():
            continue
        f = json.loads(line)
        fid = finding_id(f)
        if fid in folded and fid not in known:
            continue
        if fid not in known:
            rec = {**f, "id": fid, "status": "open"}
            known[fid] = rec
            new.append(rec)
            continue
        dups = known[fid].get("duplicates", [])
        added = [d for d in f.get("duplicates", []) if d not in dups]
        if added:
            rec = {**known[fid], "duplicates": [*dups, *added]}
            known[fid] = rec
            new.append(rec)
    append_ledger(path, new)
    return len(new)


def ledger_set(key: str, fid: str, status: str, reason: str | None) -> None:
    if status not in STATUSES:
        raise ValueError(f"unknown status {status}")
    if status in NEEDS_REASON and not reason:
        raise ValueError(f"status {status} requires --reason")
    path = ledger_path(key)
    rec = read_ledger(path).get(fid)
    if rec is None:
        raise ValueError(f"unknown finding id {fid}")
    rec = {k: v for k, v in rec.items() if k != "reason"}
    rec["status"] = status
    if reason:
        rec["reason"] = reason
    append_ledger(path, [rec])


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="forest-review round state and ledger")
    sub = p.add_subparsers(dest="cmd", required=True)
    ir = sub.add_parser("init-round")
    ir.add_argument("--mode", choices=["self", "pr", "delta"], required=True)
    ir.add_argument("--pr", type=int)
    ir.add_argument("--base")
    ir.add_argument("--effort", choices=["quick", "full", "max"], default="full")
    lu = sub.add_parser("ledger-upsert")
    lu.add_argument("key")
    lu.add_argument("findings")
    ls = sub.add_parser("ledger-set")
    ls.add_argument("key")
    ls.add_argument("id")
    ls.add_argument("status", choices=sorted(STATUSES))
    ls.add_argument("--reason")
    return p


def main(argv=None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.cmd == "init-round" and args.mode == "pr" and args.pr is None:
        parser.error("--mode pr requires --pr")
    try:
        if args.cmd == "init-round":
            print(json.dumps(init_round(args.mode, args.pr, args.base, args.effort)))
        elif args.cmd == "ledger-upsert":
            ledger_upsert(args.key, args.findings)
        else:
            ledger_set(args.key, args.id, args.status, args.reason)
    except BaseRefError as e:
        print(f"error: {e}", file=sys.stderr)
        return 4
    except (RuntimeError, ValueError, KeyError, OSError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
