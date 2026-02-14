// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.30;

/// @title Tracer Contract
/// @notice Test contract for validating Forest's trace_call RPC implementation.
/// @dev This contract is used internally for:
///   - Integration tests comparing Forest's trace_call output with other implementations (e.g., Anvil)
///   - Manual testing of trace, stateDiff
///   - Generating test vectors for EVM execution tracing
///
/// NOT intended for production use. Functions are designed to exercise specific
/// EVM behaviors (storage writes, subcalls, reverts, events) for trace validation.
///
/// See: docs/docs/developers/guides/trace_call_guide.md
contract Tracer {
    uint256 public x; // slot 0 - initialized to 42
    mapping(address => uint256) public balances; // slot 1 - mapping base

    // Storage slots for stateDiff testing (uninitialized = start empty)
    uint256 public storageTestA; // slot 2 - for add/change/delete tests
    uint256 public storageTestB; // slot 3 - for multiple slot tests
    uint256 public storageTestC; // slot 4 - for multiple slot tests
    uint256[] public dynamicArray; // slot 5 - for array storage tests

    event Transfer(address indexed from, address indexed to, uint256 value);

    constructor() payable {
        x = 42;
    }

    // Allow contract to receive ETH
    receive() external payable {}

    // 1. Simple storage write
    function setX(uint256 _x) external {
        x = _x;
    }

    // 2. Balance update (SSTORE) - Contract receives ETH
    function deposit() external payable {
        balances[msg.sender] = msg.value;
    }

    // 2b. Send ETH to address - Tests balance decrease/increase
    function sendEth(address payable to) external payable {
        to.transfer(msg.value);
    }

    // 2c. Withdraw ETH - Contract sends ETH to caller
    function withdraw(uint256 amount) external {
        require(
            address(this).balance >= amount,
            "insufficient contract balance"
        );
        payable(msg.sender).transfer(amount);
    }

    // 3. Transfer between two accounts (SSTORE x2)
    function transfer(address to, uint256 amount) external {
        require(balances[msg.sender] >= amount, "insufficient balance");
        balances[msg.sender] -= amount;
        balances[to] += amount;
        emit Transfer(msg.sender, to, amount);
    }

    // 4. CALL (external call to self – creates CALL opcode)
    function callSelf(uint256 _x) external {
        (bool ok, ) = address(this).call(
            abi.encodeWithSelector(this.setX.selector, _x)
        );
        require(ok, "call failed");
    }

    // 5. DELEGATECALL (to self – shows delegatecall trace)
    function delegateSelf(uint256 _x) external {
        (bool ok, ) = address(this).delegatecall(
            abi.encodeWithSelector(this.setX.selector, _x)
        );
        require(ok, "delegatecall failed");
    }

    // 6. STATICCALL (read-only)
    function staticRead() external view returns (uint256) {
        return x;
    }

    // 7. CREATE (deploy a tiny contract)
    function createChild() external returns (address child) {
        bytes
            memory code = hex"6080604052348015600f57600080fd5b5060019050601c806100226000396000f3fe6080604052";
        assembly {
            child := create(0, add(code, 0x20), 0x1c)
        }
    }

    // 8. SELFDESTRUCT (send ETH to caller)
    // Deprecated (EIP-6780): selfdestruct only sends ETH (code & storage stay)
    function destroyAndSend() external {
        selfdestruct(payable(msg.sender));
    }

    // 9. Precompile use – keccak256
    function keccakIt(bytes32 input) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(input));
    }

    // 10. Revert
    function doRevert() external pure {
        revert("from some fiasco");
    }

    // ========== DEEP TRACE FUNCTIONS ==========

    // 11. Deep recursive CALL trace
    // Creates trace depth of `depth` levels
    function deepTrace(uint256 depth) external returns (uint256) {
        if (depth == 0) {
            x = x + 1; // Storage write at deepest level
            return x;
        }
        (bool ok, bytes memory result) = address(this).call(
            abi.encodeWithSelector(this.deepTrace.selector, depth - 1)
        );
        require(ok, "deep call failed");
        return abi.decode(result, (uint256));
    }

    // 12. Mixed call types trace
    // Alternates between CALL, DELEGATECALL, and STATICCALL
    function mixedTrace(uint256 depth) external returns (uint256) {
        if (depth == 0) {
            return x;
        }

        uint256 callType = depth % 3;

        if (callType == 0) {
            // Regular CALL
            (bool ok, bytes memory result) = address(this).call(
                abi.encodeWithSelector(this.mixedTrace.selector, depth - 1)
            );
            require(ok, "call failed");
            return abi.decode(result, (uint256));
        } else if (callType == 1) {
            // DELEGATECALL
            (bool ok, bytes memory result) = address(this).delegatecall(
                abi.encodeWithSelector(this.mixedTrace.selector, depth - 1)
            );
            require(ok, "delegatecall failed");
            return abi.decode(result, (uint256));
        } else {
            // STATICCALL (read-only)
            (bool ok, bytes memory result) = address(this).staticcall(
                abi.encodeWithSelector(this.mixedTrace.selector, depth - 1)
            );
            require(ok, "staticcall failed");
            return abi.decode(result, (uint256));
        }
    }

    // 13. Wide trace - multiple sibling calls at same level
    // Creates `width` parallel calls, each going `depth` levels deep
    // Example: wideTrace(3, 2) creates 3 siblings, each 2 levels deep
    function wideTrace(
        uint256 width,
        uint256 depth
    ) external returns (uint256 sum) {
        if (depth == 0) {
            return 1;
        }

        for (uint256 i = 0; i < width; i++) {
            (bool ok, bytes memory result) = address(this).call(
                abi.encodeWithSelector(
                    this.wideTrace.selector,
                    width,
                    depth - 1
                )
            );
            require(ok, "wide call failed");
            sum += abi.decode(result, (uint256));
        }
        return sum;
    }

    // 14. Complex trace - combines everything
    // Level 0: CALL to setX
    // Level 1: DELEGATECALL to inner
    // Level 2: Multiple CALLs
    // Level 3: STATICCALL
    function complexTrace() external returns (uint256) {
        // First: regular call to setX
        (bool ok1, ) = address(this).call(
            abi.encodeWithSelector(this.setX.selector, 100)
        );
        require(ok1, "setX failed");

        // Second: delegatecall that does more calls
        (bool ok2, bytes memory result) = address(this).delegatecall(
            abi.encodeWithSelector(this.innerComplex.selector)
        );
        require(ok2, "innerComplex failed");

        return abi.decode(result, (uint256));
    }

    // Helper for complexTrace
    function innerComplex() external returns (uint256) {
        // Multiple sibling calls
        (bool ok1, ) = address(this).call(
            abi.encodeWithSelector(this.setX.selector, 200)
        );
        require(ok1, "inner call 1 failed");

        (bool ok2, ) = address(this).call(
            abi.encodeWithSelector(this.setX.selector, 300)
        );
        require(ok2, "inner call 2 failed");

        // Staticcall to read
        (bool ok3, bytes memory result) = address(this).staticcall(
            abi.encodeWithSelector(this.staticRead.selector)
        );
        require(ok3, "staticcall failed");

        return abi.decode(result, (uint256));
    }

    // 15. Failing nested trace - revert at depth
    // Useful for testing partial trace on failure
    function failAtDepth(
        uint256 depth,
        uint256 failAt
    ) external returns (uint256) {
        if (depth == failAt) {
            revert("intentional failure at depth");
        }
        if (depth == 0) {
            return x;
        }
        (bool ok, bytes memory result) = address(this).call(
            abi.encodeWithSelector(this.failAtDepth.selector, depth - 1, failAt)
        );
        require(ok, "nested call failed");
        return abi.decode(result, (uint256));
    }

    // ========== STORAGE DIFF TEST FUNCTIONS ==========
    // These functions test stateDiff storage tracking:
    // - Added (+): Write non-zero to empty slot (slot was 0)
    // - Changed (*): Change non-zero to different non-zero
    // - Removed (-): Set non-zero slot to 0

    // 16. Storage Add - Write to empty slot
    // First call creates "Added" (+) entry in stateDiff.storage
    function storageAdd(uint256 value) external {
        require(value != 0, "use non-zero for add test");
        storageTestA = value;
    }

    // 17. Storage Change - Modify existing slot
    // Creates "Changed" (*) entry in stateDiff.storage
    function storageChange(uint256 newValue) external {
        require(storageTestA != 0, "slot must have value first");
        require(
            newValue != 0 && newValue != storageTestA,
            "use different non-zero"
        );
        storageTestA = newValue;
    }

    // 18. Storage Delete - Set slot to zero
    // Creates "Removed" (-) entry in stateDiff.storage
    function storageDelete() external {
        require(storageTestA != 0, "slot must have value first");
        storageTestA = 0;
    }

    // 19. Storage Multiple - Change multiple slots in one call
    // Useful for testing multiple storage entries in stateDiff
    function storageMultiple(uint256 a, uint256 b, uint256 c) external {
        storageTestA = a;
        storageTestB = b;
        storageTestC = c;
    }

    // 20. Storage Mixed - Add, Change, and Delete in one call
    // Requires: storageTestA has value, storageTestB is empty
    function storageMixed(uint256 newA, uint256 newC) external {
        // Change existing (storageTestA should have value)
        storageTestA = newA;
        // Delete (set to 0)
        storageTestB = 0;
        // Add new value
        storageTestC = newC;
    }

    // 21. Array Push - Adds new storage slot
    // Dynamic arrays use keccak256(slot) + index for element storage
    function arrayPush(uint256 value) external {
        dynamicArray.push(value);
    }

    // 22. Array Pop - Removes storage slot (sets to 0)
    function arrayPop() external {
        require(dynamicArray.length > 0, "array is empty");
        dynamicArray.pop();
    }

    // 23. Reset storage test slots to initial state (all zeros)
    function storageReset() external {
        storageTestA = 0;
        storageTestB = 0;
        storageTestC = 0;
        delete dynamicArray;
    }

    // 24. Get storage test values (for verification)
    function getStorageTestValues()
        external
        view
        returns (uint256, uint256, uint256, uint256)
    {
        return (storageTestA, storageTestB, storageTestC, dynamicArray.length);
    }
}
