// SPDX-License-Identifier: MIT
pragma solidity >=0.8.17;

contract OutOfMemoryFail {
    constructor() {
        // There is no direct way of generating Panic(0x41) Out memory panic through code because so we are
        // manually triggering it. Overloading the memory causes "capacity overflow" (it might be rust related)
        assembly {
            let ptr := mload(0x40) // Get free memory pointer
            // Store Panic(uint256) selector: 0x4e487b71
            mstore(
                ptr,
                0x4e487b7100000000000000000000000000000000000000000000000000000000
            )
            // Store the panic code 0x41 (65 decimal)
            mstore(add(ptr, 0x04), 65)
            // Revert with the 36-byte panic data (4-byte selector + 32-byte code)
            revert(ptr, 0x24)
        }
    }
}
