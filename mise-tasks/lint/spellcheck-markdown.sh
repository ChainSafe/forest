#!/bin/bash
#MISE description="Run cargo-spellcheck to check for spelling errors in Markdown files."

# cargo-spellcheck has a bug where leading HTML blocks cause the rest of the
# file to be silently skipped. Work around this by stripping leading HTML
# before checking. See https://github.com/drahnr/cargo-spellcheck/issues/357
set -euo pipefail

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

FILES=$(find . -name "*.md" \
  -not -path "*/node_modules/*" \
  -not -path "*/target/*" \
  -not -path "*/.git/*" \
  -not -path "*/CHANGELOG.md")

for f in $FILES; do
  DEST="$TMPDIR/$f"
  mkdir -p "$(dirname "$DEST")"
  # Strip leading HTML blocks (lines starting with < or blank, before first non-HTML line)
  awk '
    BEGIN { in_leading_html = 1 }
    in_leading_html && /^[[:space:]]*$/ { next }
    in_leading_html && /^[[:space:]]*</ { next }
    { in_leading_html = 0; print }
  ' "$f" > "$DEST"
done

cargo spellcheck check --code 1 $TMPDIR/*.md $TMPDIR/**/*.md \
  || (echo "See .config/spellcheck.md for tips"; false)
