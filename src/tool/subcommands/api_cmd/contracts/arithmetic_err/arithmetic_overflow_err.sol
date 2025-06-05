// SPDX-License-Identifier: MIT
pragma solidity = 0.8.30;

contract ArithmeticOverflow {
    constructor() {
        uint8 a = 255;
        uint8 b = a + 1; // Triggers panic code 0x11
    }
}
