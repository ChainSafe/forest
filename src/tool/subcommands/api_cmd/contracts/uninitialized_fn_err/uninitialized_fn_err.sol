// SPDX-License-Identifier: MIT
pragma solidity = 0.8.30;

contract UninitializedFunctionFail {
    function() internal view returns (uint) unassignedPointer;

    constructor() {
        unassignedPointer(); // Trigger CalledUninitializedFunction panic (0x51)
    }
}
