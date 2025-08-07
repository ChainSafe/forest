package main

import (
	"os"
	"testing"

	"github.com/ipfs/go-cid"
	"github.com/stretchr/testify/require"
)

func TestIsCidDefined(t *testing.T) {
	require.NotEqual(t, cid.Undef, CID_UNDEF_RUST)
	require.False(t, isCidDefined(cid.Undef))
	require.False(t, isCidDefined(CID_UNDEF_RUST))
	require.True(t, isCidDefined(cid.MustParse("bafy2bzaceac6jbaeolcsbh7rawcshcvh2cokvxrgsh4sxg5yu34i5xllbfpw4")))
}

func TestOsOpen(t *testing.T) {
	_, err := os.Open("/var/tmp/dummy")
	require.NoError(t, err)
}
