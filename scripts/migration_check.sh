#!/usr/bin/env bash
set -euo pipefail

# Function to sync using a specific tag
function sync_with_tag() {
    local tag=$1
    echo "Syncing using tag ($tag)..."

    # Write build and sync logic here
    git checkout $tag
    make clean
    make build

    ./target/debug/forest --chain calibnet --encrypt-keystore false --auto-download-snapshot --detach
    ./target/debug/forest-cli --chain calibnet sync wait
    # Check if the sync succeeded for the tag
    if [ $? -eq 0 ]; then
        echo "Sync successful for tag: $tag"
    else
        echo "Sync failed for tag: $tag"
        exit 1
    fi
}

# Get the repository URL
repo_url="https://github.com/ChainSafe/forest.git"

# DB Migration are supported v0.11.1 onwards
START_TAG="v0.11.1"

# Clone the repository
echo "Cloning the repository..."
git clone $repo_url --recursive
cd forest || exit

# Fetch the latest tags from the remote repository
git fetch --tags

# Check if the start tag exists
if ! git rev-parse "$START_TAG" &>/dev/null; then
  echo "Start tag $START_TAG does not exist."
  exit 1
fi

# Get a list of all tags sorted chronologically
tags=$(git tag --sort=creatordate)

# Flag to indicate if we should start building
start_building=false

# Loop through each tag and sync with corresponding version
for tag in $tags; do
  # Check if the current tag matches the start tag
  if [ "$tag" = "$START_TAG" ]; then
    start_building=true
  fi
  if $start_building; then
    # Run sync check with the current tag
    sync_with_tag "$tag"
  fi
done

# Get latest tag
LATEST_TAG=$(git describe --tags --abbrev=0)
# Clean DB before testing db migration from "V0.11.1" to latest
/target/debug/forest-cli --chain calibnet db clean --force

# Sync all migrations, from "`V0.11.1`" to latest
# Sync calibnet with Forest `V0.11.1`
sync_with_tag $START_TAG
# Sync calibnet with latest version of Forest
sync_with_tag $LATEST_TAG


