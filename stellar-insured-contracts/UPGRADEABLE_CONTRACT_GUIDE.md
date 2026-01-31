# Upgradeable Contract Architecture & Versioning

## Overview

The Stellar Insured Soroban contracts implement a safe, backward-compatible versioning system that enables controlled upgrades without breaking storage layouts or compromising user funds. This document describes the upgrade architecture, migration process, and best practices.

## Design Principles

### 1. Explicit Versioning
- Contract version is stored on-chain and checked before each operation
- Version number is incremented for each upgrade (no downgrades allowed)
- Version history is maintained for audit purposes

### 2. Migration Hooks
- Contracts can define custom migration logic for version upgrades
- Hooks can transform data structures, clean up old storage, or validate invariants
- Hooks are optional; simple upgrades can use no-op migrations

### 3. Backward Compatibility
- Old data structures remain readable during migration period
- New features can be gradually rolled out
- Storage keys don't change (data is transformed in-place)

### 4. Safety Checks
- Upgrade authorization (typically admin-only)
- Schema validation before and after migration
- Invariant checks to ensure protocol consistency
- Atomic transactions prevent partial upgrades

### 5. Immutable History
- All version transitions are logged
- Non-authorized upgrades are rejected
- Rollback capability for failed migrations (manual intervention)

## Storage Architecture

Version information is stored in reserved keys that never conflict with application data:

```text
Key: CONTRACT_VERSION              Type: u32
     → Current version number

Key: CONTRACT_VERSION_HISTORY      Type: Vec<VersionTransition>
     → Historical record of all migrations
     → Limited to 100 entries (oldest removed first)

Key: MIGRATION_STATE               Type: u32 (MigrationState enum)
     → Tracks current migration progress
     → Values: None(0), InProgress(1), Complete(2), RollbackRequired(3)

Key: LAST_MIGRATION_TIME           Type: u64
     → Timestamp of last successful migration
     → Used for audit and analysis
```

## Contract Upgrade Workflow

### Phase 1: Initialization

When a contract is first deployed:

```rust
#[contractimpl]
impl MyContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        
        // Initialize contract versioning
        UpgradeableContract::initialize(&env)?;
        
        // Initialize application state
        // ...
        
        Ok(())
    }
}
```

### Phase 2: Normal Operation

Before each contract function executes, check version compatibility:

```rust
#[contractimpl]
impl MyContract {
    pub fn issue_policy(env: Env, holder: Address, amount: i128) -> Result<(), ContractError> {
        // Check version compatibility
        // Prevents execution during migrations
        // Ensures this code is compatible with the contract version
        UpgradeableContract::ensure_version_compatible(&env, 1)?;
        
        // Business logic
        // ...
        
        Ok(())
    }
}
```

### Phase 3: Upgrade Execution

When upgrading to a new version:

```rust
#[contractimpl]
impl MyContract {
    pub fn upgrade(env: Env, new_version: u32) -> Result<(), ContractError> {
        // Get admin address (implementation-specific)
        let admin = get_admin(&env)?;
        admin.require_auth();
        
        // Get current version
        let current = VersionManager::current_version(&env)?;
        
        // Execute migration with custom logic
        UpgradeableContract::upgrade(
            &env,
            current,
            new_version,
            admin,
            |env| {
                // Custom migration logic here
                migrate_v1_to_v2(env)?;
                Ok(())
            }
        )?;
        
        Ok(())
    }
}
```

### Phase 4: Post-Upgrade Verification

After an upgrade, verify the contract is operational:

```rust
// Query version info
let info = VersionManager::version_info(&env)?;
assert_eq!(info.current_version, 2);
assert_eq!(info.migration_state, MigrationState::Complete as u32);

// Query migration history
let history = VersionManager::version_history(&env)?;
assert_eq!(history.len(), 1);
assert_eq!(history[0].from_version, 1);
assert_eq!(history[0].to_version, 2);
```

## Migration Hooks

### Purpose

Migration hooks execute custom logic during upgrades. They can:
- Transform data from old format to new format
- Clean up deprecated storage keys
- Validate invariants
- Re-index data
- Perform any complex upgrade logic

### Hook Signature

```rust
fn migration_hook(env: &Env) -> Result<(), VersioningError>
```

### Example 1: Simple Data Transformation

Upgrade a policy structure from v1 to v2 (add new field):

```rust
fn migrate_policy_v1_to_v2(env: &Env) -> Result<(), VersioningError> {
    // Read old policy data
    let mut policies: Vec<PolicyV1> = env.storage()
        .instance()
        .get(&"POLICIES")
        .unwrap_or(Vec::new(env));
    
    // Transform to new format
    let new_policies: Vec<PolicyV2> = policies
        .iter()
        .map(|old_policy| PolicyV2 {
            id: old_policy.id,
            holder: old_policy.holder,
            coverage_amount: old_policy.coverage_amount,
            premium_amount: old_policy.premium_amount,
            start_time: old_policy.start_time,
            end_time: old_policy.end_time,
            state: old_policy.state,
            created_at: old_policy.created_at,
            // New field with default value
            metadata: PolicyMetadata::default(),
        })
        .collect();
    
    // Write new format
    env.storage().instance().set(&"POLICIES", &new_policies);
    
    Ok(())
}
```

### Example 2: Invariant Validation

Ensure all invariants hold after migration:

```rust
fn validate_invariants_after_upgrade(env: &Env) -> Result<(), VersioningError> {
    // Check total supply consistency
    let balance = ProtocolInvariants::check_balance_invariant(env)
        .map_err(|_| VersioningError::SchemaValidationFailed)?;
    
    // Check all policies are in valid states
    let all_valid = ProtocolInvariants::validate_all_policy_states(env)
        .map_err(|_| VersioningError::SchemaValidationFailed)?;
    
    // Check claim status transitions
    ProtocolInvariants::validate_claim_status_transitions(env)
        .map_err(|_| VersioningError::SchemaValidationFailed)?;
    
    Ok(())
}
```

### Example 3: Composing Multiple Hooks

Execute multiple migration steps:

```rust
fn upgrade_to_v3(env: &Env) -> Result<(), VersioningError> {
    // Step 1: Transform policy data
    migrate_policy_v2_to_v3(env)?;
    
    // Step 2: Transform claim data
    migrate_claims_v2_to_v3(env)?;
    
    // Step 3: Rebuild indexes
    rebuild_policy_index(env)?;
    
    // Step 4: Validate invariants
    validate_invariants_after_upgrade(env)?;
    
    Ok(())
}
```

## Version Management API

### VersionManager - Core API

```rust
/// Initialize contract with version 1
VersionManager::initialize(&env, 1)?;

/// Initialize with specific version
VersionManager::initialize(&env, 2)?;

/// Get current version
let version = VersionManager::current_version(&env)?;

/// Get complete version information
let info = VersionManager::version_info(&env)?;
// {
//   current_version: 1,
//   migration_count: 0,
//   last_migration_time: 1234567890,
//   migration_state: 0 (None)
// }

/// Get migration history
let history = VersionManager::version_history(&env)?;
// Returns Vec<VersionTransition>

/// Execute migration
VersionManager::migrate(&env, 1, 2, migrator, |env| {
    // Custom logic
    Ok(())
})?;

/// Check version compatibility
VersionManager::ensure_compatible(&env, 1)?;

/// Reset migration state (admin-only recovery)
VersionManager::reset_migration_state(&env, &admin)?;
```

### UpgradeableContract - Convenience Wrapper

```rust
/// Initialize with version 1
UpgradeableContract::initialize(&env)?;

/// Initialize with specific version
UpgradeableContract::initialize_with_version(&env, 2)?;

/// Ensure version compatibility (use in contract functions)
UpgradeableContract::ensure_version_compatible(&env, 1)?;

/// Execute upgrade
UpgradeableContract::upgrade(&env, 1, 2, admin, |env| {
    // Custom migration
    Ok(())
})?;

/// Get current version
let version = UpgradeableContract::current_version(&env)?;

/// Get version info
let info = UpgradeableContract::version_info(&env)?;

/// Get history
let history = UpgradeableContract::version_history(&env)?;

/// Recovery: reset migration state
UpgradeableContract::reset_migration_state(&env, &admin)?;
```

## Error Handling

The versioning system provides detailed error types:

```rust
pub enum VersioningError {
    NotInitialized = 1,              // Version not set
    VersionMismatch = 2,              // Version doesn't match expected
    MigrationInProgress = 3,          // Migration currently executing
    MigrationFailed = 4,              // Migration failed
    UnauthorizedUpgrade = 5,          // Not authorized for upgrade
    InvalidVersionNumber = 6,         // Version validation failed
    MigrationHookFailed = 7,          // Custom hook failed
    SchemaValidationFailed = 8,       // Data schema invalid
    RollbackFailed = 9,               // Rollback failed
}
```

### Error Recovery Pattern

```rust
match VersionManager::migrate(&env, 1, 2, admin, |env| {
    // Custom migration
    data_migration(env)
}) {
    Ok(()) => {
        // Success - contract now at v2
    }
    Err(VersioningError::MigrationHookFailed) => {
        // Hook failed - migration state is RollbackRequired
        // Admin must investigate and call reset_migration_state
        // after manual fixes
    }
    Err(VersioningError::InvalidVersionNumber) => {
        // Tried to downgrade or same version
        // This is a logic error in the upgrade code
    }
    Err(e) => {
        // Other errors - investigate
    }
}
```

## Migration State Machine

The contract tracks migration progress through discrete states:

```
┌────────────────────────────────────────────────────────────────┐
│                    Migration State Diagram                      │
└────────────────────────────────────────────────────────────────┘

    ┌──────────────────────────────────────────────────────────┐
    │                    NONE (Initial)                        │
    │              No migration in progress                    │
    └────────────────┬─────────────────────────────────────────┘
                     │
                     │ Call migrate()
                     ↓
    ┌──────────────────────────────────────────────────────────┐
    │                 IN_PROGRESS                              │
    │         Migration hook is executing                      │
    │     Contract operations are blocked                      │
    └─────┬──────────────────────────────────┬─────────────────┘
          │                                  │
          │ Hook succeeds                    │ Hook fails
          ↓                                  ↓
    ┌──────────────────┐           ┌─────────────────────────┐
    │    COMPLETE      │           │  ROLLBACK_REQUIRED      │
    │ (Ready for next  │           │  (Manual intervention   │
    │   migration)     │           │   needed, then reset)   │
    └──────────────────┘           └─────────────────────────┘
          │                               │
          │                               │ reset_migration_state()
          │                               ↓
          │                          NONE (Recovery)
          │                               
          └─ Next upgrade() call ───→ IN_PROGRESS
```

## Backward Compatibility Guarantees

The versioning system ensures:

1. **Storage Continuity**: Storage keys don't change between versions
2. **Data Accessibility**: Old data is readable during migration
3. **Atomic Transitions**: Migration completes fully or not at all
4. **Audit Trail**: All transitions are logged immutably
5. **Authorization**: Only authorized addresses can upgrade

## Multi-Contract Coordination

For systems with multiple contracts (Treasury, Policy, Claims, etc.):

```rust
// All contracts maintain independent version histories
let treasury_version = VersionManager::current_version(&treasury_env)?;
let policy_version = VersionManager::current_version(&policy_env)?;
let claims_version = VersionManager::current_version(&claims_env)?;

// Upgrades can be coordinated through governance
// Each contract is responsible for its own migration logic
// but can call other contracts to verify compatible versions
```

### Example: Coordinated Upgrade

```rust
#[contractimpl]
impl Governance {
    pub fn execute_upgrade(env: Env, contracts: Vec<Address>) -> Result<(), ContractError> {
        admin.require_auth();
        
        // Upgrade Treasury
        treasury::upgrade(&env, 2)?;
        
        // Upgrade Policy
        policy::upgrade(&env, 2)?;
        
        // Upgrade Claims
        claims::upgrade(&env, 2)?;
        
        // Verify all at new version
        verify_all_upgraded(&env, 2)?;
        
        Ok(())
    }
}
```

## Best Practices

### 1. Plan Upgrades in Advance
- Document all storage changes between versions
- Test migrations thoroughly in staging
- Ensure migration hooks are idempotent

### 2. Make Migration Hooks Simple
- Move complex logic to separate helper functions
- Break large migrations into smaller steps
- Handle errors gracefully

### 3. Maintain Data Compatibility
- Keep old data structures readable during migration period
- Don't delete data without understanding implications
- Validate data before and after migration

### 4. Test Thoroughly
- Test migrations with realistic data volumes
- Test rollback scenarios
- Test version compatibility checks
- Test multi-contract interactions

### 5. Communicate Changes
- Document version changes in README or changelog
- Notify users of planned upgrades
- Provide rollback plans if needed
- Monitor post-upgrade metrics

### 6. Version Numbering Strategy
- Increment version for every upgrade
- Consider semantic versioning (MAJOR.MINOR.PATCH)
- Never downgrade versions
- Reserve version 0 for errors/invalid states

## Testing Upgrade Scenarios

### Test Case: Simple Upgrade

```rust
#[test]
fn test_simple_upgrade_v1_to_v2() {
    let env = Env::default();
    
    // Initialize at v1
    VersionManager::initialize(&env, 1)?;
    assert_eq!(VersionManager::current_version(&env)?, 1);
    
    // Upgrade to v2
    VersionManager::migrate(&env, 1, 2, admin, |env| Ok(()))?;
    assert_eq!(VersionManager::current_version(&env)?, 2);
    
    // Verify history
    let history = VersionManager::version_history(&env)?;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].from_version, 1);
    assert_eq!(history[0].to_version, 2);
}
```

### Test Case: Failed Migration Recovery

```rust
#[test]
fn test_failed_migration_recovery() {
    let env = Env::default();
    
    // Initialize
    VersionManager::initialize(&env, 1)?;
    
    // Attempt failed migration
    let result = VersionManager::migrate(&env, 1, 2, admin, |env| {
        Err(VersioningError::MigrationHookFailed)
    });
    
    assert!(result.is_err());
    
    // Version should not change
    assert_eq!(VersionManager::current_version(&env)?, 1);
    
    // Migration state should be RollbackRequired
    let info = VersionManager::version_info(&env)?;
    assert_eq!(info.migration_state, MigrationState::RollbackRequired as u32);
    
    // Reset migration state
    VersionManager::reset_migration_state(&env, &admin)?;
    
    // Contract should be usable again
    VersionManager::ensure_compatible(&env, 1)?;
}
```

## Troubleshooting

### Issue: "MigrationInProgress" Error

**Cause**: A previous migration didn't complete or failed.

**Solution**:
1. Check migration state: `VersionManager::version_info(&env)?`
2. Investigate what caused the incomplete migration
3. Manually fix the state: `VersionManager::reset_migration_state(&env, &admin)?`
4. Retry the migration

### Issue: "VersionMismatch" Error

**Cause**: Contract version doesn't match what code expects.

**Solution**:
1. Check current version: `VersionManager::current_version(&env)?`
2. Either upgrade the contract to expected version, or update code to match
3. Ensure all contract operations check the correct version

### Issue: Data Not Migrated Correctly

**Cause**: Migration hook didn't execute or had errors.

**Solution**:
1. Add logging to migration hook
2. Test migration in isolation
3. Verify hook reads old data and writes new data
4. Check storage keys are correct

## Future Enhancements

Potential improvements to the versioning system:

1. **Automatic Schema Migration**: Generate migrations from type definitions
2. **Version-Specific Contract Logic**: Branch code paths based on version
3. **Gradual Rollout**: Deploy to subset of users at new version first
4. **Snapshot & Restore**: Snapshot state before migration, restore on failure
5. **Cross-Contract Versioning**: Verify compatible versions across contracts

## References

- [VersionManager API Documentation](../contracts/shared/src/versioning.rs)
- [UpgradeableContract Wrapper](../contracts/shared/src/upgradeable.rs)
- [Upgrade Test Suite](../contracts/shared/src/upgrade_tests.rs)
- [Treasury Contract Example](../contracts/treasury/src/lib.rs)
- [Policy Contract Example](../contracts/policy/lib.rs)
- [Claims Contract Example](../contracts/claims/src/lib.rs)

## Summary

The upgradeable contract architecture provides:

✅ **Explicit Versioning**: On-chain version tracking
✅ **Safe Migrations**: Atomic operations with rollback capability
✅ **Flexibility**: Custom migration hooks for complex logic
✅ **Auditability**: Complete migration history
✅ **Backward Compatibility**: Old data remains accessible
✅ **Authorization**: Upgrade protection with admin checks
✅ **Multi-Contract Support**: Independent versioning per contract

This system enables the Stellar Insured ecosystem to evolve safely while maintaining user trust and protocol integrity.
