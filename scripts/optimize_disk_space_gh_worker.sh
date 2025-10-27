#!/bin/bash

# This script checks the free disk space on a GitHub Actions runner
# and cleans up unnecessary files. To be used as a GitHub Actions workflow step
# when hitting disk space limits.
#
# DO NOT RUN IT LOCALLY as it may mess up your system.

if [[ -z "${GITHUB_ACTIONS}" ]]; then
  echo "This script is intended to be run only in GitHub Actions runners."
  exit 1
fi

echo "Disk space before cleanup $(df -h)"

sudo rm -rf /usr/share/dotnet
sudo rm -rf /usr/local/lib/android
sudo rm -rf /opt/ghc
sudo rm -rf /opt/hostedtoolcache/CodeQL
sudo docker image prune --all --force
sudo docker builder prune -a

echo "Disk space after cleanup $(df -h)"
