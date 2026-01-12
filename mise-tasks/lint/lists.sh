#!/bin/bash
#MISE description="Sorts specified list files in place to ensure consistent ordering."

LISTS=(
    "./src/tool/subcommands/api_cmd/test_snapshots_ignored.txt"
    "./src/tool/subcommands/api_cmd/test_snapshots.txt"
)

export LC_ALL=C

# Sort each list file in place
for FILE in "${LISTS[@]}"; do
    if [[ -f "$FILE" ]]; then
        sort --unique -o "$FILE" "$FILE"
        echo "Sorted $FILE"
      else
        echo "File $FILE does not exist."
    fi
  done
echo "All specified list files have been sorted."

