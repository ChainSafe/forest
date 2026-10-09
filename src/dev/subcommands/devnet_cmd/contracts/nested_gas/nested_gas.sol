// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

/// Each `recurse` level forwards at most 63/64 of the remaining gas (EIP-150:
/// https://github.com/ethereum/EIPs/blob/15f61ed0fda82ec86d8d6a872f6b874816f03d96/EIPS/eip-150.md#L32-L33),
/// so the gas *limit* the top-level call needs grows as (64/63)^depth above the
/// gas it actually *uses*. That gap is what `eth_estimateGas`'s search exists to close.
contract NestedGas {
    uint256 public acc;

    /// Succeeds only when handed a large gas limit, and otherwise reverts explicitly rather than
    /// running out of gas. Raising the limit fixes it, so the estimator searches past the revert.
    function requiresHighGasLimit() external {
        require(gasleft() > 50_000_000, "gas limit too low");
        acc += 1;
    }

    /// Reverts at any gas limit, the failure the estimator must report rather than search around.
    function alwaysReverts() external pure {
        revert("always reverts");
    }

    function recurse(uint256 depth) external {
        if (depth == 0) {
            acc += 1;
            return;
        }
        (bool ok, ) = address(this).call(
            abi.encodeWithSelector(this.recurse.selector, depth - 1)
        );
        require(ok, "subcall out of gas");
    }
}
