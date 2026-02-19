//! Authorization Module for Stellar Insured Protocol
//!
//! This module provides a unified, role-based access control (RBAC) system
//! for all contracts in the Stellar Insured ecosystem.
//!
//! ## Features
//! - Standardized role definitions across all contracts
//! - Explicit permission checking for privileged operations
//! - Cross-contract call validation
//! - Least-privilege enforcement
//! - Audit trail support

#![no_std]

use soroban_sdk::{contracttype, Address, Env};

/// Protocol-wide role definitions
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Role {
    /// Root administrator with full protocol access
    Admin,
    /// Governance contract or approved governance participant
    Governance,
    /// Risk pool manager authorized to handle liquidity operations
    RiskPoolManager,
    /// Policy manager authorized to create and manage policies
    PolicyManager,
    /// Claim processor authorized to approve/reject claims
    ClaimProcessor,
    /// Auditor authorized to view sensitive data and perform audits
    Auditor,
    /// Regular user (policyholder, liquidity provider, etc.)
    User,
}

/// Storage keys for role assignments
#[contracttype]
#[derive(Clone)]
pub enum RoleKey {
    /// Maps Address -> Role
    UserRole(Address),
    /// Contract-level admin address
    ContractAdmin,
    /// Trusted contract addresses for cross-contract calls
    TrustedContract(Address),
}

/// Authorization errors
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AuthError {
    Unauthorized,
    InvalidRole,
    RoleNotFound,
    NotTrustedContract,
}

/// Permission matrix: defines what each role can do
impl Role {
    /// Check if this role has permission for administrative actions
    pub fn can_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }

    /// Check if this role can manage policies
    pub fn can_manage_policies(&self) -> bool {
        matches!(self, Role::Admin | Role::PolicyManager)
    }

    /// Check if this role can process claims
    pub fn can_process_claims(&self) -> bool {
        matches!(self, Role::Admin | Role::ClaimProcessor)
    }

    /// Check if this role can manage risk pool
    pub fn can_manage_risk_pool(&self) -> bool {
        matches!(self, Role::Admin | Role::RiskPoolManager)
    }

    /// Check if this role can participate in governance
    pub fn can_govern(&self) -> bool {
        matches!(self, Role::Admin | Role::Governance)
    }

    /// Check if this role can submit claims
    pub fn can_submit_claim(&self) -> bool {
        !matches!(self, Role::ClaimProcessor) // Claim processors cannot submit their own claims
    }

    /// Check if this role can audit system operations
    pub fn can_audit(&self) -> bool {
        matches!(self, Role::Admin | Role::Auditor)
    }

    /// Check if this role has read-only access (for auditors and users)
    pub fn can_read(&self) -> bool {
        matches!(self, Role::Admin | Role::Auditor | Role::User)
    }

    /// Check if this role has elevated permissions (admin or governance roles)
    pub fn has_elevated_permissions(&self) -> bool {
        matches!(self, Role::Admin | Role::Governance)
    }
}

/// Core authorization functions

/// Initialize contract admin (call once during contract initialization)
pub fn initialize_admin(env: &Env, admin: Address) {
    env.storage()
        .persistent()
        .set(&RoleKey::ContractAdmin, &admin);
    env.storage()
        .persistent()
        .set(&RoleKey::UserRole(admin.clone()), &Role::Admin);
}

/// Get the contract admin address
pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&RoleKey::ContractAdmin)
}

/// Grant a role to an address (admin only)
pub fn grant_role(env: &Env, caller: &Address, target: &Address, role: Role) -> Result<(), AuthError> {
    // Verify caller is admin
    require_role(env, caller, Role::Admin)?;
    
    // Grant the role
    env.storage()
        .persistent()
        .set(&RoleKey::UserRole(target.clone()), &role);
    
    // Emit event for role change logging
    env.events()
        .publish(("role_granted", target.clone(), role.clone()), caller.clone());
    
    Ok(())
}

/// Revoke a role from an address (admin only)
pub fn revoke_role(env: &Env, caller: &Address, target: &Address) -> Result<(), AuthError> {
    // Verify caller is admin
    require_role(env, caller, Role::Admin)?;
    
    // Prevent admin from revoking their own role (safeguard)
    if caller == target {
        return Err(AuthError::Unauthorized);
    }
    
    // Revoke by setting to User role (lowest privilege)
    env.storage()
        .persistent()
        .set(&RoleKey::UserRole(target.clone()), &Role::User);
    
    // Emit event for role change logging
    env.events()
        .publish(("role_revoked", target.clone()), caller.clone());
    
    Ok(())
}

/// Get the role of an address
pub fn get_role(env: &Env, address: &Address) -> Role {
    env.storage()
        .persistent()
        .get(&RoleKey::UserRole(address.clone()))
        .unwrap_or(Role::User) // Default to User if no role assigned
}

/// Check if an address has a specific role
pub fn has_role(env: &Env, address: &Address, required_role: Role) -> bool {
    let user_role = get_role(env, address);
    user_role == required_role
}

/// Require that the caller has a specific role (throws error if not)
pub fn require_role(env: &Env, address: &Address, required_role: Role) -> Result<(), AuthError> {
    let user_role = get_role(env, address);
    
    if user_role == required_role {
        Ok(())
    } else {
        Err(AuthError::Unauthorized)
    }
}

/// Require admin privileges
pub fn require_admin(env: &Env, address: &Address) -> Result<(), AuthError> {
    require_role(env, address, Role::Admin)
}

/// Check if an address has any of the specified roles
pub fn has_any_role(env: &Env, address: &Address, roles: &[Role]) -> bool {
    let user_role = get_role(env, address);
    roles.contains(&user_role)
}

/// Require that the caller has one of the specified roles
pub fn require_any_role(env: &Env, address: &Address, roles: &[Role]) -> Result<(), AuthError> {
    if has_any_role(env, address, roles) {
        Ok(())
    } else {
        Err(AuthError::Unauthorized)
    }
}

/// Role delegation functions

/// Delegate a role to another address (role-dependent permission)
pub fn delegate_role(env: &Env, caller: &Address, target: &Address, role: Role) -> Result<(), AuthError> {
    caller.require_auth();
    
    // Check if caller has permission to delegate this specific role
    match role {
        Role::Admin => require_admin(env, caller), // Only admin can delegate admin role
        Role::PolicyManager => {
            // Admin or PolicyManager can delegate PolicyManager role
            if !matches!(get_role(env, caller), Role::Admin | Role::PolicyManager) {
                return Err(AuthError::Unauthorized);
            }
            Ok(())
        },
        Role::ClaimProcessor => {
            // Admin or Governance can delegate ClaimProcessor role
            if !matches!(get_role(env, caller), Role::Admin | Role::Governance) {
                return Err(AuthError::Unauthorized);
            }
            Ok(())
        },
        Role::RiskPoolManager => {
            // Admin or Governance can delegate RiskPoolManager role
            if !matches!(get_role(env, caller), Role::Admin | Role::Governance) {
                return Err(AuthError::Unauthorized);
            }
            Ok(())
        },
        Role::Auditor => {
            // Admin or Governance can delegate Auditor role
            if !matches!(get_role(env, caller), Role::Admin | Role::Governance) {
                return Err(AuthError::Unauthorized);
            }
            Ok(())
        },
        Role::Governance => {
            // Only admin can delegate governance role
            require_admin(env, caller)
        },
        Role::User => {
            // Any user can delegate User role (though not very meaningful)
            Ok(())
        },
    }?;
    
    // Grant the role to the target
    env.storage()
        .persistent()
        .set(&RoleKey::UserRole(target.clone()), &role);
    
    // Emit event for role delegation logging
    env.events()
        .publish(("role_delegated", target.clone(), role.clone()), caller.clone());
    
    Ok(())
}

/// Check if an address can delegate a specific role
pub fn can_delegate_role(env: &Env, address: &Address, role: Role) -> bool {
    match role {
        Role::Admin => matches!(get_role(env, address), Role::Admin),
        Role::PolicyManager => matches!(get_role(env, address), Role::Admin | Role::PolicyManager),
        Role::ClaimProcessor => matches!(get_role(env, address), Role::Admin | Role::Governance),
        Role::RiskPoolManager => matches!(get_role(env, address), Role::Admin | Role::Governance),
        Role::Auditor => matches!(get_role(env, address), Role::Admin | Role::Governance),
        Role::Governance => matches!(get_role(env, address), Role::Admin),
        Role::User => true, // Anyone can delegate User role
    }
}

/// Revoke a delegated role (admin or the original delegator can revoke)
pub fn revoke_delegated_role(env: &Env, caller: &Address, target: &Address) -> Result<(), AuthError> {
    caller.require_auth();
    
    // Only admin can revoke a role in this implementation
    if !matches!(get_role(env, caller), Role::Admin) {
        return Err(AuthError::Unauthorized);
    }
    
    // Revert to User role (lowest privilege)
    env.storage()
        .persistent()
        .set(&RoleKey::UserRole(target.clone()), &Role::User);
    
    // Emit event for role revocation logging
    env.events()
        .publish(("role_delegation_revoked", target.clone()), caller.clone());
    
    Ok(())
}

/// Get all roles assigned to an address (for audit purposes)
pub fn get_all_roles(env: &Env) -> Vec<(Address, Role)> {
    // This is a simplified implementation since Soroban storage doesn't support iteration
    // In a real implementation, we'd need to maintain a separate mapping for this
    Vec::new()
}

/// Permission-based authorization (more granular than role-based)

/// Require permission to manage policies
pub fn require_policy_management(env: &Env, address: &Address) -> Result<(), AuthError> {
    let role = get_role(env, address);
    if role.can_manage_policies() {
        Ok(())
    } else {
        Err(AuthError::Unauthorized)
    }
}

/// Require permission to process claims
pub fn require_claim_processing(env: &Env, address: &Address) -> Result<(), AuthError> {
    let role = get_role(env, address);
    if role.can_process_claims() {
        Ok(())
    } else {
        Err(AuthError::Unauthorized)
    }
}

/// Require permission to manage risk pool
pub fn require_risk_pool_management(env: &Env, address: &Address) -> Result<(), AuthError> {
    let role = get_role(env, address);
    if role.can_manage_risk_pool() {
        Ok(())
    } else {
        Err(AuthError::Unauthorized)
    }
}

/// Require permission to participate in governance
pub fn require_governance_permission(env: &Env, address: &Address) -> Result<(), AuthError> {
    let role = get_role(env, address);
    if role.can_govern() {
        Ok(())
    } else {
        Err(AuthError::Unauthorized)
    }
}

/// Cross-contract call validation

/// Register a trusted contract address (admin only)
pub fn register_trusted_contract(env: &Env, caller: &Address, contract_address: &Address) -> Result<(), AuthError> {
    require_admin(env, caller)?;
    
    env.storage()
        .persistent()
        .set(&RoleKey::TrustedContract(contract_address.clone()), &true);
    
    Ok(())
}

/// Unregister a trusted contract address (admin only)
pub fn unregister_trusted_contract(env: &Env, caller: &Address, contract_address: &Address) -> Result<(), AuthError> {
    require_admin(env, caller)?;
    
    env.storage()
        .persistent()
        .remove(&RoleKey::TrustedContract(contract_address.clone()));
    
    Ok(())
}

/// Check if a contract address is trusted
pub fn is_trusted_contract(env: &Env, contract_address: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&RoleKey::TrustedContract(contract_address.clone()))
        .unwrap_or(false)
}

/// Require that the contract making the call is trusted
pub fn require_trusted_contract(env: &Env, contract_address: &Address) -> Result<(), AuthError> {
    if is_trusted_contract(env, contract_address) {
        Ok(())
    } else {
        Err(AuthError::NotTrustedContract)
    }
}

/// Utility: Combine identity verification with role check
/// This is the recommended pattern for most privileged operations
pub fn verify_and_require_role(env: &Env, caller: &Address, required_role: Role) -> Result<(), AuthError> {
    // First, verify the caller's identity (Soroban's built-in auth)
    caller.require_auth();
    
    // Then, check if they have the required role
    require_role(env, caller, required_role)
}

/// Utility: Verify identity and check permission
pub fn verify_and_check_permission<F>(env: &Env, caller: &Address, permission_check: F) -> Result<(), AuthError>
where
    F: Fn(&Role) -> bool,
{
    caller.require_auth();
    
    let role = get_role(env, caller);
    if permission_check(&role) {
        Ok(())
    } else {
        Err(AuthError::Unauthorized)
    }
}
