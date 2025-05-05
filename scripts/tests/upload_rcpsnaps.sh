#!/usr/bin/env bash
# This script compresses all `.rpcsnap.json` files in a given directory using zstd,
# uploads the compressed files to our DigitalOcean Spaces,
# updates `test_snapshots.txt` with the uploaded filenames,
# and prompts the user to optionally run the regression tests.

set -euo pipefail

SPACE_NAME="forest-snapshots"
DEST_DIR="rpc_test"

TEST_SNAPSHOTS="src/tool/subcommands/api_cmd/test_snapshots.txt"

if ! command -v s3cmd >/dev/null 2>&1; then
    echo "❌ 's3cmd' is not installed or not in your PATH."
    echo "Install it via your package manager (e.g. 'brew install s3cmd', 'yum install s3cmd')."
    exit 1
fi

if [ -z "$1" ]; then
    echo "❌ Please provide the directory path as an argument."
    echo "Usage: $0 <directory_path>"
    exit 1
fi

DIR_PATH="$1"

if [ ! -d "$DIR_PATH" ]; then
    echo "❌ Directory does not exist: ${DIR_PATH}"
    exit 1
fi

EXISTING_REMOTE=$(s3cmd ls "s3://${SPACE_NAME}/${DEST_DIR}/" | awk '{print $4}' | sed 's|.*/||')

# Gather files to be uploaded
FILES_TO_UPLOAD=()
CONFLICTING_FILES=()

for FILE_PATH in "${DIR_PATH}"/*.rpcsnap.json; do
    FILE_NAME=$(basename "$FILE_PATH")
    DEST_FILENAME="${FILE_NAME}.zst"

    if echo "$EXISTING_REMOTE" | grep -qx "$DEST_FILENAME"; then
        CONFLICTING_FILES+=("$DEST_FILENAME")
    else
        FILES_TO_UPLOAD+=("$FILE_PATH")
    fi
done

# Abort if any files already exist
if [ ${#CONFLICTING_FILES[@]} -gt 0 ]; then
    echo "❌ The following files already exist in DigitalOcean and would be overwritten:"
    for f in "${CONFLICTING_FILES[@]}"; do
        echo "  - $f"
    done
    echo "Aborting. No files were uploaded."
    exit 1
fi

for FILE_PATH in "${FILES_TO_UPLOAD[@]}"; do
    FILE_NAME=$(basename "$FILE_PATH")
    COMPRESSED_FILE="${FILE_PATH}.zst"
    DEST_PATH="${DEST_DIR}/${FILE_NAME}.zst"
    BUCKET_URL="s3://${SPACE_NAME}/${DEST_PATH}"

    zstd -f "$FILE_PATH" -o "$COMPRESSED_FILE"

    s3cmd --quiet --no-progress put "${COMPRESSED_FILE}" "${BUCKET_URL}" \
        --acl-public \
        --mime-type="application/json"

    echo "✅ Uploaded: ${COMPRESSED_FILE}"

    BASE_NAME=$(basename "$COMPRESSED_FILE")
    echo "$BASE_NAME" >> "$TEST_SNAPSHOTS"
done

# Sort the file in lexicographical order and remove dup lines
sort -u -o "$TEST_SNAPSHOTS" "$TEST_SNAPSHOTS"

read -r -p "🧪 Do you want to run 'cargo test --lib -- --test rpc_regression_tests --nocapture'? [y/N] " answer
case "$answer" in
    [yY][eE][sS]|[yY])
        echo "▶ Running tests..."
        cargo test --lib -- --test rpc_regression_tests --nocapture
        ;;
    *)
        echo "Skipping test run."
        ;;
esac
