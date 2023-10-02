#!/bin/bash
set -euxo pipefail

# This script tests the migration(s) from Forest 0.12.1 to the current version.
# As simple as it is, it will detect regressions in the migration process and breaking changes.

source "$(dirname "$0")/harness.sh"

DATA_DIR="${TMP_DIR}/data_dir"
mkdir -p "${DATA_DIR}"

chmod -R 777 "${DATA_DIR}"

# Run Forest 0.12.1 with mounted db so that we can re-use it later.
# NOTE: We aren't using '--auto-download-snapshot', because of a bug in
# forest-0.12.1 that's related to `Content-Disposition` parsing.
docker run --init --rm --name forest-0.12.1 \
  --volume "${DATA_DIR}":/home/forest/.local/share/forest \
  ghcr.io/chainsafe/forest:v0.12.1 \
  --chain calibnet \
  --encrypt-keystore false \
  --import-snapshot=https://forest-archive.chainsafe.dev/latest/calibnet/ \
  --halt-after-import

# Assert the database is looking as expected.
if [ ! -d "${DATA_DIR}/calibnet/paritydb" ]; then
  echo "Database directory not found"
  exit 1
fi

# If can't access due to permissions, try changing ownership to the current user.
# This is needed for GHA which runs under a particular user.
if [ ! -w "${DATA_DIR}/calibnet/paritydb" ]; then
  sudo chown -R "$(id -u):$(id -g)" "${DATA_DIR}/"
fi

# Note that for Forest 0.12.1, a seamless migration is not really possible given that there is no notion of versioning.
# We **know** here that Forest was at 0.12.1 so we manually change the directory name to reflect that. This should not be required in the future.
mv "${DATA_DIR}/calibnet/paritydb" "${DATA_DIR}/calibnet/0.12.1"

CONFIG_FILE="${TMP_DIR}/config.toml"

# Create config file to point to the old database
echo "[client]" > "${CONFIG_FILE}"
echo "data_dir = \"${TMP_DIR}/data_dir\"" >> "${CONFIG_FILE}"
echo 'encrypt_keystore = false' >> "${CONFIG_FILE}"

# Run the current Forest with the old database. This should trigger a migration (or several ones).
forest --chain calibnet --log-dir "$LOG_DIRECTORY" --halt-after-import --track-peak-rss --config "${CONFIG_FILE}"

# Sync to HEAD. This might reveal migrations errors not caught above.
forest --chain calibnet --log-dir "$LOG_DIRECTORY" --detach --save-token ./admin_token --track-peak-rss --config "${CONFIG_FILE}"

ADMIN_TOKEN=$(cat admin_token)
FULLNODE_API_INFO="$ADMIN_TOKEN:/ip4/127.0.0.1/tcp/2345/http"

export ADMIN_TOKEN
export FULLNODE_API_INFO

forest_wait_for_sync
forest_check_db_stats

# Assert there is no "0.12.1" directory in the database directory. This and a successful sync indicate that the database was successfully migrated.
if [ -d "${DATA_DIR}/calibnet/0.12.1" ]; then
  echo "Database directory not migrated"
  exit 1
fi

# Get current Forest version
CURRENT_VERSION=$(forest --version | sed -E 's/.* (.*)\+.*/\1/')

# Assert there is a database directory for the current version
ls -d "${DATA_DIR}"/calibnet/"${CURRENT_VERSION}"/

echo "Migration test successful, artifacts are in ${TMP_DIR}"
