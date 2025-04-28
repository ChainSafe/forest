#!/usr/bin/env bash
# This script compresses all `.rpcsnap.json` files in a given directory using zstd,
# then uploads the compressed files to our DigitalOcean Spaces.

SPACE_NAME="forest-snapshots"
# REGION="fra1"
DEST_DIR="rpc_test"

TEST_SNAPSHOTS="src/tool/subcommands/api_cmd/test_snapshots.txt"

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

for FILE_PATH in "${DIR_PATH}"/*.rpcsnap.json; do
    if [ ! -f "$FILE_PATH" ]; then
        echo "❌ No .rpcsnap.json files found in ${DIR_PATH}"
        break
    fi

    FILE_NAME=$(basename "$FILE_PATH")
    COMPRESSED_FILE="${FILE_PATH}.zst"
    DEST_PATH="${DEST_DIR}/${FILE_NAME}.zst"
    BUCKET_URL="s3://${SPACE_NAME}/${DEST_PATH}"

    if zstd -f "$FILE_PATH" -o "$COMPRESSED_FILE"; then
        if s3cmd --quiet --no-progress put "${COMPRESSED_FILE}" "${BUCKET_URL}" --acl-public --mime-type="application/json" --add-header="Cache-Control: no-cache, no-store, must-revalidate"; then
            echo "✅ Uploaded: ${COMPRESSED_FILE}"

            BASE_NAME=$(basename "$COMPRESSED_FILE")
            echo "$BASE_NAME" >> "$TEST_SNAPSHOTS"
        else
            echo "❌ Failed to upload: ${COMPRESSED_FILE}"
        fi
    else
        echo "❌ Failed to compress: ${FILE_NAME}"
    fi
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
