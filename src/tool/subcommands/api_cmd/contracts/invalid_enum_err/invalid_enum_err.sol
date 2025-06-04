// SPDX-License-Identifier: MIT
pragma solidity = 0.8.30;

contract InvalidEnumFail {
    enum MyEnum { A, B, C } // Max value is 2

    constructor() {
        uint256 val = 100;
        MyEnum _e = MyEnum(val); // Will fail if val >= 3, triggers panic code 0x21
    }
}