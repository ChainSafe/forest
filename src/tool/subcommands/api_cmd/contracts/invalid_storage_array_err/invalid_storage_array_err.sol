// SPDX-License-Identifier: MIT
pragma solidity >=0.8.17;

contract InvalidStorageArrayFail {
    constructor() {
        // There is no direct way of generating Panic(0x22) Invalid Storage array because it requires us to
        // corrupt the memory encoding so, for now manually triggering the InvalidStorageArray panic (0x22)
        // using inline assembly
        assembly {
            let ptr := mload(0x40)
            // Panic(uint256) selector: 0x4e487b71
            mstore(
                ptr,
                0x4e487b7100000000000000000000000000000000000000000000000000000000
            )
            // Panic code 0x22 (34 decimal)
            mstore(add(ptr, 0x04), 34) // 34 is 0x22
            // Revert with 36-byte panic data (4-byte selector + 32-byte code)
            revert(ptr, 0x24)
        }
    }
}
