package main

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestValidateWithoutEOF(t *testing.T) {
	metric := []byte(`
cache_zstd_frame_0_size_bytes 268422051
zstd_frame_0_len 18022
zstd_frame_0_cap -1
`)
	e := Validate(metric)
	require.NoError(t, e)
}

func TestValidateWithEOF(t *testing.T) {
	metric := []byte(`
# HELP cache_zstd_frame_0_size_bytes Size of LruCache zstd_frame_0 in bytes
# TYPE cache_zstd_frame_0_size_bytes gauge
# UNIT cache_zstd_frame_0_size_bytes bytes
cache_zstd_frame_0_size_bytes 268422051
# HELP zstd_frame_0_len Length of LruCache zstd_frame_0
# TYPE zstd_frame_0_len gauge
zstd_frame_0_len 18022
# HELP zstd_frame_0_cap Capacity of LruCache zstd_frame_0
# TYPE zstd_frame_0_cap gauge
zstd_frame_0_cap -1
# EOF
`)
	e := Validate(metric)
	require.NoError(t, e)
}

func TestValidateWithInvalidCharacters(t *testing.T) {
	metric := []byte(`
cache_zstd_frame_0_size_bytes 268422051
zstd_frame_0_len 18022
zstd_frame_0_<cap> -1
`)
	e := Validate(metric)
	require.ErrorContains(t, e, "unsupported character")
}
