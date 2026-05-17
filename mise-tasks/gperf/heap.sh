#!/bin/bash
#MISE description="Build and heap-profile a Forest binary with gperftools. Usage: mise run gperf:heap <bin> [-- <bin-args...>]. Dumps to /tmp/gperfheap.<bin>.prof.*"

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: mise run gperf:heap <bin> [-- <bin-args...>]" >&2
    exit 2
fi

bin="$1"
shift
# Strip the `--` separator if mise forwarded it.
if [ "${1-}" = "--" ]; then
    shift
fi

# `FOREST_PROFILING_GPERFTOOLS_BUILD` toggles `cargo:rustc-link-lib=tcmalloc` in
# build.rs — without it, the binary won't be linked against gperftools and
# `HEAPPROFILE` is inert.
# `--message-format=json-render-diagnostics` reports the built executable path
# on stdout (respecting any `CARGO_TARGET_DIR` / `[build] target-dir` override);
# diagnostics still render on stderr.
exe=$(
    FOREST_PROFILING_GPERFTOOLS_BUILD=1 cargo build \
        --no-default-features --features system-alloc --profile=profiling \
        --bin "$bin" --message-format=json-render-diagnostics |
        sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' |
        tail -1
)
if [ -z "$exe" ]; then
    echo "could not locate built executable for $bin" >&2
    exit 1
fi

ulimit -n 8192
HEAPPROFILE_USE_PID=t \
HEAPPROFILE="/tmp/gperfheap.${bin}.prof" \
    "$exe" "$@"
