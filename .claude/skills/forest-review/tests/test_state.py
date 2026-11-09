import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "state.py"
sys.path.insert(0, str(SCRIPT.parent))


def git(cwd, *args):
    return subprocess.run(
        ["git", "-C", str(cwd), "-c", "user.name=t", "-c", "user.email=t@t", *args],
        check=True, capture_output=True, text=True,
    ).stdout


def run_state(cwd, *args):
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args], cwd=cwd, capture_output=True, text=True
    )


def init_round(cwd, *args):
    r = run_state(cwd, "init-round", *args)
    assert r.returncode == 0, r.stderr
    return json.loads(r.stdout)


class StateTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.repo = Path(self._tmp.name).resolve() / "repo"
        self.repo.mkdir()
        git(self.repo, "init", "-q", "-b", "main")
        git(self.repo, "config", "commit.gpgsign", "false")
        git(self.repo, "config", "tag.gpgsign", "false")
        (self.repo / ".gitignore").write_text(".forest-review/\n")
        (self.repo / "a.txt").write_text("one\n")
        git(self.repo, "add", ".")
        git(self.repo, "commit", "-q", "-m", "init")

    def test_key_sanitizes_slashes(self):
        git(self.repo, "checkout", "-q", "-b", "feature/x")
        self.assertEqual(init_round(self.repo, "--mode", "self")["key"], "feature__x")

    def test_pr_key(self):
        out = init_round(self.repo, "--mode", "pr", "--pr", "42")
        self.assertEqual(out["key"], "pr-42")

    def test_pr_mode_requires_pr_number(self):
        self.assertEqual(run_state(self.repo, "init-round", "--mode", "pr").returncode, 2)

    def test_invalid_base_exits_4(self):
        r = run_state(self.repo, "init-round", "--mode", "self", "--base", "nope")
        self.assertEqual(r.returncode, 4)
        self.assertIn("nope", r.stderr)
        self.assertFalse((self.repo / ".forest-review" / "main" / "rounds").exists())

    def test_no_default_base_exits_4(self):
        git(self.repo, "checkout", "-q", "-b", "other")
        git(self.repo, "branch", "-q", "-D", "main")
        r = run_state(self.repo, "init-round", "--mode", "self")
        self.assertEqual(r.returncode, 4)
        self.assertIn("origin/main", r.stderr)

    def test_set_open_clears_reason(self):
        self._ledger([{"id": "a1"}])
        run_state(self.repo, "ledger-set", "main", "a1", "rejected", "--reason", "x")
        run_state(self.repo, "ledger-set", "main", "a1", "open")
        path = self.repo / ".forest-review" / "main" / "ledger.jsonl"
        last = json.loads(path.read_text().splitlines()[-1])
        self.assertEqual(last["status"], "open")
        self.assertNotIn("reason", last)

    def test_detached_head_key(self):
        git(self.repo, "checkout", "-q", "--detach")
        self.assertTrue(init_round(self.repo, "--mode", "self")["key"].startswith("detached-"))

    def test_linked_worktree_uses_main_root(self):
        wt = self.repo.parent / "wt"
        git(self.repo, "worktree", "add", "-q", "-b", "side", str(wt))
        out = init_round(wt, "--mode", "self")
        self.assertEqual(out["root"], str(self.repo / ".forest-review"))
        self.assertTrue((self.repo / ".forest-review" / "side" / "rounds" / "1").is_dir())

    def test_refuses_with_leftover_backups(self):
        init_round(self.repo, "--mode", "self")
        b = self.repo / ".forest-review" / "main" / "backups"
        b.mkdir(parents=True)
        (b / "leftover.rs").write_text("x")
        r = run_state(self.repo, "init-round", "--mode", "self")
        self.assertEqual(r.returncode, 3)
        self.assertIn("leftover.rs", r.stdout)

    def test_delta_after_rebase_forces_full_pass(self):
        first = init_round(self.repo, "--mode", "self")
        self.assertTrue(first["full_pass"])
        self.assertFalse(init_round(self.repo, "--mode", "delta")["full_pass"])
        git(self.repo, "commit", "-q", "--amend", "-m", "rewritten")
        self.assertTrue(init_round(self.repo, "--mode", "delta")["full_pass"])

    def test_rounds_increment_and_meta(self):
        a = init_round(self.repo, "--mode", "self", "--effort", "quick")
        b = init_round(self.repo, "--mode", "self")
        self.assertEqual((a["round"], b["round"]), (1, 2))
        meta = json.loads((Path(b["round_dir"]) / "meta.json").read_text())
        for k in ("root", "key", "round", "base_sha", "head_sha", "tree_sha", "full_pass", "effort", "created"):
            self.assertIn(k, meta)

    def test_tree_sha_captures_unstaged_edit(self):
        clean = init_round(self.repo, "--mode", "self")
        self.assertEqual(clean["tree_sha"], clean["head_sha"])
        (self.repo / "a.txt").write_text("two\n")
        (self.repo / "new.txt").write_text("n\n")
        before = (git(self.repo, "status", "--porcelain"), git(self.repo, "stash", "list"))
        dirty = init_round(self.repo, "--mode", "self")
        after = (git(self.repo, "status", "--porcelain"), git(self.repo, "stash", "list"))
        self.assertEqual(before, after)
        self.assertNotEqual(dirty["tree_sha"], clean["tree_sha"])
        self.assertEqual(git(self.repo, "show", f"{dirty['tree_sha']}:a.txt"), "two\n")

    def test_stable_id(self):
        import state
        self.assertEqual(state.stable_id("f.rs", "foo", "panic"), hashlib.sha1(b"f.rs|foo|panic").hexdigest()[:10])

    def _ledger(self, findings):
        init_round(self.repo, "--mode", "self")
        f = self.repo / "f.jsonl"
        f.write_text("".join(json.dumps(x) + "\n" for x in findings))
        r = run_state(self.repo, "ledger-upsert", "main", str(f))
        self.assertEqual(r.returncode, 0, r.stderr)

    def closed(self):
        from state import closed_ids, read_ledger
        return sorted(closed_ids(read_ledger(self.repo / ".forest-review" / "main" / "ledger.jsonl")))

    def test_ledger_reason_required(self):
        self._ledger([{"id": "aaa"}])
        self.assertNotEqual(run_state(self.repo, "ledger-set", "main", "aaa", "rejected").returncode, 0)
        self.assertNotEqual(run_state(self.repo, "ledger-set", "main", "aaa", "deferred").returncode, 0)
        self.assertEqual(run_state(self.repo, "ledger-set", "main", "aaa", "rejected", "--reason", "nope").returncode, 0)

    def test_ledger_closed_lists_only_closed(self):
        self._ledger([{"id": "a1"}, {"id": "b2"}, {"file": "x.rs", "symbol": "s", "class": "c"}])
        run_state(self.repo, "ledger-set", "main", "a1", "fixed")
        out = self.closed()
        self.assertEqual(out, ["a1"])

    def test_upsert_keeps_existing_status(self):
        self._ledger([{"id": "a1"}])
        run_state(self.repo, "ledger-set", "main", "a1", "accepted")
        r = run_state(self.repo, "ledger-upsert", "main", str(self.repo / "f.jsonl"))
        self.assertEqual(r.returncode, 0)
        self.assertEqual(self.closed(), ["a1"])

    def test_duplicates_close_with_their_primary(self):
        self._ledger([{"id": "a1", "duplicates": ["d1"]}])
        f = self.repo / "f2.jsonl"
        f.write_text(json.dumps({"id": "a1", "duplicates": ["d2"]}) + "\n")
        self.assertEqual(run_state(self.repo, "ledger-upsert", "main", str(f)).returncode, 0)
        self.assertEqual(self.closed(), [])
        run_state(self.repo, "ledger-set", "main", "a1", "rejected", "--reason", "r")
        self.assertEqual(self.closed(), ["a1", "d1", "d2"])

    def test_upsert_skips_id_folded_as_duplicate(self):
        self._ledger([{"id": "a1", "duplicates": ["d1"]}])
        f = self.repo / "f2.jsonl"
        f.write_text(json.dumps({"id": "d1"}) + "\n")
        self.assertEqual(run_state(self.repo, "ledger-upsert", "main", str(f)).returncode, 0)
        ledger = (self.repo / ".forest-review" / "main" / "ledger.jsonl").read_text().splitlines()
        self.assertEqual([json.loads(line)["id"] for line in ledger], ["a1"])


if __name__ == "__main__":
    unittest.main()
