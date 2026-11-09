import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "report.py"


def finding(fid, severity="should-fix", file="a.rs", line=1, lens="design", evidence="e", cls="other"):
    return {"id": fid, "lens": lens, "severity": severity, "file": file, "line": line, "symbol": "f",
            "class": cls, "claim": f"claim {fid}", "evidence": evidence, "fix": "x", "verdict": None, "mitigation": None}


def write_jsonl(path, recs):
    path.write_text("".join(json.dumps(r) + "\n" for r in recs))


def run(*args):
    return subprocess.run([sys.executable, str(SCRIPT), *args], capture_output=True, text=True)


class ReportTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.key_dir = Path(self.tmp.name) / "k"
        self.rd = self.key_dir / "rounds" / "1"
        self.rd.mkdir(parents=True)

    def merged(self):
        return [json.loads(line) for line in (self.rd / "merged.jsonl").read_text().splitlines()]

    def test_merge_dedups_by_id_keeps_highest_severity(self):
        write_jsonl(self.rd / "design.jsonl", [finding("aaaaaaaaaa", "nit", evidence="one")])
        write_jsonl(self.rd / "perf.jsonl", [finding("aaaaaaaaaa", "must-fix", lens="perf", evidence="two")])
        self.assertEqual(run("merge", str(self.rd)).returncode, 0)
        m = self.merged()
        self.assertEqual(len(m), 1)
        self.assertEqual(m[0]["severity"], "must-fix")
        self.assertIn("one", m[0]["evidence"])
        self.assertIn("two", m[0]["evidence"])

    def test_merge_same_id_records_other_lens_in_also(self):
        write_jsonl(self.rd / "design.jsonl", [finding("aaaaaaaaaa", "nit")])
        write_jsonl(self.rd / "perf.jsonl", [finding("aaaaaaaaaa", "must-fix", lens="perf")])
        write_jsonl(self.rd / "tests.jsonl", [finding("aaaaaaaaaa", "nit", lens="tests")])
        run("merge", str(self.rd))
        m = self.merged()[0]
        self.assertEqual(m["lens"], "perf")
        self.assertEqual(m["also"], ["design", "tests"])

    def test_merge_rejects_key_argument(self):
        self.assertNotEqual(run("merge", str(self.rd), "k").returncode, 0)

    def test_merge_drops_closed_ledger_ids(self):
        write_jsonl(self.rd / "design.jsonl", [finding("aaaaaaaaaa"), finding("bbbbbbbbbb")])
        write_jsonl(self.key_dir / "ledger.jsonl", [
            {"id": "aaaaaaaaaa", "status": "open"},
            {"id": "aaaaaaaaaa", "status": "fixed"},
            {"id": "bbbbbbbbbb", "status": "open"},
        ])
        run("merge", str(self.rd))
        self.assertEqual([f["id"] for f in self.merged()], ["bbbbbbbbbb"])

    def test_render_drops_refuted_and_sorts(self):
        write_jsonl(self.rd / "merged.jsonl", [
            finding("1111111111", "nit", "z.rs", 1),
            finding("2222222222", "blocker", "b.rs", 9),
            finding("3333333333", "blocker", "b.rs", 2),
            finding("4444444444", "must-fix", "a.rs", 5),
        ])
        write_jsonl(self.rd / "skeptic.jsonl", [
            {"id": "4444444444", "verdict": "refuted", "note": "n"},
            {"id": "2222222222", "mutation_candidate": "t", "file": "b.rs", "find": "a", "replace": "b"},
            {"id": "2222222222", "verdict": "confirmed", "note": "n"},
        ])
        run("render", str(self.rd))
        report = (self.rd / "report.md").read_text()
        self.assertNotIn("4444444444", report)
        order = [report.index(i) for i in ("3333333333", "2222222222", "1111111111")]
        self.assertEqual(order, sorted(order))

    def test_render_stdout_is_one_line_per_finding_plus_path(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111", "nit"), finding("2222222222", "blocker")])
        (self.rd / "adversarial.md").write_text(
            "## Input-surface table\n\n| Entry point | Input-derived parameter | Bound | Enforced at | Mitigation considered |\n"
            "| --- | --- | --- | --- | --- |\n| SECRETROW | n | 1 | none | m |\n")
        r = run("render", str(self.rd))
        lines = r.stdout.strip().splitlines()
        self.assertEqual(len(lines), 3)
        self.assertEqual(lines[0], "[blocker] 2222222222 a.rs:1 claim 2222222222")
        self.assertEqual(lines[2], str(self.rd / "report.md"))
        self.assertNotIn("SECRETROW", r.stdout)
        self.assertIn("SECRETROW", (self.rd / "report.md").read_text())

    def test_render_handles_missing_optional_files(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111")])
        r = run("render", str(self.rd))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue((self.rd / "report.md").is_file())

    def test_render_copies_parity_table_and_readiness(self):
        write_jsonl(self.rd / "merged.jsonl", [])
        (self.rd / "parity.md").write_text(
            "text\n\n| Behaviour | Forest | Lotus (permalink) | Same? | Changelog |\n| - | - | - | - | - |\n| ROW | a | b | yes | n/a |\n\nafter\n")
        (self.rd / "readiness.md").write_text("READINESS BODY\n")
        run("render", str(self.rd))
        report = (self.rd / "report.md").read_text()
        self.assertIn("| ROW |", report)
        self.assertNotIn("after", report)
        self.assertIn("READINESS BODY", report)

    def test_mutation_green_without_finding_becomes_finding(self):
        write_jsonl(self.rd / "merged.jsonl", [])
        write_jsonl(self.rd / "mutation.jsonl", [
            {"id": "cccccccccc", "result": "green", "note": "passes"},
            {"id": "dddddddddd", "result": "red", "note": "ok"},
        ])
        r = run("render", str(self.rd))
        self.assertIn("[should-fix] cccccccccc", r.stdout)
        self.assertNotIn("[should-fix] dddddddddd", r.stdout)
        self.assertIn("red", (self.rd / "report.md").read_text())

    def test_mutation_green_with_existing_finding_adds_no_duplicate(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111")])
        write_jsonl(self.rd / "mutation.jsonl", [{"id": "1111111111", "result": "green", "note": "p"}])
        r = run("render", str(self.rd))
        self.assertEqual(len(r.stdout.strip().splitlines()), 2)


    def test_render_writes_rendered_set_to_report_jsonl(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111"), finding("4444444444", "must-fix")])
        write_jsonl(self.rd / "skeptic.jsonl", [
            {"id": "4444444444", "verdict": "refuted", "note": "n"},
            {"id": "cccccccccc", "mutation_candidate": "t", "file": "m.rs", "find": "a", "replace": "b"},
        ])
        write_jsonl(self.rd / "mutation.jsonl", [{"id": "cccccccccc", "result": "green", "note": "passes"}])
        self.assertEqual(run("render", str(self.rd)).returncode, 0)
        rendered = {f["id"]: f for f in map(json.loads, (self.rd / "report.jsonl").read_text().splitlines())}
        self.assertEqual(set(rendered), {"1111111111", "cccccccccc"})
        self.assertEqual(rendered["cccccccccc"]["class"], "passes-without-fix")
        self.assertEqual(rendered["cccccccccc"]["file"], "m.rs")

    def test_merge_ignores_report_jsonl(self):
        write_jsonl(self.rd / "design.jsonl", [finding("1111111111")])
        write_jsonl(self.rd / "report.jsonl", [finding("9999999999")])
        run("merge", str(self.rd))
        self.assertEqual([f["id"] for f in self.merged()], ["1111111111"])

    def reported(self):
        return {f["id"]: f for f in map(json.loads, (self.rd / "report.jsonl").read_text().splitlines())}

    def test_render_folds_duplicates_into_primary(self):
        write_jsonl(self.rd / "merged.jsonl", [
            finding("1111111111", "should-fix", lens="design"),
            finding("2222222222", "must-fix", lens="correctness"),
            finding("3333333333", "nit", lens="tests"),
            finding("4444444444", "nit", lens="design"),
        ])
        write_jsonl(self.rd / "skeptic.jsonl", [
            {"id": "1111111111", "verdict": "confirmed", "note": "n"},
            {"id": "2222222222", "verdict": "confirmed", "note": "n", "duplicate_of": "1111111111"},
            {"id": "3333333333", "verdict": "confirmed", "note": "n", "duplicate_of": "1111111111"},
            {"id": "4444444444", "verdict": "confirmed", "note": "n", "duplicate_of": "1111111111"},
        ])
        r = run("render", str(self.rd))
        self.assertEqual(r.returncode, 0, r.stderr)
        rendered = self.reported()
        self.assertEqual(set(rendered), {"1111111111"})
        primary = rendered["1111111111"]
        self.assertEqual(primary["severity"], "must-fix")
        self.assertEqual(primary["duplicates"], ["2222222222", "3333333333", "4444444444"])
        lines = r.stdout.strip().splitlines()
        self.assertEqual(lines[0], "[must-fix] 1111111111 a.rs:1 claim 1111111111 (also: correctness, tests)")
        self.assertEqual(len(lines), 2)
        report = (self.rd / "report.md").read_text()
        self.assertNotIn("### `2222222222`", report)
        self.assertIn("2222222222", report)

    def test_render_records_verdicts_and_marks_unjudged(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111", "blocker"), finding("2222222222", "must-fix")])
        write_jsonl(self.rd / "skeptic.jsonl", [{"id": "1111111111", "verdict": "confirmed", "note": "n"}])
        run("render", str(self.rd))
        rendered = self.reported()
        self.assertEqual(rendered["1111111111"]["verdict"], "confirmed")
        self.assertIsNone(rendered["2222222222"]["verdict"])
        report = (self.rd / "report.md").read_text()
        self.assertEqual(report.count("Verdict: none (the skeptic did not judge it)"), 1)

    def test_render_without_skeptic_marks_nothing_unjudged(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111", "blocker")])
        run("render", str(self.rd))
        self.assertNotIn("Verdict", (self.rd / "report.md").read_text())

    def test_render_pr_appends_neutral_draft_comments(self):
        adv = {**finding("1111111111", "blocker", "src/p.rs", 7, lens="adversarial"), "claim": "SECRET peer OOM"}
        also_adv = {**finding("3333333333", "must-fix", lens="correctness"), "also": ["adversarial"], "fix": "SECRET cap"}
        write_jsonl(self.rd / "merged.jsonl", [adv, also_adv, finding("2222222222", "should-fix", "b.rs", 3)])
        self.assertEqual(run("render", str(self.rd), "--pr").returncode, 0)
        report = (self.rd / "report.md").read_text()
        drafts = report[report.index("## Draft review comments"):]
        self.assertIn("`src/p.rs:7`", drafts)
        self.assertIn("`b.rs:3`: claim 2222222222 Suggested change: x", drafts)
        self.assertNotIn("SECRET", drafts)
        self.assertEqual(drafts.count("\n- `"), 3)

    def test_render_without_pr_has_no_draft_comments(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111")])
        run("render", str(self.rd))
        self.assertNotIn("Draft review comments", (self.rd / "report.md").read_text())

    def test_render_keeps_duplicate_of_missing_primary(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("2222222222", "must-fix")])
        write_jsonl(self.rd / "skeptic.jsonl", [
            {"id": "2222222222", "verdict": "confirmed", "note": "n", "duplicate_of": "9999999999"},
        ])
        run("render", str(self.rd))
        self.assertEqual(set(self.reported()), {"2222222222"})

    def test_render_keeps_duplicate_of_refuted_primary(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111", "must-fix"), finding("2222222222", "should-fix", lens="tests")])
        write_jsonl(self.rd / "skeptic.jsonl", [
            {"id": "1111111111", "verdict": "refuted", "note": "n"},
            {"id": "2222222222", "verdict": "plausible", "note": "n", "duplicate_of": "1111111111"},
        ])
        run("render", str(self.rd))
        rendered = self.reported()
        self.assertEqual(set(rendered), {"2222222222"})
        self.assertEqual(rendered["2222222222"]["severity"], "should-fix")
        self.assertNotIn("also", rendered["2222222222"])

    def test_render_drops_duplicate_cycle_without_losing_findings(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111"), finding("2222222222")])
        write_jsonl(self.rd / "skeptic.jsonl", [
            {"id": "1111111111", "verdict": "confirmed", "note": "n", "duplicate_of": "2222222222"},
            {"id": "2222222222", "verdict": "confirmed", "note": "n", "duplicate_of": "1111111111"},
        ])
        run("render", str(self.rd))
        self.assertEqual(set(self.reported()), {"1111111111", "2222222222"})

    def test_mutation_candidates_on_one_symbol_get_distinct_stable_ids(self):
        write_jsonl(self.rd / "merged.jsonl", [])
        base = {"id": None, "mutation_candidate": "parse_cases", "file": "src/p.rs", "result": "green", "note": "n"}
        write_jsonl(self.rd / "mutation.jsonl", [{**base, "find": "a"}, {**base, "find": "b"}])
        run("render", str(self.rd))
        first = set(self.reported())
        self.assertEqual(len(first), 2)
        run("render", str(self.rd))
        self.assertEqual(set(self.reported()), first)
        report = (self.rd / "report.md").read_text()
        for fid in first:
            self.assertIn(f"`{fid}` green `parse_cases`", report)

    def test_mutation_green_on_existing_finding_keeps_its_id(self):
        write_jsonl(self.rd / "merged.jsonl", [finding("1111111111")])
        write_jsonl(self.rd / "mutation.jsonl", [
            {"id": "1111111111", "mutation_candidate": "t", "file": "a.rs", "find": "x", "result": "green", "note": "p"},
        ])
        run("render", str(self.rd))
        self.assertEqual(set(self.reported()), {"1111111111"})

    def test_render_caps_printed_lines(self):
        recs = [finding(f"b{i:09d}", "blocker", line=i) for i in range(2)]
        recs += [finding(f"m{i:09d}", "must-fix", line=i) for i in range(3)]
        recs += [finding(f"s{i:09d}", "should-fix", line=i) for i in range(14)]
        recs += [finding(f"n{i:09d}", "nit", line=i) for i in range(4)]
        write_jsonl(self.rd / "merged.jsonl", recs)
        lines = run("render", str(self.rd)).stdout.strip().splitlines()
        self.assertEqual(len(lines), 16)
        self.assertEqual(sum(line.startswith("[blocker]") for line in lines), 2)
        self.assertEqual(sum(line.startswith("[must-fix]") for line in lines), 3)
        self.assertEqual(sum(line.startswith("[should-fix]") for line in lines), 10)
        self.assertEqual(lines[-1], f"+8 more (4 should-fix, 4 nit) in {self.rd / 'report.md'}")
        self.assertEqual(len(self.reported()), 23)
        self.assertIn("n000000003", (self.rd / "report.md").read_text())

    def test_render_prints_every_blocker_and_must_fix_beyond_cap(self):
        recs = [finding(f"m{i:09d}", "must-fix", line=i) for i in range(17)] + [finding("s000000000", "should-fix")]
        write_jsonl(self.rd / "merged.jsonl", recs)
        lines = run("render", str(self.rd)).stdout.strip().splitlines()
        self.assertEqual(len(lines), 18)
        self.assertEqual(lines[-1], f"+1 more (1 should-fix, 0 nit) in {self.rd / 'report.md'}")

    def test_render_prints_all_at_cap(self):
        write_jsonl(self.rd / "merged.jsonl", [finding(f"n{i:09d}", "nit", line=i) for i in range(15)])
        lines = run("render", str(self.rd)).stdout.strip().splitlines()
        self.assertEqual(len(lines), 16)
        self.assertEqual(lines[-1], str(self.rd / "report.md"))

    def test_render_prints_path_when_cap_leaves_nothing_out(self):
        write_jsonl(self.rd / "merged.jsonl", [finding(f"m{i:09d}", "must-fix", line=i) for i in range(16)])
        lines = run("render", str(self.rd)).stdout.strip().splitlines()
        self.assertEqual(len(lines), 17)
        self.assertEqual(lines[-1], str(self.rd / "report.md"))

    def test_render_counts_unknown_severity_in_more_line(self):
        recs = [finding(f"s{i:09d}", "should-fix", line=i) for i in range(16)] + [finding("x000000000", "major")]
        write_jsonl(self.rd / "merged.jsonl", recs)
        lines = run("render", str(self.rd)).stdout.strip().splitlines()
        self.assertEqual(lines[-1], f"+2 more (1 should-fix, 0 nit, 1 major) in {self.rd / 'report.md'}")

    def test_mutation_green_without_id_or_candidate_fields_adds_no_finding(self):
        write_jsonl(self.rd / "merged.jsonl", [])
        write_jsonl(self.rd / "mutation.jsonl", [
            {"id": None, "result": "green", "note": "a"},
            {"id": None, "result": "green", "note": "b"},
        ])
        r = run("render", str(self.rd))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.reported(), {})

    def test_merge_drops_duplicates_of_closed_primary(self):
        write_jsonl(self.rd / "design.jsonl", [finding("aaaaaaaaaa"), finding("bbbbbbbbbb")])
        write_jsonl(self.key_dir / "ledger.jsonl", [
            {"id": "cccccccccc", "status": "rejected", "duplicates": ["aaaaaaaaaa"]},
        ])
        run("merge", str(self.rd))
        self.assertEqual([f["id"] for f in self.merged()], ["bbbbbbbbbb"])


if __name__ == "__main__":
    unittest.main()
