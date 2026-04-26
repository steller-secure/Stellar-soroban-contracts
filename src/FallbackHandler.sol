// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title FallbackHandler
/// @notice Handles plain ether transfers and unknown function calls gracefully.
contract FallbackHandler {
    event EtherReceived(address indexed sender, uint256 amount);
    event UnknownCall(bytes4 indexed selector);

    /// @notice Triggered when the contract receives plain ether with no calldata.
    receive() external payable {
        emit EtherReceived(msg.sender, msg.value);
    }

    /// @notice Triggered when the contract is called with data that does not
    ///         match any function selector.
    fallback() external payable {
        emit UnknownCall(msg.sig);
    }
}
