import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parent.parent
SCRIPT = SKILL_DIR / "scripts" / "guard.py"
PROJECT_DIR = SKILL_DIR.parents[2]

BLOCKED = [
    "git commit -m x",
    "git -C . push",
    "git -c user.name=x commit -m x",
    "cd a && git push origin x",
    "FOO=1 git push",
    "echo hi\ngit push",
    "git log | cat; git commit -m x",
    "gh pr comment 1 -b x",
    "gh pr review 1 --approve",
    "gh pr edit 1",
    "gh pr merge 1",
    "gh pr close 1",
    "gh pr create",
    "gh pr ready 1",
    "gh issue comment 1 -b x",
    "gh issue create",
    "gh api repos/o/r/issues/1/comments -f body=x",
    "gh api -X POST repos/o/r/pulls",
    "gh api --method PATCH x",
    "gh api --method=DELETE x",
    "gh api repos/o/r --input body.json",
    "gh api graphql -f query='mutation{...}'",
    "gh api graphql --method GET -f query='mutation{x}'",
    "echo 'unterminated && git push",
    'bash -c "git push"',
    'sh -c "git commit -m x"',
    "bash -lc 'git push'",
    "git log | xargs -I{} git push",
    "echo x | xargs -I {} git push",
    'eval "git push"',
    "if true; then git push; fi",
    "for i in 1; do git push; done",
    "{ git push; }",
    "! git push",
    "cd a && \\\ngit push",
    "git \\\n commit -m x",
    "env -u X git push",
    "time -p git push",
    "sudo -u x git push",
    "/usr/bin/git push",
    "gh pr reopen 1",
    "gh pr update-branch 1",
    "gh issue delete 1",
    "gh issue lock 1",
    "gh release create v1",
    "gh release upload v1 f",
    "gh workflow run x.yml",
    "bash <<'EOF'\ngit push\nEOF",
    "sh <<-EOF\n\tgit commit -m x\n\tEOF",
    "cat <<'EOF' | bash\ngit push\nEOF",
    "sudo bash <<'EOF'\ngit push\nEOF",
    "fish -c 'git push'",
    "source /dev/stdin <<'EOF'\ngit push\nEOF",
    "eval \"$(cat)\" <<'EOF'\ngit push\nEOF",
    "git commit -F - <<'EOF'\nmsg\nEOF",
    "cat > f <<'EOF'\nok\nEOF\ngit push",
    "cat > f <<EOF\n$(git push)\nEOF",
    "cat <<E\"O\"F\nEOF\ngit push\nE",
    "cat <<'E'OF\nEOF\ngit push\nE",
    "cat <<E:\nE:\ngit push\nE",
    "cat <<E\\OF\nEOF\ngit push\nE",
    "echo $((1<<2))\ngit push\n2",
    "(( x = 1 << y ))\ngit push\ny",
    "timeout 60 bash <<'EOF'\ngit push\nEOF",
    "nice bash <<'EOF'\ngit push\nEOF",
    "env -S 'bash -e' <<'EOF'\ngit push\nEOF",
    "busybox sh <<'EOF'\ngit push\nEOF",
    "tee >(bash) <<'EOF'\ngit push\nEOF",
    "cat <<'EOF' > >(bash)\ngit push\nEOF",
    "exec 3<<'EOF'\ngit push\nEOF\nbash <&3",
    "cat > f <<\\EOF\n$(git push)\nEOF",
    "cat > notes.md <<'EOF'\nsome prose\ngit push origin x\nEOF",
    "timeout 60 git push",
    "timeout -s KILL 5 git commit -m x",
    "nice -n 5 git push",
    "bash <<< 'git push'",
    'bash <<<"git push"',
    "cat <<EOF\nE\\\nOF\ngit push \\\n  origin x\nEOF",
    "echo `: <<EOF `\ngit push \\\n  origin x\nEOF",
    "echo $'a\\' <<EOF \\'' # '\ngit push \\\n  origin x\nEOF",
    ". /dev/stdin <<< 'git push'",
    "source /dev/stdin <<< 'git push'",
    "eval 'git push' <<< hi",
    "echo ${x:-<<'y' }\ngit push \\\n  origin x\ny",
    "echo ${a[1<<'y' ]}\ngit push \\\n  origin x\ny",
    "echo $[1<<'y' ]\ngit push \\\n  origin x\ny",
    "echo ${x:-<<y }\necho $'\\'' ; git push\ny",
    "a=( <<'y' )\ngit push \\\n  origin x\ny",
    "cat <<\"EOF\" > f\ngit push is blocked; it's prose\nEOF",
    "cat > f <<'EOF'\n$(git push) doesn't run\nEOF",
    "cat >> f <<'EOF'\nrun git push later, it's prose\nEOF",
    "echo it's\n(git push)",
    "echo it's\n\\git push",
    "echo it's\ng\\it p\\\nush",
    "echo it's\ngit push>/dev/null",
    "echo it's\nbash -c 'git push'",
    "echo it's\n/usr/bin/git -C d commit -m x",
    "echo it's\ngh -R o/r pr comment 1 -b x",
    "echo it's\ngh api x -f a=b",
    "git -C 'my dir' push; echo it's",
    "git -c 'user.name=Tom & Jerry' commit -m x; echo $'\\''",
    "git -c \"core.sshCommand=ssh -i k; true\" push origin HEAD\necho it's",
]

ALLOWED = [
    "git log",
    "git diff --stat",
    "git -C . log --oneline",
    "gh pr view 1 --json title",
    "gh pr diff 1",
    "gh issue view 1",
    "gh api repos/filecoin-project/lotus/contents/x?ref=v1.38.3",
    "gh api --method GET search/issues -f q=x",
    "gh api -X GET search/issues -f q=x",
    'echo "git push"',
    "echo 'a && git commit'",
    "git log --format='%s' && ls",
    'bash -c "git log"',
    "xargs echo",
    "git --no-pager log",
    "gh release view v1",
    "gh workflow list",
    "",
    "cat >> f <<'EOF'\nThe test doesn't fail.\nEOF",
    "cat >> f <<EOF\nit's fine\nEOF\ngit log",
    "cat <<\"EOF\" > f\npushing is blocked; it's prose\nEOF",
    "cat <<-EOF\n\tdon't\n\tEOF",
    "cat <<A <<'B'\nit's\nA\nwon't\nB",
    "cat >> f <<'EOF'\nWe ran git log and gh pr view; it's fine.\nEOF",
    "bash <<EOF\necho it's\nEOF",
    "cat <<EOF\nabc",
    "cat <<< \"it's\"",
    "git log # see <<EOF",
]


def run(payload):
    return subprocess.run(
        [sys.executable, str(SCRIPT)],
        input=payload,
        capture_output=True,
        text=True,
    )


def bash(command):
    return json.dumps({"tool_name": "Bash", "tool_input": {"command": command}})


def hook_command():
    for line in (SKILL_DIR / "SKILL.md").read_text().splitlines():
        key, _, value = line.strip().partition(": ")
        if key == "command" and value.startswith("'") and value.endswith("'"):
            return value[1:-1].replace("''", "'")
    raise AssertionError("no hook command in SKILL.md")


def run_hook(project_dir, payload, home=None):
    env = {**os.environ, "CLAUDE_PROJECT_DIR": str(project_dir), "HOME": str(home or project_dir)}
    return subprocess.run(["sh", "-c", hook_command()], input=payload, capture_output=True, text=True, env=env)


class GuardTest(unittest.TestCase):
    def test_blocked(self):
        for cmd in BLOCKED:
            with self.subTest(cmd=cmd):
                r = run(bash(cmd))
                self.assertEqual(r.returncode, 2)
                self.assertTrue(r.stderr.strip())

    def test_allowed(self):
        for cmd in ALLOWED:
            with self.subTest(cmd=cmd):
                self.assertEqual(run(bash(cmd)).returncode, 0)

    def test_non_bash_tool_ignored(self):
        payload = json.dumps(
            {"tool_name": "Write", "tool_input": {"command": "git push"}}
        )
        self.assertEqual(run(payload).returncode, 0)

    def test_malformed_stdin_passes(self):
        for payload in ["", "not json", "[]", '{"tool_name": "Bash"}']:
            with self.subTest(payload=payload):
                self.assertEqual(run(payload).returncode, 0)


class HookCommandTest(unittest.TestCase):
    def test_blocks_through_the_shipped_command(self):
        self.assertTrue((PROJECT_DIR / ".claude/skills/forest-review/scripts/guard.py").is_file())
        self.assertEqual(run_hook(PROJECT_DIR, bash("git push")).returncode, 2)
        self.assertEqual(run_hook(PROJECT_DIR, bash("git log")).returncode, 0)

    def test_personal_install_blocks(self):
        with tempfile.TemporaryDirectory() as empty, tempfile.TemporaryDirectory() as home:
            (Path(home) / ".claude/skills").mkdir(parents=True)
            (Path(home) / ".claude/skills/forest-review").symlink_to(SKILL_DIR)
            self.assertEqual(run_hook(empty, bash("git push"), home).returncode, 2)

    def test_missing_guard_allows(self):
        with tempfile.TemporaryDirectory() as empty:
            self.assertEqual(run_hook(empty, bash("git push")).returncode, 0)


if __name__ == "__main__":
    unittest.main()
