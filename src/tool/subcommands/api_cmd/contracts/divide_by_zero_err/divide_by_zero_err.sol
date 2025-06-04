// SPDX-License-Identifier: MIT
pragma solidity = 0.8.30;

contract DivideByZero {
    constructor() {
        uint256 a = 1;
        uint256 b = 0;
        uint256 _c = a / b; // Triggers panic code 0x12
    }
}
