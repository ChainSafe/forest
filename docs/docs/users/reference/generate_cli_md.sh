#!/bin/bash

if [ "$1" == "local" ]; then
  ENVIRONMENT="local"
elif [ "$1" == "docker" ]; then
  ENVIRONMENT="docker"
else
  echo "Usage: $0 <local|docker>"
  exit 1
fi

cat <<EOF
---
title: Command Line Options
sidebar_position: 1
---

<!--
CLI reference documentation for forest, forest-wallet, forest-cli, and forest-tool.
Do not edit manually, use the \`generate_cli_md.sh\` script.
-->

This document lists every command line option and sub-command for Forest.
EOF

if [ "$ENVIRONMENT" == "local" ]; then
  bash ./cli.sh
else
  docker run --rm --entrypoint /bin/bash -v "$(pwd)":/forest ghcr.io/chainsafe/forest:edge-fat /forest/cli.sh
fi
