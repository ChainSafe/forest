#!/bin/bash
#MISE description="Build and heap-profile a Forest binary with jemalloc. Usage: mise run jemalloc:heap <bin> [-- <bin-args...>](e.g. `mise run jemalloc:heap forest --encrypt-keystore=false --chain calibnet`). Dumps to /var/tmp/jeprof/jeprof.*.heap"
#MISE env={MALLOC_CONF="prof:true,prof_leak:true,prof_final:true,prof_prefix:/var/tmp/jeprof/jeprof"}

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: mise run jemalloc:heap <bin> [-- <bin-args...>]" >&2
    exit 2
fi

# create dump dir if not exist
mkdir -p /var/tmp/jeprof

cargo run \
    --no-default-features --features jemalloc-profiling --profile=release-symbols \
    --bin "$1" \
    -- "${@:2}"
