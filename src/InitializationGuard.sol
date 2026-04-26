// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title InitializationGuard
/// @notice Prevents a contract from being initialized more than once.
contract InitializationGuard {
    bool public initialized;
    address public admin;

    event Initialized(address indexed admin);

    /// @dev Reverts if the contract has already been initialized.
    modifier notInitialized() {
        require(!initialized, "InitializationGuard: already initialized");
        _;
    }

    /// @notice Initialize the contract with an admin address.
    ///         Can only be called once.
    /// @param _admin The address to set as admin.
    function initialize(address _admin) external notInitialized {
        require(_admin != address(0), "InitializationGuard: zero address admin");
        initialized = true;
        admin = _admin;
        emit Initialized(_admin);
    }
}
