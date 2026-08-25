// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

/// Lotus `itests/contracts/ContractA.sol`, pinned to the compiler this
/// directory's `compile.sh` uses. Calls into ContractB, which calls back here.
interface IContractB {
    function callBackAndRead(address origin) external view returns (uint256);
    function callBackAndDouble(address origin) external view returns (uint256);
}

contract ContractA {
    uint256 public storedValue;
    address public contractB;

    constructor() {
        storedValue = 42;
    }

    function setContractB(address _contractB) external {
        contractB = _contractB;
    }

    function getValue() external view returns (uint256) {
        return storedValue;
    }

    function setValue(uint256 _value) external {
        storedValue = _value;
    }

    function callBAndReadBack() external view returns (uint256) {
        return IContractB(contractB).callBackAndRead(address(this));
    }

    function callBAndDouble() external view returns (uint256) {
        return IContractB(contractB).callBackAndDouble(address(this));
    }

    function getContractB() external view returns (address) {
        return contractB;
    }
}
