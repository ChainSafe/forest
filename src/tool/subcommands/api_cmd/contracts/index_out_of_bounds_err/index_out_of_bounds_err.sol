// SPDX-License-Identifier: MIT
pragma solidity >=0.8.17;

contract ArrayIndexOutOfBoundsFail {
    constructor() {
        uint256[] memory memArr = new uint256[](1);
        memArr[0] = 10;
        uint256 _res = memArr[1]; // Accessing index 1 is out of bounds, triggers panic code 0x32
    }
}