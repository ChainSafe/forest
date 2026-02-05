#!/bin/bash
#MISE description="Extracts conformance report from the API compare compose volume and adds it to the docs."

TMP_REPORT=$(mktemp -d)/report.json
DATE=$(date +%Y-%m-%d)
./scripts/tests/api_compare/extract_conformance_report.sh "$TMP_REPORT"
./scripts/tests/api_compare/convert_report_to_markdown.sh "$TMP_REPORT" ./docs/docs/users/reports/api_conformance/report_"$DATE".md
