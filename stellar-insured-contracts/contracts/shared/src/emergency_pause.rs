//! Emergency pause functionality for Stellar Soroban contracts
//!
//! This module provides comprehensive emergency pause capabilities:
//! - Global contract pause/unpause
//! - Selective function pausing
//! - Emergency pause with immediate effect
//! - Pause reason tracking and audit trail
//! - Admin-only controls with proper authorization

use soroban_sdk::{contracttype, Address, Env, Symbol, Vec, Map};
use crate::errors::ContractError;

/// Emergency pause configuration and state
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyPauseConfig {
    /// Whether emergency pause is active
    pub is_emergency_paused: bool,
    /// Reason for emergency pause (for audit trail)
    pub pause_reason: Symbol,
    /// Timestamp when pause was activated
    pub pause_timestamp: u64,
    /// Admin who initiated the pause
    pub paused_by: Address,
    /// Selectively paused functions (empty = all functions paused)
    pub paused_functions: Vec<Symbol>,
    /// Maximum duration for emergency pause (0 = indefinite)
    pub max_duration_seconds: u64,
}

/// Emergency pause event data for logging
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyPauseEvent {
    /// Action performed (pause/unpause)
    pub action: Symbol,
    /// Who performed the action
    pub actor: Address,
    /// Reason for the action
    pub reason: Symbol,
    /// Timestamp of the action
    pub timestamp: u64,
    /// Affected functions (if selective pause)
    pub affected_functions: Vec<Symbol>,
}

/// Storage keys for emergency pause functionality
const EMERGENCY_PAUSE_CONFIG: Symbol = Symbol::short("EMG_PAUSED");
const EMERGENCY_PAUSE_HISTORY: Symbol = Symbol::short("EMG_HISTORY");
const SELECTIVE_PAUSE_MAP: Symbol = Symbol::short("SEL_PAUSE");

/// Emergency pause management utilities
pub struct EmergencyPause;

impl EmergencyPause {
    /// Initialize emergency pause configuration
    pub fn initialize(env: &Env, admin: &Address) -> Result<(), ContractError> {
        if env.storage().persistent().has(&EMERGENCY_PAUSE_CONFIG) {
            return Err(ContractError::AlreadyInitialized);
        }

        let config = EmergencyPauseConfig {
            is_emergency_paused: false,
            pause_reason: Symbol::new(env, "not_paused"),
            pause_timestamp: 0,
            paused_by: admin.clone(),
            paused_functions: Vec::new(env),
            max_duration_seconds: 0, // Indefinite by default
        };

        env.storage().persistent().set(&EMERGENCY_PAUSE_CONFIG, &config);
        Ok(())
    }

    /// Check if contract is currently emergency paused
    pub fn is_emergency_paused(env: &Env) -> bool {
        env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .map(|config: EmergencyPauseConfig| config.is_emergency_paused)
            .unwrap_or(false)
    }

    /// Check if a specific function is paused
    pub fn is_function_paused(env: &Env, function_name: &Symbol) -> bool {
        let config: EmergencyPauseConfig = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .unwrap_or_else(|| EmergencyPauseConfig {
                is_emergency_paused: false,
                pause_reason: Symbol::new(env, "not_paused"),
                pause_timestamp: 0,
                paused_by: Address::from_string(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"),
                paused_functions: Vec::new(env),
                max_duration_seconds: 0,
            });

        // If global emergency pause is active, all functions are paused
        if config.is_emergency_paused {
            return true;
        }

        // Check selective function pause
        for i in 0..config.paused_functions.len() {
            if let Some(paused_func) = config.paused_functions.get(i) {
                if &paused_func == function_name {
                    return true;
                }
            }
        }

        false
    }

    /// Activate emergency pause for the entire contract
    pub fn activate_emergency_pause(
        env: &Env,
        admin: &Address,
        reason: Symbol,
        max_duration_seconds: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();

        let mut config: EmergencyPauseConfig = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .ok_or(ContractError::NotInitialized)?;

        if config.is_emergency_paused {
            return Err(ContractError::InvalidState); // Already paused
        }

        config.is_emergency_paused = true;
        config.pause_reason = reason.clone();
        config.pause_timestamp = env.ledger().timestamp();
        config.paused_by = admin.clone();
        config.max_duration_seconds = max_duration_seconds;
        // Clear selective pauses when activating global emergency pause
        config.paused_functions = Vec::new(env);

        env.storage().persistent().set(&EMERGENCY_PAUSE_CONFIG, &config);

        // Record in history
        Self::record_pause_event(
            env,
            Symbol::new(env, "emergency_pause"),
            admin.clone(),
            reason,
            Vec::new(env),
        );

        // Emit event
        env.events().publish(
            (Symbol::new(env, "emergency_pause_activated"), ()),
            (admin.clone(), reason, env.ledger().timestamp(), max_duration_seconds),
        );

        Ok(())
    }

    /// Deactivate emergency pause
    pub fn deactivate_emergency_pause(
        env: &Env,
        admin: &Address,
        reason: Symbol,
    ) -> Result<(), ContractError> {
        admin.require_auth();

        let mut config: EmergencyPauseConfig = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .ok_or(ContractError::NotInitialized)?;

        if !config.is_emergency_paused {
            return Err(ContractError::InvalidState); // Not paused
        }

        config.is_emergency_paused = false;
        config.pause_reason = Symbol::new(env, "not_paused");
        config.pause_timestamp = 0;
        config.paused_by = admin.clone();
        config.max_duration_seconds = 0;

        env.storage().persistent().set(&EMERGENCY_PAUSE_CONFIG, &config);

        // Record in history
        Self::record_pause_event(
            env,
            Symbol::new(env, "emergency_unpause"),
            admin.clone(),
            reason,
            Vec::new(env),
        );

        // Emit event
        env.events().publish(
            (Symbol::new(env, "emergency_pause_deactivated"), ()),
            (admin.clone(), reason, env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Pause specific functions selectively
    pub fn pause_functions(
        env: &Env,
        admin: &Address,
        functions: &Vec<Symbol>,
        reason: Symbol,
    ) -> Result<(), ContractError> {
        admin.require_auth();

        let mut config: EmergencyPauseConfig = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .ok_or(ContractError::NotInitialized)?;

        // Add functions to paused list (avoiding duplicates)
        for i in 0..functions.len() {
            let func = functions.get(i).unwrap();
            let mut found = false;
            
            // Check if already in paused list
            for j in 0..config.paused_functions.len() {
                if let Some(existing) = config.paused_functions.get(j) {
                    if existing == func {
                        found = true;
                        break;
                    }
                }
            }
            
            if !found {
                config.paused_functions.push_back(func);
            }
        }

        env.storage().persistent().set(&EMERGENCY_PAUSE_CONFIG, &config);

        // Record in history
        Self::record_pause_event(
            env,
            Symbol::new(env, "selective_pause"),
            admin.clone(),
            reason,
            functions.clone(),
        );

        // Emit event
        env.events().publish(
            (Symbol::new(env, "functions_paused"), ()),
            (admin.clone(), functions.clone(), reason),
        );

        Ok(())
    }

    /// Unpause specific functions
    pub fn unpause_functions(
        env: &Env,
        admin: &Address,
        functions: &Vec<Symbol>,
        reason: Symbol,
    ) -> Result<(), ContractError> {
        admin.require_auth();

        let mut config: EmergencyPauseConfig = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .ok_or(ContractError::NotInitialized)?;

        // Remove functions from paused list
        let mut new_paused_functions = Vec::new(env);
        for i in 0..config.paused_functions.len() {
            if let Some(paused_func) = config.paused_functions.get(i) {
                let mut should_remove = false;
                for j in 0..functions.len() {
                    if let Some(target_func) = functions.get(j) {
                        if &paused_func == target_func {
                            should_remove = true;
                            break;
                        }
                    }
                }
                if !should_remove {
                    new_paused_functions.push_back(paused_func);
                }
            }
        }
        config.paused_functions = new_paused_functions;

        env.storage().persistent().set(&EMERGENCY_PAUSE_CONFIG, &config);

        // Record in history
        Self::record_pause_event(
            env,
            Symbol::new(env, "selective_unpause"),
            admin.clone(),
            reason,
            functions.clone(),
        );

        // Emit event
        env.events().publish(
            (Symbol::new(env, "functions_unpaused"), ()),
            (admin.clone(), functions.clone(), reason),
        );

        Ok(())
    }

    /// Get current emergency pause configuration
    pub fn get_pause_config(env: &Env) -> Result<EmergencyPauseConfig, ContractError> {
        env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .ok_or(ContractError::NotInitialized)
    }

    /// Get emergency pause history
    pub fn get_pause_history(env: &Env, limit: u32) -> Vec<EmergencyPauseEvent> {
        let mut history: Vec<EmergencyPauseEvent> = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_HISTORY)
            .unwrap_or_else(|| Vec::new(env));

        // Return last N events (most recent first)
        let total_events = history.len();
        if total_events <= limit as u32 {
            return history;
        }

        let mut result = Vec::new(env);
        let start_index = total_events - limit as u32;
        for i in start_index..total_events {
            if let Some(event) = history.get(i) {
                result.push_back(event);
            }
        }
        result
    }

    /// Check if emergency pause has expired
    pub fn check_pause_expiry(env: &Env) -> Result<bool, ContractError> {
        let config: EmergencyPauseConfig = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_CONFIG)
            .ok_or(ContractError::NotInitialized)?;

        if !config.is_emergency_paused || config.max_duration_seconds == 0 {
            return Ok(false); // Not paused or indefinite pause
        }

        let current_time = env.ledger().timestamp();
        let elapsed = current_time.saturating_sub(config.pause_timestamp);
        
        if elapsed >= config.max_duration_seconds {
            // Auto-unpause expired emergency pause
            let mut updated_config = config;
            updated_config.is_emergency_paused = false;
            updated_config.pause_reason = Symbol::new(env, "not_paused");
            updated_config.pause_timestamp = 0;
            updated_config.max_duration_seconds = 0;
            
            env.storage().persistent().set(&EMERGENCY_PAUSE_CONFIG, &updated_config);
            
            // Record auto-unpause event
            Self::record_pause_event(
                env,
                Symbol::new(env, "auto_unpause"),
                updated_config.paused_by,
                Symbol::new(env, "timeout"),
                Vec::new(env),
            );
            
            env.events().publish(
                (Symbol::new(env, "emergency_pause_expired"), ()),
                (config.pause_timestamp, config.max_duration_seconds),
            );
            
            return Ok(true);
        }

        Ok(false)
    }

    /// Record pause/unpause event in history
    fn record_pause_event(
        env: &Env,
        action: Symbol,
        actor: Address,
        reason: Symbol,
        affected_functions: Vec<Symbol>,
    ) {
        let mut history: Vec<EmergencyPauseEvent> = env.storage().persistent()
            .get(&EMERGENCY_PAUSE_HISTORY)
            .unwrap_or_else(|| Vec::new(env));

        let event = EmergencyPauseEvent {
            action,
            actor,
            reason,
            timestamp: env.ledger().timestamp(),
            affected_functions,
        };

        history.push_back(event);

        // Keep only last 100 events to prevent storage bloat
        if history.len() > 100 {
            let mut trimmed_history = Vec::new(env);
            let start = history.len() - 100;
            for i in start..history.len() {
                if let Some(event) = history.get(i) {
                    trimmed_history.push_back(event);
                }
            }
            history = trimmed_history;
        }

        env.storage().persistent().set(&EMERGENCY_PAUSE_HISTORY, &history);
    }

    /// Validate that operation is allowed (not paused)
    pub fn validate_not_paused(env: &Env, function_name: Option<&Symbol>) -> Result<(), ContractError> {
        // Check if emergency pause is active
        if Self::is_emergency_paused(env) {
            return Err(ContractError::Paused);
        }

        // Check selective function pause if function name provided
        if let Some(func_name) = function_name {
            if Self::is_function_paused(env, func_name) {
                return Err(ContractError::FunctionPaused);
            }
        }

        // Check for pause expiry
        if Self::check_pause_expiry(env)? {
            return Err(ContractError::Paused); // Pause expired and auto-unpaused
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, Address};

    #[test]
    fn test_emergency_pause_initialization() {
        let env = Env::default();
        let admin = Address::generate(&env);

        // Should succeed on first initialization
        let result = EmergencyPause::initialize(&env, &admin);
        assert!(result.is_ok());

        // Should fail on second initialization
        let result2 = EmergencyPause::initialize(&env, &admin);
        assert_eq!(result2, Err(ContractError::AlreadyInitialized));

        // Check initial state
        let config = EmergencyPause::get_pause_config(&env).unwrap();
        assert!(!config.is_emergency_paused);
        assert_eq!(config.pause_reason, Symbol::new(&env, "not_paused"));
    }

    #[test]
    fn test_emergency_pause_activation() {
        let env = Env::default();
        let admin = Address::generate(&env);
        
        EmergencyPause::initialize(&env, &admin).unwrap();

        // Activate emergency pause
        let reason = Symbol::new(&env, "security_vulnerability");
        let result = EmergencyPause::activate_emergency_pause(&env, &admin, reason.clone(), 3600);
        assert!(result.is_ok());

        // Check state
        assert!(EmergencyPause::is_emergency_paused(&env));
        let config = EmergencyPause::get_pause_config(&env).unwrap();
        assert!(config.is_emergency_paused);
        assert_eq!(config.pause_reason, reason);
        assert_eq!(config.paused_by, admin);
        assert_eq!(config.max_duration_seconds, 3600);
    }

    #[test]
    fn test_emergency_pause_deactivation() {
        let env = Env::default();
        let admin = Address::generate(&env);
        
        EmergencyPause::initialize(&env, &admin).unwrap();
        EmergencyPause::activate_emergency_pause(&env, &admin, Symbol::new(&env, "test"), 3600).unwrap();

        // Deactivate emergency pause
        let reason = Symbol::new(&env, "issue_resolved");
        let result = EmergencyPause::deactivate_emergency_pause(&env, &admin, reason.clone());
        assert!(result.is_ok());

        // Check state
        assert!(!EmergencyPause::is_emergency_paused(&env));
        let config = EmergencyPause::get_pause_config(&env).unwrap();
        assert!(!config.is_emergency_paused);
        assert_eq!(config.pause_reason, Symbol::new(&env, "not_paused"));
    }

    #[test]
    fn test_selective_function_pause() {
        let env = Env::default();
        let admin = Address::generate(&env);
        
        EmergencyPause::initialize(&env, &admin).unwrap();

        let functions = Vec::from_array(&env, [
            Symbol::new(&env, "issue_policy"),
            Symbol::new(&env, "renew_policy"),
        ]);

        // Pause specific functions
        let result = EmergencyPause::pause_functions(&env, &admin, &functions, Symbol::new(&env, "maintenance"));
        assert!(result.is_ok());

        // Check individual function states
        assert!(EmergencyPause::is_function_paused(&env, &Symbol::new(&env, "issue_policy")));
        assert!(EmergencyPause::is_function_paused(&env, &Symbol::new(&env, "renew_policy")));
        assert!(!EmergencyPause::is_function_paused(&env, &Symbol::new(&env, "get_policy")));

        // Unpause specific functions
        let result2 = EmergencyPause::unpause_functions(&env, &admin, &Vec::from_array(&env, [Symbol::new(&env, "issue_policy")]), Symbol::new(&env, "maintenance_complete"));
        assert!(result2.is_ok());

        // Check state after unpausing
        assert!(!EmergencyPause::is_function_paused(&env, &Symbol::new(&env, "issue_policy")));
        assert!(EmergencyPause::is_function_paused(&env, &Symbol::new(&env, "renew_policy")));
    }

    #[test]
    fn test_pause_validation() {
        let env = Env::default();
        let admin = Address::generate(&env);
        
        EmergencyPause::initialize(&env, &admin).unwrap();

        // Should allow operations when not paused
        assert!(EmergencyPause::validate_not_paused(&env, None).is_ok());
        assert!(EmergencyPause::validate_not_paused(&env, Some(&Symbol::new(&env, "test_function"))).is_ok());

        // Pause a specific function
        EmergencyPause::pause_functions(
            &env, 
            &admin, 
            &Vec::from_array(&env, [Symbol::new(&env, "restricted_function")]),
            Symbol::new(&env, "test")
        ).unwrap();

        // Should fail for paused function
        assert_eq!(
            EmergencyPause::validate_not_paused(&env, Some(&Symbol::new(&env, "restricted_function"))),
            Err(ContractError::FunctionPaused)
        );

        // Should still allow other functions
        assert!(EmergencyPause::validate_not_paused(&env, Some(&Symbol::new(&env, "allowed_function"))).is_ok());

        // Activate global emergency pause
        EmergencyPause::activate_emergency_pause(&env, &admin, Symbol::new(&env, "global"), 3600).unwrap();

        // Should fail for all functions during global pause
        assert_eq!(
            EmergencyPause::validate_not_paused(&env, Some(&Symbol::new(&env, "any_function"))),
            Err(ContractError::Paused)
        );
        assert_eq!(
            EmergencyPause::validate_not_paused(&env, None),
            Err(ContractError::Paused)
        );
    }

    #[test]
    fn test_pause_history() {
        let env = Env::default();
        let admin = Address::generate(&env);
        
        EmergencyPause::initialize(&env, &admin).unwrap();

        // Perform some pause operations
        EmergencyPause::activate_emergency_pause(&env, &admin, Symbol::new(&env, "reason1"), 3600).unwrap();
        EmergencyPause::deactivate_emergency_pause(&env, &admin, Symbol::new(&env, "resolved")).unwrap();
        
        let functions = Vec::from_array(&env, [Symbol::new(&env, "test_func")]);
        EmergencyPause::pause_functions(&env, &admin, &functions, Symbol::new(&env, "maintenance")).unwrap();

        // Check history
        let history = EmergencyPause::get_pause_history(&env, 10);
        assert_eq!(history.len(), 3);
        
        let first_event = history.get(0).unwrap();
        assert_eq!(first_event.action, Symbol::new(&env, "emergency_pause"));
        
        let last_event = history.get(2).unwrap();
        assert_eq!(last_event.action, Symbol::new(&env, "selective_pause"));
    }

    #[test]
    fn test_pause_expiry() {
        let env = Env::default();
        env.ledger().with_mut(|ledger| {
            ledger.timestamp = 1000;
        });
        
        let admin = Address::generate(&env);
        EmergencyPause::initialize(&env, &admin).unwrap();

        // Activate pause with 100 second duration
        EmergencyPause::activate_emergency_pause(&env, &admin, Symbol::new(&env, "test"), 100).unwrap();
        assert!(EmergencyPause::is_emergency_paused(&env));

        // Advance time by 50 seconds
        env.ledger().with_mut(|ledger| {
            ledger.timestamp = 1050;
        });

        // Should still be paused
        assert!(EmergencyPause::is_emergency_paused(&env));
        assert!(EmergencyPause::validate_not_paused(&env, None).is_err());

        // Advance time by another 60 seconds (total 110 seconds)
        env.ledger().with_mut(|ledger| {
            ledger.timestamp = 1110;
        });

        // Should be auto-unpaused due to expiry
        assert!(!EmergencyPause::is_emergency_paused(&env));
        assert!(EmergencyPause::validate_not_paused(&env, None).is_ok());
    }
}