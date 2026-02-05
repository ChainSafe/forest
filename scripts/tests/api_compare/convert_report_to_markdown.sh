#!/usr/bin/env bash

# Simple script to generate a markdown report from test.json
# Usage: ./convert_report_to_markdown.sh <input_json> <output_markdown>

set -euo pipefail

INPUT_FILE="$1"
OUTPUT_FILE="$2"

# Check if input file exists
if [[ ! -f "$INPUT_FILE" ]]; then
    echo "Error: Input file '$INPUT_FILE' not found."
    exit 1
fi

# Generate the markdown report
{
    echo "# $(date '+%Y-%m-%d') - API Parity Report"
    echo ""
    echo "| Method | Lotus-conformance check |"
    echo "|--------|-------------------------|"
    
    jq -r '.methods[] | 
        if (.name | startswith("Forest.")) then
            "| `\(.name)` | N/A (Forest-specific) |"
        else
            "| `\(.name)` | \(if .status.type == "tested" then "✅" else "❌" end) |"
        end' "$INPUT_FILE"
    
} > "$OUTPUT_FILE"

echo "Report generated: $OUTPUT_FILE"
