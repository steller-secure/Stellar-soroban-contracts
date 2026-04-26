// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title OwnershipTransfer
/// @notice Two-step ownership transfer to prevent accidental loss of ownership.
contract OwnershipTransfer {
    address public owner;
    address public pendingOwner;

    event OwnershipTransferInitiated(address indexed currentOwner, address indexed pendingOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "OwnershipTransfer: caller is not the owner");
        _;
    }

    /// @notice Initiate an ownership transfer to `newOwner`.
    ///         The new owner must call `acceptOwnership()` to confirm.
    /// @param newOwner The address nominated to become the new owner.
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "OwnershipTransfer: zero address");
        pendingOwner = newOwner;
        emit OwnershipTransferInitiated(owner, newOwner);
    }

    /// @notice Confirm ownership transfer. Must be called by the pending owner.
    function acceptOwnership() external {
        require(msg.sender == pendingOwner, "OwnershipTransfer: caller is not pending owner");
        address previous = owner;
        owner = pendingOwner;
        pendingOwner = address(0);
        emit OwnershipTransferred(previous, owner);
    }
}
