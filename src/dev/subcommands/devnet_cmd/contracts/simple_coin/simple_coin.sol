// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

/// Lotus `itests/contracts/SimpleCoin.sol`, pinned to the compiler this
/// directory's `compile.sh` uses. `sendCoin` returns whether `balances[msg.sender]`
/// covered the amount, so a skip-sender `eth_call` can report the requested `from`.
contract SimpleCoin {
    mapping(address => uint256) balances;

    event Transfer(address indexed _from, address indexed _to, uint256 _value);

    constructor() {
        balances[tx.origin] = 10000;
    }

    function sendCoin(address receiver, uint256 amount) public returns (bool sufficient) {
        if (balances[msg.sender] < amount) return false;
        balances[msg.sender] -= amount;
        balances[receiver] += amount;
        emit Transfer(msg.sender, receiver, amount);
        return true;
    }

    function getBalanceInEth(address addr) public view returns (uint256) {
        return getBalance(addr) * 2;
    }

    function getBalance(address addr) public view returns (uint256) {
        return balances[addr];
    }
}
