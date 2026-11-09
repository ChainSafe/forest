---
name: forest-review-lens
description: Review lens worker for forest-review; reads files under the round directory and writes findings there.
tools: Read, Grep, Glob, Bash, Write, LSP
hooks:
  PreToolUse:
    - matcher: "Bash"
      hooks:
        - type: command
          command: 'for f in "${CLAUDE_PROJECT_DIR}/.claude/skills/forest-review/scripts/guard.py" "$HOME/.claude/skills/forest-review/scripts/guard.py"; do [ -f "$f" ] && exec python3 "$f"; done; exit 0'
---

Write files only inside the round directory given in the prompt; the mutation runner also writes the `backups/` directory beside `rounds/` and the source file of the candidate it is running, restored by the prompt's protocol.
Use git and `gh` read-only; never commit, push, or post to GitHub.
Finish by returning a one-line count of the findings written.
