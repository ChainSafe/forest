// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

/// Lotus `itests/contracts/ContractB.sol`, pinned to the compiler this
/// directory's `compile.sh` uses. Callback target for ContractA skip-sender tests.
interface IContractA {
    function getValue() external view returns (uint256);
}

contract ContractB {
    function callBackAndRead(address origin) external view returns (uint256) {
        return IContractA(origin).getValue();
    }

    function callBackAndDouble(address origin) external view returns (uint256) {
        uint256 val = IContractA(origin).getValue();
        return val * 2;
    }
}
