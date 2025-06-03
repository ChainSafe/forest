// SPDX-License-Identifier: MIT
pragma solidity >=0.8.17;

contract PopEmptyArrayFail {
    uint256[] public myArray;
    constructor() {
        myArray.pop(); // Triggers panic code 0x31
    }
}