#!/bin/bash
# Build forest/curio-devnet:${CURIO_COMMIT} when that tag is missing.
set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
source "${PARENT_PATH}/.env"

IMAGE="forest/curio-devnet:${CURIO_COMMIT}"
if docker image inspect "${IMAGE}" >/dev/null 2>&1; then
  echo "Using existing ${IMAGE}"
  exit 0
fi

echo "Building ${IMAGE} (linux/amd64) ..."
docker build \
  --platform linux/amd64 \
  --build-arg "CURIO_COMMIT=${CURIO_COMMIT}" \
  -f "${PARENT_PATH}/curio.dockerfile" \
  -t "${IMAGE}" \
  "${PARENT_PATH}"
