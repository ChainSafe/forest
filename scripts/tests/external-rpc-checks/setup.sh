#!/usr/bin/env bash
# Runs the RPC checks against the external data.riba.plus dataset.
# Needs docker only; everything else happens in containers.

set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
pushd "${PARENT_PATH}"

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans --volumes

# Resolves the snapshot and probes the dataset for DAYS_AGO.
# Sets CHAIN, START and END on success.
# Returns 2 on failure
check_day() {
  local status=0
  docker compose run --rm --env DAYS_AGO="$1" resolve || status=$?
  case ${status} in
    0) ;;
    2)
      echo "no snapshot covers the day $1 day(s) ago"
      return 2
      ;;
    *)
      echo "resolve failed for the day $1 day(s) ago with status ${status}"
      exit "${status}"
      ;;
  esac

  CHECK_TARGET="$(docker compose run --rm --entrypoint cat resolve /data/check-target)"
  read -r CHAIN START END <<< "${CHECK_TARGET}"
  [[ -n ${CHAIN} && ${START} =~ ^[0-9]+$ && ${END} =~ ^[0-9]+$ ]] || {
    echo "resolve did not report a usable chain and epoch range: ${CHECK_TARGET}"
    exit 1
  }

  docker compose run --rm rpc-checks --probe --network "${CHAIN}" "${START}" "${END}" || status=$?
  case ${status} in
    0) ;;
    2)
      echo "the dataset has not published the day $1 day(s) ago yet"
      return 2
      ;;
    *)
      echo "the dataset probe failed for ${CHAIN} ${START}..${END}"
      exit "${status}"
      ;;
  esac
}

# Tries to fetch latest available archive dataset (from yesterday)
# if not available lookup an archive dataset before yesterday
# if nothing is available than fail
check_day 1 || check_day 2

# Imports the snapshot and back-fills the index.
docker compose run --rm init

docker compose up --detach --wait forest

# Syncs the rest till reach END.
docker compose exec forest \
  timeout 30m forest-cli sync wait --target "$((END + 1))" > /dev/null

docker compose run --rm verify "${START}" "${END}"

docker compose run --rm rpc-checks "${START}" "${END}"

popd
