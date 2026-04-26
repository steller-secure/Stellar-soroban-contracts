// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title InputValidation
/// @notice Demonstrates strict input type constraints to prevent invalid state transitions.
contract InputValidation {
    mapping(address => uint256) public balances;

    event Transfer(address indexed from, address indexed to, uint256 amount);

    /// @notice Transfer tokens with strict input constraints.
    /// @param to  Recipient address — must not be the zero address.
    /// @param amount  Amount to transfer — must be > 0 and fit within uint128.
    function transfer(address to, uint256 amount) external {
        require(to != address(0), "InputValidation: zero address recipient");
        require(amount > 0, "InputValidation: amount must be positive");
        require(
            amount <= type(uint128).max,
            "InputValidation: amount exceeds uint128 max"
        );
        require(
            balances[msg.sender] >= amount,
            "InputValidation: insufficient balance"
        );

        balances[msg.sender] -= amount;
        balances[to] += amount;

        emit Transfer(msg.sender, to, amount);
    }

    /// @notice Deposit ether to receive token balance (1:1 for demo purposes).
    function deposit() external payable {
        require(msg.value > 0, "InputValidation: zero deposit");
        balances[msg.sender] += msg.value;
    }
}
