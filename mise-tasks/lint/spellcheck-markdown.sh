#!/bin/bash
#MISE description="Run cargo-spellcheck to check for spelling errors in Markdown files."

set -euo pipefail

FILES=$(find . -name "*.md" \
  -not -path "*/node_modules/*" \
  -not -path "*/target/*" \
  -not -path "*/.git/*" \
  -not -path "*/CHANGELOG.md")

cargo spellcheck check --code 1 $FILES \
  || (echo "See .config/spellcheck.md for tips"; false)
