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
}
