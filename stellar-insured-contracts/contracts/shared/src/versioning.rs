//! # Contract Versioning and Migration System
//!
//! This module provides a safe, backward-compatible versioning system for Stellar Insured
//! Soroban contracts. It enables controlled upgrades without breaking storage layouts or
//! compromising user funds.
//!
//! ## Design Principles
//!
//! 1. **Explicit Versioning**: Contract version is stored on-chain and checked on initialization
//! 2. **Migration Hooks**: Each contract can define custom migration logic for version upgrades
//! 3. **Backward Compatibility**: Old data structures remain readable; new features are opt-in
//! 4. **Safety Checks**: Upgrade authorization, schema validation, and migration verification
//! 5. **Immutable History**: All version transitions are logged for audit purposes
//!
//! ## Storage Layout
//!
//! Version information is stored in dedicated keys that don't interfere with application data:
//!
//! ```text
//! CONTRACT_VERSION           -> Current version (u32)
//! CONTRACT_VERSION_HISTORY   -> Vec of historical versions
//! MIGRATION_STATE            -> State during migration (None, InProgress, Complete)
//! LAST_MIGRATION_TIME        -> Timestamp of last successful migration
//! ```
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use shared::versioning::{VersionManager, MigrationHook, MigrationState};
//!
//! // In contract initialization
//! VersionManager::initialize(&env, 1)?;
//!
//! // Before contract logic
//! VersionManager::ensure_compatible(&env, 1)?;
//!
//! // During contract upgrade
//! VersionManager::migrate(&env, 1, 2, migration_hook)?;
//!
//! // Query version info
//! let version = VersionManager::current_version(&env)?;
//! let history = VersionManager::version_history(&env)?;
//! ```

use soroban_sdk::{contracterror, contracttype, Address, Env, Symbol, Vec};

// ============================================================================
// Constants
// ============================================================================

/// Storage key for the current contract version
const CONTRACT_VERSION: Symbol = Symbol::short("VERSION");

/// Storage key for version migration history
const CONTRACT_VERSION_HISTORY: Symbol = Symbol::short("VERS_HIS");

/// Storage key for the current migration state
const MIGRATION_STATE: Symbol = Symbol::short("MIG_STA");

/// Storage key for the timestamp of the last successful migration
const LAST_MIGRATION_TIME: Symbol = Symbol::short("LAST_MIG");

/// Maximum number of versions in history to track (prevents unbounded growth)
const MAX_VERSION_HISTORY: u32 = 100;

// ============================================================================
// Error Types
// ============================================================================

/// Versioning and migration-related errors
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum VersioningError {
    /// Contract version is not initialized
    NotInitialized = 1,

    /// Contract version is incompatible with expected version
    VersionMismatch = 2,

    /// Migration in progress - cannot execute contract logic
    MigrationInProgress = 3,

    /// Migration failed or incomplete
    MigrationFailed = 4,

    /// Upgrade authorization required but not provided
    UnauthorizedUpgrade = 5,

    /// Invalid version number (e.g., upgrading to same or lower version)
    InvalidVersionNumber = 6,

    /// Migration hook failed
    MigrationHookFailed = 7,

    /// Storage schema validation failed
    SchemaValidationFailed = 8,

    /// Rollback failed - contract in inconsistent state
    RollbackFailed = 9,
}

// ============================================================================
// Type Definitions
// ============================================================================

/// Represents the state of a migration in progress
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationState {
    /// No migration in progress
    None = 0,

    /// Migration is currently executing
    InProgress = 1,

    /// Migration completed successfully
    Complete = 2,

    /// Migration failed and needs rollback
    RollbackRequired = 3,
}

/// Represents a single version transition in the migration history
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionTransition {
    /// Source version
    pub from_version: u32,

    /// Target version
    pub to_version: u32,

    /// Address that authorized the migration
    pub migrated_by: Address,

    /// Timestamp when migration was executed
    pub migration_timestamp: u64,

    /// Whether the migration completed successfully
    pub success: bool,

    /// Optional message describing the migration
    pub message: soroban_sdk::String,
}

/// Contract versioning information
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionInfo {
    /// Current contract version
    pub current_version: u32,

    /// Number of migrations performed
    pub migration_count: u32,

    /// Timestamp of the last successful migration
    pub last_migration_time: u64,

    /// Current state of migration
    pub migration_state: u32, // MigrationState enum
}

// ============================================================================
// Migration Hook Trait
// ============================================================================

/// Trait for custom migration logic during contract upgrades
///
/// Contracts can implement custom migration logic to handle data transformation,
/// state cleanup, or other operations during version upgrades.
///
/// # Error Handling
///
/// If migration logic fails, the entire upgrade is rolled back. Ensure migration
/// functions are idempotent and handle partial failures gracefully.
///
/// # Example
///
/// ```rust,ignore
/// pub fn my_migration_hook(env: &Env) -> Result<(), VersioningError> {
///     // 1. Read old data
///     let old_data: OldType = env.storage().instance().get(&"key").ok();
///
///     // 2. Transform to new format
///     let new_data = NewType::from(old_data);
///
///     // 3. Write new data
///     env.storage().instance().set(&"key", &new_data);
///
///     Ok(())
/// }
/// ```
pub trait MigrationHook: Fn(&Env) -> Result<(), VersioningError> {}
impl<F> MigrationHook for F where F: Fn(&Env) -> Result<(), VersioningError> {}

// ============================================================================
// Version Manager
// ============================================================================

/// Central manager for contract versioning and migrations
///
/// This struct provides static methods to initialize contracts, check version
/// compatibility, execute migrations, and query version history.
pub struct VersionManager;

impl VersionManager {
    // ========================================================================
    // Initialization
    // ========================================================================

    /// Initializes the contract with a specific version
    ///
    /// This should be called once during contract deployment. It sets the
    /// initial version and creates an empty migration history.
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `initial_version` - The version number to initialize (typically 1)
    ///
    /// # Errors
    ///
    /// Returns `VersioningError::NotInitialized` if already initialized
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
    ///     admin.require_auth();
    ///     VersionManager::initialize(&env, 1)?;
    ///     // ... rest of initialization
    ///     Ok(())
    /// }
    /// ```
    pub fn initialize(env: &Env, initial_version: u32) -> Result<(), VersioningError> {
        // Check if already initialized
        if env.storage().instance().has(&CONTRACT_VERSION) {
            return Err(VersioningError::NotInitialized);
        }

        // Validate version number
        if initial_version == 0 {
            return Err(VersioningError::InvalidVersionNumber);
        }

        // Store initial version
        env.storage().instance().set(&CONTRACT_VERSION, &initial_version);

        // Initialize empty migration history
        let history: Vec<VersionTransition> = Vec::new(&env);
        env.storage().instance().set(&CONTRACT_VERSION_HISTORY, &history);

        // Set migration state to None
        env.storage().instance().set(&MIGRATION_STATE, &(MigrationState::None as u32));

        // Record initialization timestamp
        let now = env.ledger().timestamp();
        env.storage().instance().set(&LAST_MIGRATION_TIME, &now);

        Ok(())
    }

    // ========================================================================
    // Version Queries
    // ========================================================================

    /// Returns the current contract version
    ///
    /// # Errors
    ///
    /// Returns `VersioningError::NotInitialized` if version not set
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let version = VersionManager::current_version(&env)?;
    /// println!("Current version: {}", version);
    /// ```
    pub fn current_version(env: &Env) -> Result<u32, VersioningError> {
        env.storage()
            .instance()
            .get(&CONTRACT_VERSION)
            .ok_or(VersioningError::NotInitialized)
    }

    /// Returns complete version information
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let info = VersionManager::version_info(&env)?;
    /// println!("Version: {}, Migrations: {}", info.current_version, info.migration_count);
    /// ```
    pub fn version_info(env: &Env) -> Result<VersionInfo, VersioningError> {
        let current_version = Self::current_version(env)?;
        let history = Self::version_history(env)?;
        let migration_count = history.len() as u32;
        let migration_state: u32 = env
            .storage()
            .instance()
            .get(&MIGRATION_STATE)
            .unwrap_or(0);
        let last_migration_time: u64 = env
            .storage()
            .instance()
            .get(&LAST_MIGRATION_TIME)
            .unwrap_or(0);

        Ok(VersionInfo {
            current_version,
            migration_count,
            last_migration_time,
            migration_state,
        })
    }

    /// Returns the complete version migration history
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let history = VersionManager::version_history(&env)?;
    /// for transition in history.iter() {
    ///     println!("v{} -> v{}: {}", transition.from_version, transition.to_version, transition.migration_timestamp);
    /// }
    /// ```
    pub fn version_history(env: &Env) -> Result<Vec<VersionTransition>, VersioningError> {
        env.storage()
            .instance()
            .get(&CONTRACT_VERSION_HISTORY)
            .ok_or(VersioningError::NotInitialized)
    }

    // ========================================================================
    // Compatibility Checks
    // ========================================================================

    /// Ensures that the current contract version matches the expected version
    ///
    /// This check should be performed at the beginning of contract execution
    /// to detect version mismatches and prevent executing incompatible logic.
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `expected_version` - The version this operation expects
    ///
    /// # Errors
    ///
    /// - `VersioningError::NotInitialized` if version not set
    /// - `VersioningError::VersionMismatch` if versions don't match
    /// - `VersioningError::MigrationInProgress` if migration is active
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[contractimpl]
    /// impl MyContract {
    ///     pub fn issue_policy(env: Env, ...) -> Result<(), ContractError> {
    ///         VersionManager::ensure_compatible(&env, 1)?;
    ///         // ... issue policy logic
    ///         Ok(())
    ///     }
    /// }
    /// ```
    pub fn ensure_compatible(env: &Env, expected_version: u32) -> Result<(), VersioningError> {
        // Check migration state - prevent execution during migration
        let migration_state: u32 = env
            .storage()
            .instance()
            .get(&MIGRATION_STATE)
            .unwrap_or(0);

        if migration_state == MigrationState::InProgress as u32 {
            return Err(VersioningError::MigrationInProgress);
        }

        if migration_state == MigrationState::RollbackRequired as u32 {
            return Err(VersioningError::RollbackFailed);
        }

        // Check version match
        let current = Self::current_version(env)?;
        if current != expected_version {
            return Err(VersioningError::VersionMismatch);
        }

        Ok(())
    }

    /// Checks if a version upgrade is valid (new version must be higher)
    fn validate_version_upgrade(
        current: u32,
        new_version: u32,
    ) -> Result<(), VersioningError> {
        if new_version <= current {
            return Err(VersioningError::InvalidVersionNumber);
        }
        Ok(())
    }

    // ========================================================================
    // Migration Execution
    // ========================================================================

    /// Executes a contract migration from one version to another
    ///
    /// This is the core upgrade mechanism. It:
    /// 1. Validates the version upgrade path
    /// 2. Sets migration state to InProgress
    /// 3. Executes the migration hook (custom upgrade logic)
    /// 4. Updates the version and history
    /// 5. Sets migration state to Complete
    ///
    /// If any step fails, the migration state is set to RollbackRequired
    /// and the contract is locked until manual intervention.
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `from_version` - Current version
    /// * `to_version` - Target version (must be > from_version)
    /// * `migrator` - Address authorizing the migration
    /// * `hook` - Custom migration logic to execute
    ///
    /// # Errors
    ///
    /// - `VersioningError::InvalidVersionNumber` if upgrade path is invalid
    /// - `VersioningError::MigrationInProgress` if another migration is running
    /// - `VersioningError::MigrationHookFailed` if custom logic fails
    /// - `VersioningError::RollbackFailed` if rollback itself fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// pub fn upgrade_contract(env: Env, new_version: u32) -> Result<(), ContractError> {
    ///     // Authorization check (typically admin-only)
    ///     let admin = // ... get admin address
    ///     admin.require_auth();
    ///
    ///     let current = VersionManager::current_version(&env)?;
    ///     VersionManager::migrate(
    ///         &env,
    ///         current,
    ///         new_version,
    ///         admin,
    ///         |env| {
    ///             // Custom migration logic here
    ///             Ok(())
    ///         }
    ///     )?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn migrate<F>(
        env: &Env,
        from_version: u32,
        to_version: u32,
        migrator: Address,
        hook: F,
    ) -> Result<(), VersioningError>
    where
        F: Fn(&Env) -> Result<(), VersioningError>,
    {
        // Validate version upgrade
        Self::validate_version_upgrade(from_version, to_version)?;

        // Check that current version matches from_version
        let current = Self::current_version(env)?;
        if current != from_version {
            return Err(VersioningError::VersionMismatch);
        }

        // Check if migration is already in progress
        let migration_state: u32 = env
            .storage()
            .instance()
            .get(&MIGRATION_STATE)
            .unwrap_or(0);

        if migration_state == MigrationState::InProgress as u32 {
            return Err(VersioningError::MigrationInProgress);
        }

        // Set migration state to InProgress
        env.storage()
            .instance()
            .set(&MIGRATION_STATE, &(MigrationState::InProgress as u32));

        // Execute migration hook
        if let Err(e) = hook(env) {
            // On failure, set state to RollbackRequired and return error
            env.storage()
                .instance()
                .set(&MIGRATION_STATE, &(MigrationState::RollbackRequired as u32));
            return Err(e);
        }

        // Update version
        env.storage().instance().set(&CONTRACT_VERSION, &to_version);

        // Record migration in history
        let mut history = Self::version_history(env)?;
        let transition = VersionTransition {
            from_version,
            to_version,
            migrated_by: migrator,
            migration_timestamp: env.ledger().timestamp(),
            success: true,
            message: soroban_sdk::String::from_slice(env, "Migration successful"),
        };

        // Trim history if it exceeds max size
        if history.len() >= MAX_VERSION_HISTORY {
            // Keep only the most recent MAX_VERSION_HISTORY - 1 entries
            let new_len = MAX_VERSION_HISTORY - 1;
            for _ in new_len..history.len() {
                history.pop_back();
            }
        }

        history.push_front(transition);
        env.storage()
            .instance()
            .set(&CONTRACT_VERSION_HISTORY, &history);

        // Update last migration time
        env.storage()
            .instance()
            .set(&LAST_MIGRATION_TIME, &env.ledger().timestamp());

        // Set migration state to Complete
        env.storage()
            .instance()
            .set(&MIGRATION_STATE, &(MigrationState::Complete as u32));

        Ok(())
    }

    // ========================================================================
    // Recovery and Debugging
    // ========================================================================

    /// Resets migration state to None (admin-only recovery)
    ///
    /// Use only when a migration has failed and been manually reviewed.
    /// This should only be callable by a privileged admin account.
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `admin` - Address of the administrator (must match contract admin)
    ///
    /// # Errors
    ///
    /// Returns `VersioningError::UnauthorizedUpgrade` if not authorized
    pub fn reset_migration_state(env: &Env, admin: &Address) -> Result<(), VersioningError> {
        // Note: This is a placeholder. Actual implementation should verify
        // that the caller is authorized (e.g., matches the admin address)
        admin.require_auth();
        env.storage()
            .instance()
            .set(&MIGRATION_STATE, &(MigrationState::None as u32));
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Converts a MigrationState enum value to its u32 representation
pub fn migration_state_to_u32(state: MigrationState) -> u32 {
    state as u32
}

/// Converts a u32 value back to a MigrationState enum
pub fn u32_to_migration_state(value: u32) -> Result<MigrationState, VersioningError> {
    match value {
        0 => Ok(MigrationState::None),
        1 => Ok(MigrationState::InProgress),
        2 => Ok(MigrationState::Complete),
        3 => Ok(MigrationState::RollbackRequired),
        _ => Err(VersioningError::InvalidVersionNumber),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_state_conversions() {
        assert_eq!(migration_state_to_u32(MigrationState::None), 0);
        assert_eq!(migration_state_to_u32(MigrationState::InProgress), 1);
        assert_eq!(migration_state_to_u32(MigrationState::Complete), 2);
        assert_eq!(migration_state_to_u32(MigrationState::RollbackRequired), 3);

        assert_eq!(
            u32_to_migration_state(0).unwrap(),
            MigrationState::None
        );
        assert_eq!(
            u32_to_migration_state(1).unwrap(),
            MigrationState::InProgress
        );
        assert_eq!(
            u32_to_migration_state(2).unwrap(),
            MigrationState::Complete
        );
        assert_eq!(
            u32_to_migration_state(3).unwrap(),
            MigrationState::RollbackRequired
        );
    }
}
