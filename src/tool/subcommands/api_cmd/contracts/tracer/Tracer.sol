// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

contract Tracer {
    uint256 public x;
    mapping(address => uint256) public balances;
    event Transfer(address indexed from, address indexed to, uint256 value);

    constructor() payable {
        x = 42;
    }

    // 1. Simple storage write
    function setX(uint256 _x) external {
        x = _x;
    }

    // 2. Balance update (SSTORE)
    function deposit() external payable {
        balances[msg.sender] = msg.value;
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
    function wideTrace(uint256 width, uint256 depth) external returns (uint256 sum) {
        if (depth == 0) {
            return 1;
        }
        
        for (uint256 i = 0; i < width; i++) {
            (bool ok, bytes memory result) = address(this).call(
                abi.encodeWithSelector(this.wideTrace.selector, width, depth - 1)
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
    function failAtDepth(uint256 depth, uint256 failAt) external returns (uint256) {
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
}
