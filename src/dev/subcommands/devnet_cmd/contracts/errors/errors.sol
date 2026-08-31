// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

/// Lotus `itests/contracts/Errors.sol`, pinned to the compiler this
/// directory's `compile.sh` uses. Skip-sender revert tests call these methods
/// to pin JSON-RPC code, decoded reason, and revert data.
contract Errors {
    error CustomError();

    function failRevertEmpty() public {
        revert();
    }

    function failRevertReason() public {
        revert("my reason");
    }

    function failAssert() public {
        assert(false);
    }

    function failDivZero() public {
        int a = 1;
        int b = 0;
        a / b;
    }

    function failCustom() public {
        revert CustomError();
    }
}
