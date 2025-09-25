#!/bin/bash
set -euxo pipefail

# This script tests the migration(s) from Forest 0.19.2 to the current version.
# As simple as it is, it will detect regressions in the migration process and breaking changes.

source "$(dirname "$0")/harness.sh"

DATA_DIR="${TMP_DIR}/data_dir"
mkdir -p "${DATA_DIR}"

chmod -R 777 "${DATA_DIR}"

FOREST_INIT_VERSION="0.30.0"

# Run Forest 0.19.2 with mounted db so that we can re-use it later.
docker run --init --rm --name forest-${FOREST_INIT_VERSION} \
  --volume "${DATA_DIR}":/root/.local/share/forest \
  ghcr.io/chainsafe/forest:v${FOREST_INIT_VERSION} \
  --chain calibnet \
  --encrypt-keystore false \
  --auto-download-snapshot \
  --halt-after-import

# Assert the database is looking as expected.
if [ ! -d "${DATA_DIR}/calibnet/${FOREST_INIT_VERSION}" ]; then
  echo "Database directory not found"
  exit 1
fi

# If can't access due to permissions, try changing ownership to the current user.
# This is needed for GHA which runs under a particular user.
if [ ! -w "${DATA_DIR}/calibnet/${FOREST_INIT_VERSION}" ]; then
  sudo chown -R "$(id -u):$(id -g)" "${DATA_DIR}/"
fi

CONFIG_FILE="${TMP_DIR}/config.toml"

# Create config file to point to the old database
echo "[client]" > "${CONFIG_FILE}"
echo "data_dir = \"${TMP_DIR}/data_dir\"" >> "${CONFIG_FILE}"
echo 'encrypt_keystore = false' >> "${CONFIG_FILE}"

# Run the current Forest with the old database. This should trigger a migration (or several ones).
/usr/bin/time -v forest --chain calibnet --log-dir "$LOG_DIRECTORY" --halt-after-import --config "${CONFIG_FILE}"

# Sync to HEAD. This might reveal migrations errors not caught above.
/usr/bin/time -v forest --chain calibnet --log-dir "$LOG_DIRECTORY" --save-token ./admin_token --config "${CONFIG_FILE}" &

forest_wait_api

ADMIN_TOKEN=$(cat admin_token)
FULLNODE_API_INFO="$ADMIN_TOKEN:/ip4/127.0.0.1/tcp/2345/http"

export ADMIN_TOKEN
export FULLNODE_API_INFO

forest_wait_for_sync
forest_check_db_stats

# Assert there is no "0.19.2" directory in the database directory. This and a successful sync indicate that the database was successfully migrated.
if [ -d "${DATA_DIR}/calibnet/${FOREST_INIT_VERSION}" ]; then
  echo "Database directory not migrated"
  exit 1
fi

# Get current Forest version
CURRENT_VERSION=$(forest --version | sed -E 's/.* (.*)\+.*/\1/')

# Assert there is a database directory for the current version
ls -d "${DATA_DIR}"/calibnet/"${CURRENT_VERSION}"/

echo "Migration test successful, artifacts are in ${TMP_DIR}"
