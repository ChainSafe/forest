#!/usr/bin/env bash
set -euo pipefail

# Function to sync using a specific tag
function sync_with_tag() {
    local tag=$1
    echo "Syncing using tag ($tag)..."

    # Create the download URL for the forest release
    URL="https://github.com/ChainSafe/forest/releases/download/${tag}/forest-${tag}-linux-amd64.zip"
    # Download the release using curl
    curl -LJO "${URL}"
    
    # Unzip the downloaded file
    unzip "forest-${tag}-linux-amd64.zip"
    cd "forest-${tag}"

    # Run forest daemon 
    ./forest --chain calibnet --encrypt-keystore false --auto-download-snapshot --detach

    # Check if the sync succeeded for the tag
    if ./forest-cli --chain calibnet sync wait; then
        echo "Sync successful for tag: $tag"
        pkill -9 forest
        # clean up
        cd ..
        rm "forest-${tag}-linux-amd64.zip" "forest-${tag}" -rf
        sleep 5s
    else
        echo "Sync failed for tag: $tag"
        exit 1
    fi
}

# DB Migration are supported v0.11.1 onwards
START_TAG="v0.12.1"

# Fetch the latest tags from the remote repository
git fetch --tags

# Get a list of all tags sorted chronologically
tags=$(git tag --sort=creatordate)
# Get latest tag
LATEST_TAG=""

# Database migration are not supported for forest version below `v0.11.1`
is_tag_valid=false

echo "Testing db migrations from v0.11.1 to latest, one by one"
# Loop through each tag and sync with corresponding version
for tag in $tags; do
  # Check if the current tag matches the start tag
  if [ "$tag" = "$START_TAG" ]; then
    is_tag_valid=true
  fi
  if $is_tag_valid; then
    # Run sync check with the current tag
    sync_with_tag "$tag"
  fi
  LATEST_TAG="$tag"
done

echo "Testing db migration from v0.11.1 to latest, at once"
# Sync calibnet with Forest `V0.11.1`
sync_with_tag "$START_TAG"
# Sync calibnet with latest version of Forest
sync_with_tag "$LATEST_TAG"

echo "Migration check completed successfully."
