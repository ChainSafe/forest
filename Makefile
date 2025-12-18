##
## Memory Profiling
##

# Read up on memory profiling in Forest: https://rumcajs.dev/posts/memory-analysis-in-rust/

# Memory profiling is done with the `profiling` profile. There's no silver bullet for memory profiling, so we provide a few options here.

### Gperftools
# https://github.com/gperftools/gperftools

# Profile with gperftools (Memory/Heap profiler)
# There is a workaround there, as outlined in https://github.com/gperftools/gperftools/issues/1603
gperfheapprofile = FOREST_PROFILING_GPERFTOOLS_BUILD=1 cargo build --no-default-features --features system-alloc --profile=profiling --bin $(1); \
	ulimit -n 8192; \
	HEAPPROFILE_USE_PID=t HEAPPROFILE=/tmp/gperfheap.$(1).prof target/profiling/$(1) $(2)

gperfheapprofile.forest:
	$(call gperfheapprofile,forest, --chain calibnet --encrypt-keystore=false)

# To visualize the heap profile, run:
# pprof -http=localhost:8080 <path/to/profiled/binary> <path/to/gperfheap.forest.prof
# Don't use the default `pprof` package; use the one from google instead: https://github.com/google/pprof

#### Heaptrack
# https://github.com/KDE/heaptrack

memprofile-heaptrack = cargo build --no-default-features --features system-alloc --profile=profiling --bin $(1); \
						 ulimit -n 8192; \
             heaptrack -o /tmp/heaptrack.$(1).%p.zst target/profiling/$(1) $(2)

memprofile-heaptrack.forest:
	$(call memprofile-heaptrack,forest, --chain calibnet --encrypt-keystore=false)

.PHONY: $(MAKECMDGOALS)
