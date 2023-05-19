package test

import (
	"testing"

	"github.com/filecoin-project/go-state-types/builtin"
)

func TestVerifiedRegistryActorAddr(t *testing.T) {
	t.Error(builtin.VerifiedRegistryActorAddr.String())
}
