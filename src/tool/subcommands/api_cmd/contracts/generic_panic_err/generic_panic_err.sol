// SPDX-License-Identifier: MIT
pragma solidity = 0.8.30;

contract GenericPanicFail {
    constructor() {
        // There is no direct way of generating Panic(0x00) so,
        // manually trigger Panic(0x00) using inline assembly
        assembly {
            // Store Panic(uint256) function selector: 0x4e487b71
            // followed by error code 0x00 (32 bytes)
            let ptr := mload(0x40)
            mstore(
                ptr,
                0x4e487b7100000000000000000000000000000000000000000000000000000000
            )
            mstore(
                add(ptr, 0x04),
                0x0000000000000000000000000000000000000000000000000000000000000000
            )
            revert(ptr, 0x24) // 4 bytes selector + 32 bytes data = 36 bytes (0x24)
        }
    }
}
