//! # Upgradeable Contract Base
//!
//! Provides a base implementation for upgradeable contracts with built-in version
//! management, migration hooks, and safety mechanisms.
//!
//! ## Contract Structure
//!
//! Any contract using this base should:
//! 1. Initialize with `UpgradeableContract::initialize()`
//! 2. Check version compatibility at the start of each function with `ensure_version_compatible()`
//! 3. Implement custom migration logic as needed
//!
//! ## Storage Safety
//!
//! This module reserves the following storage keys for version management:
//! - `CONTRACT_VERSION`
//! - `CONTRACT_VERSION_HISTORY`
//! - `MIGRATION_STATE`
//! - `LAST_MIGRATION_TIME`
//!
//! Application data should use different keys to avoid conflicts.

use crate::versioning::{VersionManager, VersioningError, MigrationState};
use soroban_sdk::{Address, Env};

// ============================================================================
// Upgradeable Contract Base
// ============================================================================

/// Base implementation for upgradeable contracts
///
/// This struct provides convenience methods for version management.
/// Contracts can use these methods directly or implement their own wrapping.
pub struct UpgradeableContract;

impl UpgradeableContract {
    /// Initializes an upgradeable contract with version 1
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    ///
    /// # Errors
    ///
    /// Returns an error if the contract is already initialized
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
    ///     admin.require_auth();
    ///     UpgradeableContract::initialize(&env)?;
    ///     // ... rest of initialization
    ///     Ok(())
    /// }
    /// ```
    pub fn initialize(env: &Env) -> Result<(), VersioningError> {
        VersionManager::initialize(env, 1)
    }

    /// Initializes with a specific version number
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `version` - Initial version number
    ///
    /// # Errors
    ///
    /// Returns an error if the contract is already initialized or version is invalid
    pub fn initialize_with_version(env: &Env, version: u32) -> Result<(), VersioningError> {
        VersionManager::initialize(env, version)
    }

    /// Ensures the contract version is compatible with the expected version
    ///
    /// This should be called at the beginning of each contract function
    /// to prevent execution during migrations.
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `expected_version` - The version this function expects
    ///
    /// # Errors
    ///
    /// Returns an error if version doesn't match or migration is in progress
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// pub fn transfer(env: Env, to: Address, amount: i128) -> Result<(), ContractError> {
    ///     UpgradeableContract::ensure_version_compatible(&env, 1)?;
    ///     // ... transfer logic
    ///     Ok(())
    /// }
    /// ```
    pub fn ensure_version_compatible(env: &Env, expected_version: u32) -> Result<(), VersioningError> {
        VersionManager::ensure_compatible(env, expected_version)
    }

    /// Performs a contract upgrade with custom migration logic
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `new_version` - Target version (must be > current)
    /// * `migrator` - Address authorizing the migration
    /// * `hook` - Closure with custom migration logic
    ///
    /// # Errors
    ///
    /// Returns an error if upgrade is invalid or migration hook fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// pub fn upgrade(env: Env, new_version: u32) -> Result<(), ContractError> {
    ///     // Authorization check
    ///     let admin = get_admin(&env)?;
    ///     admin.require_auth();
    ///
    ///     let current = VersionManager::current_version(&env)?;
    ///     UpgradeableContract::upgrade(
    ///         &env,
    ///         current,
    ///         new_version,
    ///         admin,
    ///         |env| {
    ///             // Custom migration logic
    ///             // Example: Transform data structures from v1 to v2
    ///             Ok(())
    ///         }
    ///     )?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn upgrade<F>(
        env: &Env,
        current_version: u32,
        new_version: u32,
        migrator: Address,
        hook: F,
    ) -> Result<(), VersioningError>
    where
        F: Fn(&Env) -> Result<(), VersioningError>,
    {
        VersionManager::migrate(env, current_version, new_version, migrator, hook)
    }

    /// Gets the current contract version
    pub fn current_version(env: &Env) -> Result<u32, VersioningError> {
        VersionManager::current_version(env)
    }

    /// Gets complete version information
    pub fn version_info(env: &Env) -> Result<crate::versioning::VersionInfo, VersioningError> {
        VersionManager::version_info(env)
    }

    /// Gets the version migration history
    pub fn version_history(env: &Env) -> Result<soroban_sdk::Vec<crate::versioning::VersionTransition>, VersioningError> {
        VersionManager::version_history(env)
    }

    /// Recovery function to reset migration state after manual intervention
    pub fn reset_migration_state(env: &Env, admin: &Address) -> Result<(), VersioningError> {
        VersionManager::reset_migration_state(env, admin)
    }
}

// ============================================================================
// Version-Safe Storage Patterns
// ============================================================================

/// Helper trait for safe storage patterns that work across upgrades
pub trait VersionSafeStorage {
    /// Check if storage layout is compatible with current version
    fn check_layout_compatibility(&self, env: &Env, expected_version: u32) -> Result<(), VersioningError>;
}

// ============================================================================
// Migration Helper Functions
// ============================================================================

/// Default no-op migration hook (used when no custom logic is needed)
pub fn default_migration_hook(_env: &Env) -> Result<(), VersioningError> {
    Ok(())
}

/// Create a migration hook that logs the migration event
pub fn logged_migration_hook(description: &str) -> impl Fn(&Env) -> Result<(), VersioningError> + '_ {
    move |_env: &Env| {
        // In a real implementation, this would log the migration event
        // For now, this serves as a documented pattern
        Ok(())
    }
}

/// Creates a composition of multiple migration hooks
///
/// Useful when you need to execute multiple migration steps in sequence.
///
/// # Example
///
/// ```rust,ignore
/// let migration = compose_hooks(
///     |env| migrate_data_v1_to_v2(env),
///     |env| cleanup_old_keys(env),
/// );
/// ```
pub fn compose_hooks<F1, F2>(
    first: F1,
    second: F2,
) -> impl Fn(&Env) -> Result<(), VersioningError>
where
    F1: Fn(&Env) -> Result<(), VersioningError>,
    F2: Fn(&Env) -> Result<(), VersioningError>,
{
    move |env: &Env| {
        first(env)?;
        second(env)?;
        Ok(())
    }
}

// ============================================================================
// Testing Utilities
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_migration_hook() {
        // This is a placeholder test for the default migration hook
        // In actual tests, you would use a mock Env
    }
}
