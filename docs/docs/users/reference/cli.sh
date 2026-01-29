#! /bin/bash

generate_markdown_section() {
  local forest_name=$1
  local command_name=$2

  # Print the command header
  echo
  if [ -z "$command_name" ]; then
    echo "## \`$forest_name\`"
  else
    echo "### \`$forest_name $command_name\`"
  fi
  echo

  echo "\`\`\`"
  eval "$forest_name $command_name --help" | sed 's/\+git\..*//g'
  echo "\`\`\`"
}

generate_markdown_section "forest"

generate_markdown_section "forest-wallet"
generate_markdown_section "forest-wallet" "new"
generate_markdown_section "forest-wallet" "balance"
generate_markdown_section "forest-wallet" "default"
generate_markdown_section "forest-wallet" "export"
generate_markdown_section "forest-wallet" "has"
generate_markdown_section "forest-wallet" "import"
generate_markdown_section "forest-wallet" "list"
generate_markdown_section "forest-wallet" "set-default"
generate_markdown_section "forest-wallet" "sign"
generate_markdown_section "forest-wallet" "validate-address"
generate_markdown_section "forest-wallet" "verify"
generate_markdown_section "forest-wallet" "delete"
generate_markdown_section "forest-wallet" "send"

generate_markdown_section "forest-cli"

generate_markdown_section "forest-cli" "chain"
generate_markdown_section "forest-cli" "chain block"
generate_markdown_section "forest-cli" "chain message"
generate_markdown_section "forest-cli" "chain read-obj"
generate_markdown_section "forest-cli" "chain set-head"
generate_markdown_section "forest-cli" "chain prune"
generate_markdown_section "forest-cli" "chain list"

generate_markdown_section "forest-cli" "auth"
generate_markdown_section "forest-cli" "auth create-token"
generate_markdown_section "forest-cli" "auth api-info"

generate_markdown_section "forest-cli" "net"
generate_markdown_section "forest-cli" "net peers"
generate_markdown_section "forest-cli" "net connect"
generate_markdown_section "forest-cli" "net disconnect"

generate_markdown_section "forest-cli" "sync"
generate_markdown_section "forest-cli" "sync wait"
generate_markdown_section "forest-cli" "sync check-bad"
generate_markdown_section "forest-cli" "sync mark-bad"

generate_markdown_section "forest-cli" "mpool"
generate_markdown_section "forest-cli" "mpool pending"
generate_markdown_section "forest-cli" "mpool stat"
generate_markdown_section "forest-cli" "mpool nonce"

generate_markdown_section "forest-cli" "state"
generate_markdown_section "forest-cli" "state fetch"
generate_markdown_section "forest-cli" "state compute"

generate_markdown_section "forest-cli" "config"

generate_markdown_section "forest-cli" "snapshot"
generate_markdown_section "forest-cli" "snapshot export"

generate_markdown_section "forest-cli" "send"
generate_markdown_section "forest-cli" "info"
generate_markdown_section "forest-cli" "shutdown"

generate_markdown_section "forest-cli" "healthcheck"
generate_markdown_section "forest-cli" "healthcheck ready"

generate_markdown_section "forest-cli" "f3"
generate_markdown_section "forest-cli" "f3 manifest"
generate_markdown_section "forest-cli" "f3 status"
generate_markdown_section "forest-cli" "f3 certs"
generate_markdown_section "forest-cli" "f3 certs get"
generate_markdown_section "forest-cli" "f3 certs list"
generate_markdown_section "forest-cli" "f3 powertable"
generate_markdown_section "forest-cli" "f3 powertable get"
generate_markdown_section "forest-cli" "f3 powertable get-proportion"
generate_markdown_section "forest-cli" "f3 ready"

generate_markdown_section "forest-tool" ""

generate_markdown_section "forest-tool" "backup"
generate_markdown_section "forest-tool" "backup create"
generate_markdown_section "forest-tool" "backup restore"

generate_markdown_section "forest-tool" "completion"

generate_markdown_section "forest-tool" "benchmark"
generate_markdown_section "forest-tool" "benchmark car-streaming"
generate_markdown_section "forest-tool" "benchmark graph-traversal"
generate_markdown_section "forest-tool" "benchmark forest-encoding"
generate_markdown_section "forest-tool" "benchmark export"

generate_markdown_section "forest-tool" "state-migration"
generate_markdown_section "forest-tool" "state-migration actor-bundle"

generate_markdown_section "forest-tool" "snapshot"
generate_markdown_section "forest-tool" "snapshot fetch"
generate_markdown_section "forest-tool" "snapshot validate-diffs"
generate_markdown_section "forest-tool" "snapshot validate"
generate_markdown_section "forest-tool" "snapshot compress"
generate_markdown_section "forest-tool" "snapshot compute-state"

generate_markdown_section "forest-tool" "fetch-params"

generate_markdown_section "forest-tool" "archive"
generate_markdown_section "forest-tool" "archive info"
generate_markdown_section "forest-tool" "archive export"
generate_markdown_section "forest-tool" "archive checkpoints"
generate_markdown_section "forest-tool" "archive f3-header"
generate_markdown_section "forest-tool" "archive metadata"
generate_markdown_section "forest-tool" "archive merge"
generate_markdown_section "forest-tool" "archive merge-f3"
generate_markdown_section "forest-tool" "archive diff"
generate_markdown_section "forest-tool" "archive sync-bucket"

generate_markdown_section "forest-tool" "db"
generate_markdown_section "forest-tool" "db stats"
generate_markdown_section "forest-tool" "db destroy"
generate_markdown_section "forest-tool" "db import"

generate_markdown_section "forest-tool" "car"
generate_markdown_section "forest-tool" "car concat"
generate_markdown_section "forest-tool" "car validate"

generate_markdown_section "forest-tool" "api"
generate_markdown_section "forest-tool" "api serve"
generate_markdown_section "forest-tool" "api compare"
generate_markdown_section "forest-tool" "api generate-test-snapshot"
generate_markdown_section "forest-tool" "api dump-tests"
generate_markdown_section "forest-tool" "api test"

generate_markdown_section "forest-tool" "net ping"

generate_markdown_section "forest-tool" "shed"
generate_markdown_section "forest-tool" "shed summarize-tipsets"
generate_markdown_section "forest-tool" "shed peer-id-from-key-pair"
generate_markdown_section "forest-tool" "shed private-key-from-key-pair"
generate_markdown_section "forest-tool" "shed openrpc"

generate_markdown_section "forest-tool" "index"
generate_markdown_section "forest-tool" "index backfill"

generate_markdown_section "forest-dev" ""

generate_markdown_section "forest-dev" "fetch-test-snapshots"

generate_markdown_section "forest-dev" "state"
generate_markdown_section "forest-dev" "state compute"
generate_markdown_section "forest-dev" "state replay-compute"
generate_markdown_section "forest-dev" "state validate"
generate_markdown_section "forest-dev" "state replay-validate"
