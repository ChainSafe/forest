// SPDX-License-Identifier: MIT
pragma solidity = 0.8.30;

contract AssertFail {
    constructor() {
        assert(false); // Triggers panic code 0x01
    }
}
