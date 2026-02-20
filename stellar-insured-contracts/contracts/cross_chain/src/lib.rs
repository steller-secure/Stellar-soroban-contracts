#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec,
};

use insurance_contracts::authorization::{get_role, initialize_admin, require_admin, Role};

// ===== Storage Keys =====
const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const CONFIG: Symbol = Symbol::short("CONFIG");
const BRIDGE: Symbol = Symbol::short("BRIDGE");
const BRIDGE_LIST: Symbol = Symbol::short("BR_LIST");
const BRIDGE_CNT: Symbol = Symbol::short("BR_CNT");
const CHAIN: Symbol = Symbol::short("CHAIN");
const CHAIN_LIST: Symbol = Symbol::short("CH_LIST");
const MESSAGE: Symbol = Symbol::short("MESSAGE");
const MSG_LIST: Symbol = Symbol::short("MSG_LIST");
const MSG_CNT: Symbol = Symbol::short("MSG_CNT");
const NONCE: Symbol = Symbol::short("NONCE");
const ASSET_MAP: Symbol = Symbol::short("ASSET_MAP");
const ASSET_LIST: Symbol = Symbol::short("AMAP_LIST");
const VALIDATOR: Symbol = Symbol::short("VALIDATR");
const VAL_LIST: Symbol = Symbol::short("VAL_LIST");
const CONFIRMER: Symbol = Symbol::short("CONFIRMER");
const CC_PROP: Symbol = Symbol::short("CC_PROP");
const CC_PROP_CNT: Symbol = Symbol::short("CCP_CNT");
const CC_PROP_LIST: Symbol = Symbol::short("CCP_LIST");
const CC_VOTER: Symbol = Symbol::short("CC_VOTER");
const STATS: Symbol = Symbol::short("STATS");

const MAX_PAGINATION_LIMIT: u32 = 50;

// ===== Error Enum =====
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    Unauthorized = 1,
    Paused = 2,
    InvalidInput = 3,
    NotFound = 5,
    AlreadyExists = 6,
    InvalidState = 7,
    NotInitialized = 8,
    AlreadyInitialized = 9,
    InvalidRole = 17,
    RoleNotFound = 18,
    NotTrustedContract = 19,
    // Cross-chain specific
    BridgeNotRegistered = 160,
    ChainNotSupported = 161,
    MessageAlreadyProcessed = 162,
    InsufficientConfirmations = 163,
    AssetNotMapped = 164,
    MessageExpired = 165,
    InvalidMessageFormat = 166,
    BridgePaused = 167,
    ValidatorAlreadyConfirmed = 168,
    CrossChainProposalNotFound = 169,
    InvalidChainId = 170,
    NonceMismatch = 171,
}

impl From<insurance_contracts::authorization::AuthError> for ContractError {
    fn from(err: insurance_contracts::authorization::AuthError) -> Self {
        match err {
            insurance_contracts::authorization::AuthError::Unauthorized => {
                ContractError::Unauthorized
            }
            insurance_contracts::authorization::AuthError::InvalidRole => {
                ContractError::InvalidRole
            }
            insurance_contracts::authorization::AuthError::RoleNotFound => {
                ContractError::RoleNotFound
            }
            insurance_contracts::authorization::AuthError::NotTrustedContract => {
                ContractError::NotTrustedContract
            }
        }
    }
}

// ===== Contract Types =====

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainConfig {
    pub admin: Address,
    pub governance_contract: Address,
    pub oracle_contract: Address,
    pub min_confirmations: u32,
    pub message_expiry_seconds: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeRegistration {
    pub bridge_id: u64,
    pub address: Address,
    pub name: Symbol,
    pub supported_chains: Vec<u32>,
    pub status: u32,
    pub registered_at: u64,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChainRecord {
    pub chain_id: u32,
    pub name: Symbol,
    pub bridge_id: u64,
    pub is_active: bool,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainMessage {
    pub message_id: u64,
    pub source_chain_id: u32,
    pub target_chain_id: u32,
    pub message_type: u32,
    pub payload_hash: BytesN<32>,
    pub sender: Address,
    pub nonce: u64,
    pub status: u32,
    pub confirmations: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMapping {
    pub stellar_asset: Address,
    pub remote_chain_id: u32,
    pub remote_asset_hash: BytesN<32>,
    pub decimals: u32,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatorRecord {
    pub validator: Address,
    pub bridge_id: u64,
    pub is_active: bool,
    pub total_confirmations: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainProposal {
    pub proposal_id: u64,
    pub proposer: Address,
    pub title: Symbol,
    pub proposal_type: u32,
    pub chain_id: u32,
    pub yes_votes: i128,
    pub no_votes: i128,
    pub total_voters: u32,
    pub status: u32,
    pub voting_ends_at: u64,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainStats {
    pub total_bridges: u32,
    pub total_chains: u32,
    pub total_messages: u64,
    pub total_confirmed: u64,
    pub total_executed: u64,
    pub total_validators: u32,
    pub total_asset_mappings: u32,
    pub total_proposals: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PaginatedMessagesResult {
    pub messages: Vec<CrossChainMessage>,
    pub total_count: u32,
}

// ===== Helper Functions =====

fn is_paused(env: &Env) -> bool {
    env.storage().persistent().get(&PAUSED).unwrap_or(false)
}

fn set_paused(env: &Env, paused: bool) {
    env.storage().persistent().set(&PAUSED, &paused);
}

fn get_next_bridge_id(env: &Env) -> u64 {
    let id: u64 = env.storage().persistent().get(&BRIDGE_CNT).unwrap_or(0) + 1;
    env.storage().persistent().set(&BRIDGE_CNT, &id);
    id
}

fn get_next_message_id(env: &Env) -> u64 {
    let id: u64 = env.storage().persistent().get(&MSG_CNT).unwrap_or(0) + 1;
    env.storage().persistent().set(&MSG_CNT, &id);
    id
}

fn get_next_proposal_id(env: &Env) -> u64 {
    let id: u64 = env.storage().persistent().get(&CC_PROP_CNT).unwrap_or(0) + 1;
    env.storage().persistent().set(&CC_PROP_CNT, &id);
    id
}

fn get_and_increment_nonce(env: &Env, chain_id: u32) -> u64 {
    let nonce: u64 = env.storage().persistent().get(&(NONCE, chain_id)).unwrap_or(0) + 1;
    env.storage().persistent().set(&(NONCE, chain_id), &nonce);
    nonce
}

fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if is_paused(env) {
        return Err(ContractError::Paused);
    }
    Ok(())
}

fn get_config(env: &Env) -> Result<CrossChainConfig, ContractError> {
    env.storage()
        .persistent()
        .get(&CONFIG)
        .ok_or(ContractError::NotInitialized)
}

fn update_stats_field(env: &Env, updater: impl FnOnce(&mut CrossChainStats)) {
    let mut stats: CrossChainStats = env.storage().persistent().get(&STATS).unwrap_or(
        CrossChainStats {
            total_bridges: 0,
            total_chains: 0,
            total_messages: 0,
            total_confirmed: 0,
            total_executed: 0,
            total_validators: 0,
            total_asset_mappings: 0,
            total_proposals: 0,
        },
    );
    updater(&mut stats);
    env.storage().persistent().set(&STATS, &stats);
}

// ===== Contract =====

#[contract]
pub struct CrossChainContract;

#[contractimpl]
impl CrossChainContract {
    // ===== Infrastructure =====

    pub fn initialize(
        env: Env,
        admin: Address,
        governance_contract: Address,
        oracle_contract: Address,
        min_confirmations: u32,
        message_expiry_seconds: u64,
    ) -> Result<(), ContractError> {
        if insurance_contracts::authorization::get_admin(&env).is_some() {
            return Err(ContractError::AlreadyInitialized);
        }
        if min_confirmations == 0 {
            return Err(ContractError::InvalidInput);
        }
        if message_expiry_seconds == 0 {
            return Err(ContractError::InvalidInput);
        }

        admin.require_auth();
        initialize_admin(&env, admin.clone());

        let config = CrossChainConfig {
            admin: admin.clone(),
            governance_contract,
            oracle_contract,
            min_confirmations,
            message_expiry_seconds,
        };
        env.storage().persistent().set(&CONFIG, &config);

        env.events()
            .publish((Symbol::new(&env, "initialized"), ()), admin);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;
        set_paused(&env, true);
        env.events()
            .publish((Symbol::new(&env, "paused"), ()), admin);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;
        set_paused(&env, false);
        env.events()
            .publish((Symbol::new(&env, "unpaused"), ()), admin);
        Ok(())
    }

    pub fn get_config(env: Env) -> Result<CrossChainConfig, ContractError> {
        get_config(&env)
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        insurance_contracts::authorization::get_admin(&env).ok_or(ContractError::NotInitialized)
    }

    pub fn is_contract_paused(env: Env) -> bool {
        is_paused(&env)
    }

    pub fn get_stats(env: Env) -> CrossChainStats {
        env.storage().persistent().get(&STATS).unwrap_or(CrossChainStats {
            total_bridges: 0,
            total_chains: 0,
            total_messages: 0,
            total_confirmed: 0,
            total_executed: 0,
            total_validators: 0,
            total_asset_mappings: 0,
            total_proposals: 0,
        })
    }

    pub fn grant_role(
        env: Env,
        admin: Address,
        account: Address,
        role_id: u32,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;
        let role = match role_id {
            0 => Role::Admin,
            1 => Role::Governance,
            _ => return Err(ContractError::InvalidInput),
        };
        insurance_contracts::authorization::grant_role(&env, &admin, &account, role)?;
        env.events().publish(
            (Symbol::new(&env, "role_granted"), account.clone()),
            (admin, role_id),
        );
        Ok(())
    }

    // ===== Bridge & Chain Management (Scope Item 4) =====

    pub fn register_bridge(
        env: Env,
        admin: Address,
        address: Address,
        name: Symbol,
        supported_chains: Vec<u32>,
    ) -> Result<u64, ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;
        require_not_paused(&env)?;
        get_config(&env)?;

        if supported_chains.is_empty() {
            return Err(ContractError::InvalidInput);
        }

        let bridge_id = get_next_bridge_id(&env);
        let now = env.ledger().timestamp();

        let bridge = BridgeRegistration {
            bridge_id,
            address,
            name: name.clone(),
            supported_chains,
            status: 0, // Active
            registered_at: now,
            updated_at: now,
        };

        env.storage()
            .persistent()
            .set(&(BRIDGE, bridge_id), &bridge);

        let mut list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&BRIDGE_LIST)
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back(bridge_id);
        env.storage().persistent().set(&BRIDGE_LIST, &list);

        update_stats_field(&env, |s| s.total_bridges += 1);

        env.events().publish(
            (Symbol::new(&env, "bridge_registered"), bridge_id),
            (name, admin),
        );
        Ok(bridge_id)
    }

    pub fn update_bridge_status(
        env: Env,
        admin: Address,
        bridge_id: u64,
        new_status: u32,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;

        if new_status > 3 {
            return Err(ContractError::InvalidInput);
        }

        let mut bridge: BridgeRegistration = env
            .storage()
            .persistent()
            .get(&(BRIDGE, bridge_id))
            .ok_or(ContractError::BridgeNotRegistered)?;

        bridge.status = new_status;
        bridge.updated_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&(BRIDGE, bridge_id), &bridge);

        env.events().publish(
            (Symbol::new(&env, "bridge_status_updated"), bridge_id),
            new_status,
        );
        Ok(())
    }

    pub fn get_bridge(env: Env, bridge_id: u64) -> Result<BridgeRegistration, ContractError> {
        env.storage()
            .persistent()
            .get(&(BRIDGE, bridge_id))
            .ok_or(ContractError::BridgeNotRegistered)
    }

    pub fn get_all_bridges(env: Env) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&BRIDGE_LIST)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn register_chain(
        env: Env,
        admin: Address,
        chain_id: u32,
        name: Symbol,
        bridge_id: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;
        require_not_paused(&env)?;

        if chain_id == 0 {
            return Err(ContractError::InvalidChainId);
        }

        // Verify bridge exists
        let _bridge: BridgeRegistration = env
            .storage()
            .persistent()
            .get(&(BRIDGE, bridge_id))
            .ok_or(ContractError::BridgeNotRegistered)?;

        if env.storage().persistent().has(&(CHAIN, chain_id)) {
            return Err(ContractError::AlreadyExists);
        }

        let chain = ChainRecord {
            chain_id,
            name: name.clone(),
            bridge_id,
            is_active: true,
            registered_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&(CHAIN, chain_id), &chain);

        let mut list: Vec<u32> = env
            .storage()
            .persistent()
            .get(&CHAIN_LIST)
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back(chain_id);
        env.storage().persistent().set(&CHAIN_LIST, &list);

        update_stats_field(&env, |s| s.total_chains += 1);

        env.events().publish(
            (Symbol::new(&env, "chain_registered"), chain_id),
            (name, bridge_id),
        );
        Ok(())
    }

    pub fn deactivate_chain(
        env: Env,
        admin: Address,
        chain_id: u32,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;

        let mut chain: ChainRecord = env
            .storage()
            .persistent()
            .get(&(CHAIN, chain_id))
            .ok_or(ContractError::ChainNotSupported)?;

        chain.is_active = false;
        env.storage()
            .persistent()
            .set(&(CHAIN, chain_id), &chain);

        env.events()
            .publish((Symbol::new(&env, "chain_deactivated"), chain_id), admin);
        Ok(())
    }

    pub fn get_chain(env: Env, chain_id: u32) -> Result<ChainRecord, ContractError> {
        env.storage()
            .persistent()
            .get(&(CHAIN, chain_id))
            .ok_or(ContractError::ChainNotSupported)
    }

    pub fn get_supported_chains(env: Env) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&CHAIN_LIST)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ===== Cross-Chain Messaging (Scope Item 1) =====

    pub fn send_message(
        env: Env,
        sender: Address,
        target_chain_id: u32,
        message_type: u32,
        payload_hash: BytesN<32>,
    ) -> Result<u64, ContractError> {
        sender.require_auth();
        require_not_paused(&env)?;
        get_config(&env)?;

        if message_type > 4 {
            return Err(ContractError::InvalidMessageFormat);
        }

        // Verify target chain exists and is active
        let chain: ChainRecord = env
            .storage()
            .persistent()
            .get(&(CHAIN, target_chain_id))
            .ok_or(ContractError::ChainNotSupported)?;
        if !chain.is_active {
            return Err(ContractError::ChainNotSupported);
        }

        // Verify the bridge for this chain is active
        let bridge: BridgeRegistration = env
            .storage()
            .persistent()
            .get(&(BRIDGE, chain.bridge_id))
            .ok_or(ContractError::BridgeNotRegistered)?;
        if bridge.status != 0 {
            return Err(ContractError::BridgePaused);
        }

        let message_id = get_next_message_id(&env);
        let nonce = get_and_increment_nonce(&env, target_chain_id);
        let now = env.ledger().timestamp();

        // source_chain_id = 0 means Stellar
        let msg = CrossChainMessage {
            message_id,
            source_chain_id: 0,
            target_chain_id,
            message_type,
            payload_hash,
            sender: sender.clone(),
            nonce,
            status: 0, // Pending
            confirmations: 0,
            created_at: now,
            updated_at: now,
        };

        env.storage()
            .persistent()
            .set(&(MESSAGE, message_id), &msg);

        let mut list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&MSG_LIST)
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back(message_id);
        env.storage().persistent().set(&MSG_LIST, &list);

        update_stats_field(&env, |s| s.total_messages += 1);

        env.events().publish(
            (Symbol::new(&env, "message_sent"), message_id),
            (sender, target_chain_id, nonce),
        );
        Ok(message_id)
    }

    pub fn receive_message(
        env: Env,
        relayer: Address,
        source_chain_id: u32,
        message_type: u32,
        payload_hash: BytesN<32>,
        nonce: u64,
    ) -> Result<u64, ContractError> {
        relayer.require_auth();
        require_not_paused(&env)?;
        get_config(&env)?;

        if message_type > 4 {
            return Err(ContractError::InvalidMessageFormat);
        }
        if source_chain_id == 0 {
            return Err(ContractError::InvalidChainId);
        }

        let chain: ChainRecord = env
            .storage()
            .persistent()
            .get(&(CHAIN, source_chain_id))
            .ok_or(ContractError::ChainNotSupported)?;
        if !chain.is_active {
            return Err(ContractError::ChainNotSupported);
        }

        let message_id = get_next_message_id(&env);
        let now = env.ledger().timestamp();

        let msg = CrossChainMessage {
            message_id,
            source_chain_id,
            target_chain_id: 0, // Stellar
            message_type,
            payload_hash,
            sender: relayer.clone(),
            nonce,
            status: 0, // Pending
            confirmations: 0,
            created_at: now,
            updated_at: now,
        };

        env.storage()
            .persistent()
            .set(&(MESSAGE, message_id), &msg);

        let mut list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&MSG_LIST)
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back(message_id);
        env.storage().persistent().set(&MSG_LIST, &list);

        update_stats_field(&env, |s| s.total_messages += 1);

        env.events().publish(
            (Symbol::new(&env, "message_received"), message_id),
            (relayer, source_chain_id, nonce),
        );
        Ok(message_id)
    }

    pub fn get_message(env: Env, message_id: u64) -> Result<CrossChainMessage, ContractError> {
        env.storage()
            .persistent()
            .get(&(MESSAGE, message_id))
            .ok_or(ContractError::NotFound)
    }

    pub fn get_messages_paginated(
        env: Env,
        start_index: u32,
        limit: u32,
    ) -> Result<PaginatedMessagesResult, ContractError> {
        let effective_limit = if limit > MAX_PAGINATION_LIMIT || limit == 0 {
            MAX_PAGINATION_LIMIT
        } else {
            limit
        };

        let msg_list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&MSG_LIST)
            .unwrap_or_else(|| Vec::new(&env));

        let total_count = msg_list.len();

        if start_index >= total_count {
            return Ok(PaginatedMessagesResult {
                messages: Vec::new(&env),
                total_count,
            });
        }

        let end_index = core::cmp::min(start_index + effective_limit, total_count);
        let mut messages: Vec<CrossChainMessage> = Vec::new(&env);

        for i in start_index..end_index {
            let msg_id = msg_list.get(i).unwrap();
            if let Some(msg) = env
                .storage()
                .persistent()
                .get::<_, CrossChainMessage>(&(MESSAGE, msg_id))
            {
                messages.push_back(msg);
            }
        }

        Ok(PaginatedMessagesResult {
            messages,
            total_count,
        })
    }

    // ===== Cross-Chain Validation (Scope Item 5) =====

    pub fn register_validator(
        env: Env,
        admin: Address,
        validator: Address,
        bridge_id: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;
        require_not_paused(&env)?;

        let _bridge: BridgeRegistration = env
            .storage()
            .persistent()
            .get(&(BRIDGE, bridge_id))
            .ok_or(ContractError::BridgeNotRegistered)?;

        let key = (VALIDATOR, bridge_id, validator.clone());
        if env.storage().persistent().has(&key) {
            return Err(ContractError::AlreadyExists);
        }

        let record = ValidatorRecord {
            validator: validator.clone(),
            bridge_id,
            is_active: true,
            total_confirmations: 0,
        };

        env.storage().persistent().set(&key, &record);

        let mut list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&(VAL_LIST, bridge_id))
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back(validator.clone());
        env.storage()
            .persistent()
            .set(&(VAL_LIST, bridge_id), &list);

        update_stats_field(&env, |s| s.total_validators += 1);

        env.events().publish(
            (Symbol::new(&env, "validator_registered"), bridge_id),
            validator,
        );
        Ok(())
    }

    pub fn deactivate_validator(
        env: Env,
        admin: Address,
        validator: Address,
        bridge_id: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;

        let key = (VALIDATOR, bridge_id, validator.clone());
        let mut record: ValidatorRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NotFound)?;

        record.is_active = false;
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            (Symbol::new(&env, "validator_deactivated"), bridge_id),
            validator,
        );
        Ok(())
    }

    pub fn confirm_message(
        env: Env,
        validator: Address,
        message_id: u64,
    ) -> Result<u32, ContractError> {
        validator.require_auth();
        require_not_paused(&env)?;
        let config = get_config(&env)?;

        let mut msg: CrossChainMessage = env
            .storage()
            .persistent()
            .get(&(MESSAGE, message_id))
            .ok_or(ContractError::NotFound)?;

        // Check message not already executed or expired
        if msg.status == 2 {
            return Err(ContractError::MessageAlreadyProcessed);
        }
        if msg.status == 4 {
            return Err(ContractError::MessageExpired);
        }

        // Check expiry
        let now = env.ledger().timestamp();
        if now > msg.created_at + config.message_expiry_seconds {
            msg.status = 4; // Expired
            msg.updated_at = now;
            env.storage()
                .persistent()
                .set(&(MESSAGE, message_id), &msg);
            return Err(ContractError::MessageExpired);
        }

        // Determine which chain to find the bridge for
        let chain_id = if msg.source_chain_id == 0 {
            msg.target_chain_id
        } else {
            msg.source_chain_id
        };
        let chain: ChainRecord = env
            .storage()
            .persistent()
            .get(&(CHAIN, chain_id))
            .ok_or(ContractError::ChainNotSupported)?;

        // Verify validator is registered for this bridge
        let val_key = (VALIDATOR, chain.bridge_id, validator.clone());
        let mut val_record: ValidatorRecord = env
            .storage()
            .persistent()
            .get(&val_key)
            .ok_or(ContractError::Unauthorized)?;

        if !val_record.is_active {
            return Err(ContractError::Unauthorized);
        }

        // Check not already confirmed by this validator
        let confirm_key = (CONFIRMER, message_id, validator.clone());
        if env.storage().persistent().has(&confirm_key) {
            return Err(ContractError::ValidatorAlreadyConfirmed);
        }

        // Record confirmation
        env.storage().persistent().set(&confirm_key, &true);
        msg.confirmations += 1;
        msg.updated_at = now;

        if msg.confirmations >= config.min_confirmations {
            msg.status = 1; // Confirmed
            update_stats_field(&env, |s| s.total_confirmed += 1);
        }

        env.storage()
            .persistent()
            .set(&(MESSAGE, message_id), &msg);

        val_record.total_confirmations += 1;
        env.storage().persistent().set(&val_key, &val_record);

        env.events().publish(
            (Symbol::new(&env, "message_confirmed"), message_id),
            (validator, msg.confirmations),
        );
        Ok(msg.confirmations)
    }

    pub fn execute_message(
        env: Env,
        executor: Address,
        message_id: u64,
    ) -> Result<(), ContractError> {
        executor.require_auth();
        require_not_paused(&env)?;
        let config = get_config(&env)?;

        let mut msg: CrossChainMessage = env
            .storage()
            .persistent()
            .get(&(MESSAGE, message_id))
            .ok_or(ContractError::NotFound)?;

        if msg.status == 2 {
            return Err(ContractError::MessageAlreadyProcessed);
        }

        // Check expiry
        let now = env.ledger().timestamp();
        if now > msg.created_at + config.message_expiry_seconds {
            msg.status = 4;
            msg.updated_at = now;
            env.storage()
                .persistent()
                .set(&(MESSAGE, message_id), &msg);
            return Err(ContractError::MessageExpired);
        }

        if msg.confirmations < config.min_confirmations {
            return Err(ContractError::InsufficientConfirmations);
        }

        msg.status = 2; // Executed
        msg.updated_at = now;
        env.storage()
            .persistent()
            .set(&(MESSAGE, message_id), &msg);

        update_stats_field(&env, |s| s.total_executed += 1);

        env.events().publish(
            (Symbol::new(&env, "message_executed"), message_id),
            executor,
        );
        Ok(())
    }

    // ===== Asset Mapping (Scope Item 2) =====

    pub fn register_asset_mapping(
        env: Env,
        admin: Address,
        stellar_asset: Address,
        remote_chain_id: u32,
        remote_asset_hash: BytesN<32>,
        decimals: u32,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;
        require_not_paused(&env)?;

        let _chain: ChainRecord = env
            .storage()
            .persistent()
            .get(&(CHAIN, remote_chain_id))
            .ok_or(ContractError::ChainNotSupported)?;

        let key = (ASSET_MAP, stellar_asset.clone(), remote_chain_id);
        if env.storage().persistent().has(&key) {
            return Err(ContractError::AlreadyExists);
        }

        let mapping = AssetMapping {
            stellar_asset: stellar_asset.clone(),
            remote_chain_id,
            remote_asset_hash,
            decimals,
            is_active: true,
        };

        env.storage().persistent().set(&key, &mapping);

        let mut list: Vec<(Address, u32)> = env
            .storage()
            .persistent()
            .get(&ASSET_LIST)
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back((stellar_asset.clone(), remote_chain_id));
        env.storage().persistent().set(&ASSET_LIST, &list);

        update_stats_field(&env, |s| s.total_asset_mappings += 1);

        env.events().publish(
            (Symbol::new(&env, "asset_mapped"), remote_chain_id),
            stellar_asset,
        );
        Ok(())
    }

    pub fn deactivate_asset_mapping(
        env: Env,
        admin: Address,
        stellar_asset: Address,
        remote_chain_id: u32,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        require_admin(&env, &admin)?;

        let key = (ASSET_MAP, stellar_asset.clone(), remote_chain_id);
        let mut mapping: AssetMapping = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::AssetNotMapped)?;

        mapping.is_active = false;
        env.storage().persistent().set(&key, &mapping);

        env.events().publish(
            (Symbol::new(&env, "asset_deactivated"), remote_chain_id),
            stellar_asset,
        );
        Ok(())
    }

    pub fn get_asset_mapping(
        env: Env,
        stellar_asset: Address,
        remote_chain_id: u32,
    ) -> Result<AssetMapping, ContractError> {
        env.storage()
            .persistent()
            .get(&(ASSET_MAP, stellar_asset, remote_chain_id))
            .ok_or(ContractError::AssetNotMapped)
    }

    pub fn verify_asset_mapping(
        env: Env,
        stellar_asset: Address,
        remote_chain_id: u32,
        remote_asset_hash: BytesN<32>,
    ) -> Result<bool, ContractError> {
        let mapping: AssetMapping = env
            .storage()
            .persistent()
            .get(&(ASSET_MAP, stellar_asset, remote_chain_id))
            .ok_or(ContractError::AssetNotMapped)?;

        Ok(mapping.is_active && mapping.remote_asset_hash == remote_asset_hash)
    }

    // ===== Cross-Chain Governance (Scope Item 3) =====

    pub fn create_cross_chain_proposal(
        env: Env,
        proposer: Address,
        title: Symbol,
        proposal_type: u32,
        chain_id: u32,
    ) -> Result<u64, ContractError> {
        proposer.require_auth();
        require_not_paused(&env)?;
        get_config(&env)?;

        if proposal_type > 4 {
            return Err(ContractError::InvalidInput);
        }

        // chain_id of 0 is valid (applies to all chains / Stellar)
        let now = env.ledger().timestamp();
        // 14-day voting period
        let voting_ends_at = now + (14 * 86400u64);

        let proposal_id = get_next_proposal_id(&env);

        let proposal = CrossChainProposal {
            proposal_id,
            proposer: proposer.clone(),
            title: title.clone(),
            proposal_type,
            chain_id,
            yes_votes: 0,
            no_votes: 0,
            total_voters: 0,
            status: 0, // Active
            voting_ends_at,
            created_at: now,
        };

        env.storage()
            .persistent()
            .set(&(CC_PROP, proposal_id), &proposal);

        let mut list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&CC_PROP_LIST)
            .unwrap_or_else(|| Vec::new(&env));
        list.push_back(proposal_id);
        env.storage().persistent().set(&CC_PROP_LIST, &list);

        update_stats_field(&env, |s| s.total_proposals += 1);

        env.events().publish(
            (Symbol::new(&env, "cc_proposal_created"), proposal_id),
            (proposer, title, chain_id),
        );
        Ok(proposal_id)
    }

    pub fn vote_cross_chain_proposal(
        env: Env,
        voter: Address,
        proposal_id: u64,
        vote_weight: i128,
        is_yes: bool,
    ) -> Result<(), ContractError> {
        voter.require_auth();
        require_not_paused(&env)?;

        if vote_weight <= 0 {
            return Err(ContractError::InvalidInput);
        }

        let mut proposal: CrossChainProposal = env
            .storage()
            .persistent()
            .get(&(CC_PROP, proposal_id))
            .ok_or(ContractError::CrossChainProposalNotFound)?;

        if proposal.status != 0 {
            return Err(ContractError::InvalidState);
        }

        let now = env.ledger().timestamp();
        if now >= proposal.voting_ends_at {
            return Err(ContractError::InvalidState);
        }

        // Check not already voted
        let vote_key = (CC_VOTER, proposal_id, voter.clone());
        if env.storage().persistent().has(&vote_key) {
            return Err(ContractError::AlreadyExists);
        }

        env.storage().persistent().set(&vote_key, &is_yes);

        if is_yes {
            proposal.yes_votes += vote_weight;
        } else {
            proposal.no_votes += vote_weight;
        }
        proposal.total_voters += 1;

        env.storage()
            .persistent()
            .set(&(CC_PROP, proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "cc_proposal_voted"), proposal_id),
            (voter, vote_weight, is_yes),
        );
        Ok(())
    }

    pub fn finalize_cross_chain_proposal(
        env: Env,
        proposal_id: u64,
    ) -> Result<u32, ContractError> {
        let mut proposal: CrossChainProposal = env
            .storage()
            .persistent()
            .get(&(CC_PROP, proposal_id))
            .ok_or(ContractError::CrossChainProposalNotFound)?;

        if proposal.status != 0 {
            return Err(ContractError::InvalidState);
        }

        let now = env.ledger().timestamp();
        if now < proposal.voting_ends_at {
            return Err(ContractError::InvalidState);
        }

        let total_votes = proposal.yes_votes + proposal.no_votes;
        // Simple majority with quorum check
        let new_status = if total_votes == 0 {
            4 // Expired
        } else {
            let yes_pct = (proposal.yes_votes * 100) / total_votes;
            if yes_pct > 50 {
                1 // Passed
            } else {
                2 // Rejected
            }
        };

        proposal.status = new_status;
        env.storage()
            .persistent()
            .set(&(CC_PROP, proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "cc_proposal_finalized"), proposal_id),
            new_status,
        );
        Ok(new_status)
    }

    pub fn execute_cross_chain_proposal(
        env: Env,
        executor: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        executor.require_auth();
        require_not_paused(&env)?;

        let mut proposal: CrossChainProposal = env
            .storage()
            .persistent()
            .get(&(CC_PROP, proposal_id))
            .ok_or(ContractError::CrossChainProposalNotFound)?;

        if proposal.status != 1 {
            return Err(ContractError::InvalidState);
        }

        proposal.status = 3; // Executed
        env.storage()
            .persistent()
            .set(&(CC_PROP, proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "cc_proposal_executed"), proposal_id),
            executor,
        );
        Ok(())
    }

    pub fn get_cross_chain_proposal(
        env: Env,
        proposal_id: u64,
    ) -> Result<CrossChainProposal, ContractError> {
        env.storage()
            .persistent()
            .get(&(CC_PROP, proposal_id))
            .ok_or(ContractError::CrossChainProposalNotFound)
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env};

    fn setup() -> (Env, CrossChainContractClient<'static>, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CrossChainContract, ());
        let client = CrossChainContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let governance = Address::generate(&env);
        let oracle = Address::generate(&env);
        (env, client, admin, governance, oracle)
    }

    fn init(client: &CrossChainContractClient, admin: &Address, gov: &Address, oracle: &Address) {
        client.initialize(admin, gov, oracle, &3, &172800);
    }

    fn register_test_bridge(env: &Env, client: &CrossChainContractClient, admin: &Address) -> u64 {
        let bridge_addr = Address::generate(env);
        let mut chains = Vec::new(env);
        chains.push_back(1u32);
        client.register_bridge(admin, &bridge_addr, &Symbol::new(env, "TestBridge"), &chains)
    }

    fn register_test_chain(client: &CrossChainContractClient, env: &Env, admin: &Address, chain_id: u32, bridge_id: u64) {
        client.register_chain(admin, &chain_id, &Symbol::new(env, "ETH"), &bridge_id);
    }

    // ===== Initialization Tests =====

    #[test]
    fn test_initialize_success() {
        let (env, client, admin, gov, oracle) = setup();
        client.initialize(&admin, &gov, &oracle, &3, &172800);
        let config = client.get_config();
        assert_eq!(config.min_confirmations, 3);
        assert_eq!(config.message_expiry_seconds, 172800);
    }

    #[test]
    fn test_initialize_already_initialized() {
        let (_env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.try_initialize(&admin, &gov, &oracle, &3, &172800);
        assert!(result.is_err());
    }

    #[test]
    fn test_initialize_zero_confirmations() {
        let (_env, client, admin, gov, oracle) = setup();
        let result = client.try_initialize(&admin, &gov, &oracle, &0, &172800);
        assert!(result.is_err());
    }

    #[test]
    fn test_initialize_zero_expiry() {
        let (_env, client, admin, gov, oracle) = setup();
        let result = client.try_initialize(&admin, &gov, &oracle, &3, &0);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_admin() {
        let (_env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.get_admin();
        assert_eq!(result, admin);
    }

    // ===== Pause/Unpause Tests =====

    #[test]
    fn test_pause_unpause() {
        let (_env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        assert!(!client.is_contract_paused());
        client.pause(&admin);
        assert!(client.is_contract_paused());
        client.unpause(&admin);
        assert!(!client.is_contract_paused());
    }

    #[test]
    fn test_pause_unauthorized() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let other = Address::generate(&env);
        let result = client.try_pause(&other);
        assert!(result.is_err());
    }

    // ===== Bridge Management Tests =====

    #[test]
    fn test_register_bridge_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        assert_eq!(bridge_id, 1);
        let bridge = client.get_bridge(&bridge_id);
        assert_eq!(bridge.status, 0);
    }

    #[test]
    fn test_register_bridge_empty_chains() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.try_register_bridge(
            &admin, &Address::generate(&env),
            &Symbol::new(&env, "Bad"), &Vec::new(&env),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_update_bridge_status() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        client.update_bridge_status(&admin, &bridge_id, &1);
        let bridge = client.get_bridge(&bridge_id);
        assert_eq!(bridge.status, 1);
    }

    #[test]
    fn test_update_bridge_status_invalid() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        let result = client.try_update_bridge_status(&admin, &bridge_id, &5);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_bridge_not_found() {
        let (_env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.try_get_bridge(&999);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_all_bridges() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        register_test_bridge(&env, &client, &admin);
        register_test_bridge(&env, &client, &admin);
        let bridges = client.get_all_bridges();
        assert_eq!(bridges.len(), 2);
    }

    // ===== Chain Management Tests =====

    #[test]
    fn test_register_chain_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);
        let chain = client.get_chain(&1);
        assert!(chain.is_active);
        assert_eq!(chain.bridge_id, bridge_id);
    }

    #[test]
    fn test_register_chain_invalid_id() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        let result = client.try_register_chain(&admin, &0, &Symbol::new(&env, "Bad"), &bridge_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_chain_duplicate() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);
        let result = client.try_register_chain(&admin, &1, &Symbol::new(&env, "ETH2"), &bridge_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_chain_bridge_not_found() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.try_register_chain(&admin, &1, &Symbol::new(&env, "ETH"), &999);
        assert!(result.is_err());
    }

    #[test]
    fn test_deactivate_chain() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);
        client.deactivate_chain(&admin, &1);
        let chain = client.get_chain(&1);
        assert!(!chain.is_active);
    }

    #[test]
    fn test_get_supported_chains() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);
        client.register_chain(&admin, &2, &Symbol::new(&env, "BSC"), &bridge_id);
        let chains = client.get_supported_chains();
        assert_eq!(chains.len(), 2);
    }

    // ===== Messaging Tests =====

    #[test]
    fn test_send_message_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);
        assert_eq!(msg_id, 1);

        let msg = client.get_message(&msg_id);
        assert_eq!(msg.source_chain_id, 0);
        assert_eq!(msg.target_chain_id, 1);
        assert_eq!(msg.status, 0);
        assert_eq!(msg.nonce, 1);
    }

    #[test]
    fn test_send_message_inactive_chain() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);
        client.deactivate_chain(&admin, &1);
        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let result = client.try_send_message(&sender, &1, &0, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_paused_bridge() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);
        client.update_bridge_status(&admin, &bridge_id, &1);
        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let result = client.try_send_message(&sender, &1, &0, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_invalid_type() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);
        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let result = client.try_send_message(&sender, &1, &99, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_chain_not_found() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let result = client.try_send_message(&sender, &99, &0, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_when_paused() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        client.pause(&admin);
        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let result = client.try_send_message(&sender, &1, &0, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_receive_message_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let relayer = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[2u8; 32]);
        let msg_id = client.receive_message(&relayer, &1, &0, &hash, &1);

        let msg = client.get_message(&msg_id);
        assert_eq!(msg.source_chain_id, 1);
        assert_eq!(msg.target_chain_id, 0);
    }

    #[test]
    fn test_receive_message_invalid_source() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let relayer = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[2u8; 32]);
        let result = client.try_receive_message(&relayer, &0, &0, &hash, &1);
        assert!(result.is_err());
    }

    #[test]
    fn test_nonce_increments() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        let id1 = client.send_message(&sender, &1, &0, &hash);
        let id2 = client.send_message(&sender, &1, &0, &hash);

        let msg1 = client.get_message(&id1);
        let msg2 = client.get_message(&id2);
        assert_eq!(msg1.nonce, 1);
        assert_eq!(msg2.nonce, 2);
    }

    #[test]
    fn test_get_messages_paginated() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        for _ in 0..5 {
            client.send_message(&sender, &1, &0, &hash);
        }

        let result = client.get_messages_paginated(&0, &3);
        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.total_count, 5);

        let result2 = client.get_messages_paginated(&3, &10);
        assert_eq!(result2.messages.len(), 2);
    }

    #[test]
    fn test_get_messages_paginated_empty() {
        let (_env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.get_messages_paginated(&0, &10);
        assert_eq!(result.messages.len(), 0);
        assert_eq!(result.total_count, 0);
    }

    #[test]
    fn test_get_messages_paginated_out_of_bounds() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.send_message(&sender, &1, &0, &hash);

        let result = client.get_messages_paginated(&100, &10);
        assert_eq!(result.messages.len(), 0);
        assert_eq!(result.total_count, 1);
    }

    // ===== Validator Tests =====

    #[test]
    fn test_register_validator_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        let validator = Address::generate(&env);
        client.register_validator(&admin, &validator, &bridge_id);
    }

    #[test]
    fn test_register_validator_duplicate() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        let validator = Address::generate(&env);
        client.register_validator(&admin, &validator, &bridge_id);
        let result = client.try_register_validator(&admin, &validator, &bridge_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_validator_bridge_not_found() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let validator = Address::generate(&env);
        let result = client.try_register_validator(&admin, &validator, &999);
        assert!(result.is_err());
    }

    #[test]
    fn test_deactivate_validator() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        let validator = Address::generate(&env);
        client.register_validator(&admin, &validator, &bridge_id);
        client.deactivate_validator(&admin, &validator, &bridge_id);
    }

    // ===== Confirmation & Execution Tests =====

    #[test]
    fn test_confirm_message_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        let count = client.confirm_message(&v1, &msg_id);
        assert_eq!(count, 1);

        let msg = client.get_message(&msg_id);
        assert_eq!(msg.confirmations, 1);
        assert_eq!(msg.status, 0); // Still pending (need 3)
    }

    #[test]
    fn test_confirm_message_reaches_threshold() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        let v3 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);
        client.register_validator(&admin, &v2, &bridge_id);
        client.register_validator(&admin, &v3, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        client.confirm_message(&v1, &msg_id);
        client.confirm_message(&v2, &msg_id);
        client.confirm_message(&v3, &msg_id);

        let msg = client.get_message(&msg_id);
        assert_eq!(msg.confirmations, 3);
        assert_eq!(msg.status, 1); // Confirmed
    }

    #[test]
    fn test_confirm_message_duplicate_validator() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        client.confirm_message(&v1, &msg_id);
        let result = client.try_confirm_message(&v1, &msg_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_confirm_message_unregistered_validator() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        let unknown = Address::generate(&env);
        let result = client.try_confirm_message(&unknown, &msg_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_confirm_message_expired() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + 172801,
            protocol_version: 25,
            sequence_number: env.ledger().sequence(),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });

        let result = client.try_confirm_message(&v1, &msg_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_message_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        let v3 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);
        client.register_validator(&admin, &v2, &bridge_id);
        client.register_validator(&admin, &v3, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        client.confirm_message(&v1, &msg_id);
        client.confirm_message(&v2, &msg_id);
        client.confirm_message(&v3, &msg_id);

        let executor = Address::generate(&env);
        client.execute_message(&executor, &msg_id);

        let msg = client.get_message(&msg_id);
        assert_eq!(msg.status, 2); // Executed
    }

    #[test]
    fn test_execute_message_insufficient_confirmations() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        client.confirm_message(&v1, &msg_id);

        let executor = Address::generate(&env);
        let result = client.try_execute_message(&executor, &msg_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_message_already_executed() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        let v3 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);
        client.register_validator(&admin, &v2, &bridge_id);
        client.register_validator(&admin, &v3, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        client.confirm_message(&v1, &msg_id);
        client.confirm_message(&v2, &msg_id);
        client.confirm_message(&v3, &msg_id);

        let executor = Address::generate(&env);
        client.execute_message(&executor, &msg_id);
        let result = client.try_execute_message(&executor, &msg_id);
        assert!(result.is_err());
    }

    // ===== Asset Mapping Tests =====

    #[test]
    fn test_register_asset_mapping_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let asset = Address::generate(&env);
        let remote_hash = BytesN::from_array(&env, &[3u8; 32]);
        client.register_asset_mapping(&admin, &asset, &1, &remote_hash, &18);

        let mapping = client.get_asset_mapping(&asset, &1);
        assert!(mapping.is_active);
        assert_eq!(mapping.decimals, 18);
    }

    #[test]
    fn test_register_asset_mapping_chain_not_found() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let asset = Address::generate(&env);
        let remote_hash = BytesN::from_array(&env, &[3u8; 32]);
        let result = client.try_register_asset_mapping(&admin, &asset, &99, &remote_hash, &18);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_asset_mapping_duplicate() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let asset = Address::generate(&env);
        let remote_hash = BytesN::from_array(&env, &[3u8; 32]);
        client.register_asset_mapping(&admin, &asset, &1, &remote_hash, &18);
        let result = client.try_register_asset_mapping(&admin, &asset, &1, &remote_hash, &18);
        assert!(result.is_err());
    }

    #[test]
    fn test_deactivate_asset_mapping() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let asset = Address::generate(&env);
        let remote_hash = BytesN::from_array(&env, &[3u8; 32]);
        client.register_asset_mapping(&admin, &asset, &1, &remote_hash, &18);
        client.deactivate_asset_mapping(&admin, &asset, &1);

        let mapping = client.get_asset_mapping(&asset, &1);
        assert!(!mapping.is_active);
    }

    #[test]
    fn test_verify_asset_mapping_valid() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let asset = Address::generate(&env);
        let remote_hash = BytesN::from_array(&env, &[3u8; 32]);
        client.register_asset_mapping(&admin, &asset, &1, &remote_hash, &18);

        let valid = client.verify_asset_mapping(&asset, &1, &remote_hash);
        assert!(valid);
    }

    #[test]
    fn test_verify_asset_mapping_wrong_hash() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let asset = Address::generate(&env);
        let remote_hash = BytesN::from_array(&env, &[3u8; 32]);
        client.register_asset_mapping(&admin, &asset, &1, &remote_hash, &18);

        let wrong_hash = BytesN::from_array(&env, &[9u8; 32]);
        let valid = client.verify_asset_mapping(&asset, &1, &wrong_hash);
        assert!(!valid);
    }

    #[test]
    fn test_verify_asset_mapping_inactive() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let asset = Address::generate(&env);
        let remote_hash = BytesN::from_array(&env, &[3u8; 32]);
        client.register_asset_mapping(&admin, &asset, &1, &remote_hash, &18);
        client.deactivate_asset_mapping(&admin, &asset, &1);

        let valid = client.verify_asset_mapping(&asset, &1, &remote_hash);
        assert!(!valid);
    }

    // ===== Cross-Chain Governance Tests =====

    #[test]
    fn test_create_proposal_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "AddChain"), &0, &1,
        );
        assert_eq!(id, 1);

        let proposal = client.get_cross_chain_proposal(&id);
        assert_eq!(proposal.status, 0);
        assert_eq!(proposal.proposer, proposer);
    }

    #[test]
    fn test_create_proposal_invalid_type() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let proposer = Address::generate(&env);
        let result = client.try_create_cross_chain_proposal(&proposer, &Symbol::new(&env, "Bad"), &99, &1);
        assert!(result.is_err());
    }

    #[test]
    fn test_vote_proposal_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "AddChain"), &0, &1,
        );

        let voter = Address::generate(&env);
        client.vote_cross_chain_proposal(&voter, &id, &1000, &true);

        let proposal = client.get_cross_chain_proposal(&id);
        assert_eq!(proposal.yes_votes, 1000);
        assert_eq!(proposal.total_voters, 1);
    }

    #[test]
    fn test_vote_proposal_duplicate() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "AddChain"), &0, &1,
        );

        let voter = Address::generate(&env);
        client.vote_cross_chain_proposal(&voter, &id, &1000, &true);
        let result = client.try_vote_cross_chain_proposal(&voter, &id, &500, &false);
        assert!(result.is_err());
    }

    #[test]
    fn test_vote_proposal_invalid_weight() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "AddChain"), &0, &1,
        );

        let voter = Address::generate(&env);
        let result = client.try_vote_cross_chain_proposal(&voter, &id, &0, &true);
        assert!(result.is_err());
    }

    #[test]
    fn test_vote_proposal_not_found() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let voter = Address::generate(&env);
        let result = client.try_vote_cross_chain_proposal(&voter, &999, &1000, &true);
        assert!(result.is_err());
    }

    #[test]
    fn test_finalize_proposal_passed() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "AddChain"), &0, &1,
        );

        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        client.vote_cross_chain_proposal(&v1, &id, &700, &true);
        client.vote_cross_chain_proposal(&v2, &id, &300, &false);

        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (14 * 86400) + 1,
            protocol_version: 25,
            sequence_number: env.ledger().sequence(),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });

        let status = client.finalize_cross_chain_proposal(&id);
        assert_eq!(status, 1); // Passed
    }

    #[test]
    fn test_finalize_proposal_rejected() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "Reject"), &0, &1,
        );

        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        client.vote_cross_chain_proposal(&v1, &id, &300, &true);
        client.vote_cross_chain_proposal(&v2, &id, &700, &false);

        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (14 * 86400) + 1,
            protocol_version: 25,
            sequence_number: env.ledger().sequence(),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });

        let status = client.finalize_cross_chain_proposal(&id);
        assert_eq!(status, 2); // Rejected
    }

    #[test]
    fn test_finalize_proposal_expired_no_votes() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "Empty"), &0, &1,
        );

        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (14 * 86400) + 1,
            protocol_version: 25,
            sequence_number: env.ledger().sequence(),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });

        let status = client.finalize_cross_chain_proposal(&id);
        assert_eq!(status, 4); // Expired
    }

    #[test]
    fn test_finalize_proposal_too_early() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "Early"), &0, &1,
        );
        let result = client.try_finalize_cross_chain_proposal(&id);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_proposal_success() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "Exec"), &0, &1,
        );

        let voter = Address::generate(&env);
        client.vote_cross_chain_proposal(&voter, &id, &1000, &true);

        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (14 * 86400) + 1,
            protocol_version: 25,
            sequence_number: env.ledger().sequence(),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });

        client.finalize_cross_chain_proposal(&id);
        let executor = Address::generate(&env);
        client.execute_cross_chain_proposal(&executor, &id);

        let proposal = client.get_cross_chain_proposal(&id);
        assert_eq!(proposal.status, 3); // Executed
    }

    #[test]
    fn test_execute_proposal_not_passed() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "Fail"), &0, &1,
        );

        let executor = Address::generate(&env);
        let result = client.try_execute_cross_chain_proposal(&executor, &id);
        assert!(result.is_err());
    }

    // ===== Stats Tests =====

    #[test]
    fn test_stats_tracking() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.send_message(&sender, &1, &0, &hash);

        let stats = client.get_stats();
        assert_eq!(stats.total_bridges, 1);
        assert_eq!(stats.total_chains, 1);
        assert_eq!(stats.total_messages, 1);
    }

    #[test]
    fn test_vote_after_voting_period() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let proposer = Address::generate(&env);
        let id = client.create_cross_chain_proposal(
            &proposer, &Symbol::new(&env, "Late"), &0, &1,
        );

        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (14 * 86400) + 1,
            protocol_version: 25,
            sequence_number: env.ledger().sequence(),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });

        let voter = Address::generate(&env);
        let result = client.try_vote_cross_chain_proposal(&voter, &id, &1000, &true);
        assert!(result.is_err());
    }

    #[test]
    fn test_confirm_already_executed_message() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        let v3 = Address::generate(&env);
        let v4 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);
        client.register_validator(&admin, &v2, &bridge_id);
        client.register_validator(&admin, &v3, &bridge_id);
        client.register_validator(&admin, &v4, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        client.confirm_message(&v1, &msg_id);
        client.confirm_message(&v2, &msg_id);
        client.confirm_message(&v3, &msg_id);

        let executor = Address::generate(&env);
        client.execute_message(&executor, &msg_id);

        let result = client.try_confirm_message(&v4, &msg_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_deactivated_validator_cannot_confirm() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let bridge_id = register_test_bridge(&env, &client, &admin);
        register_test_chain(&client, &env, &admin, 1, bridge_id);

        let v1 = Address::generate(&env);
        client.register_validator(&admin, &v1, &bridge_id);
        client.deactivate_validator(&admin, &v1, &bridge_id);

        let sender = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let msg_id = client.send_message(&sender, &1, &0, &hash);

        let result = client.try_confirm_message(&v1, &msg_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_message_not_found() {
        let (_env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.try_get_message(&999);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_asset_mapping_not_found() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let asset = Address::generate(&env);
        let result = client.try_get_asset_mapping(&asset, &1);
        assert!(result.is_err());
    }

    #[test]
    fn test_deactivate_chain_not_found() {
        let (_env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);
        let result = client.try_deactivate_chain(&admin, &999);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_bridges_and_chains() {
        let (env, client, admin, gov, oracle) = setup();
        init(&client, &admin, &gov, &oracle);

        let b1 = register_test_bridge(&env, &client, &admin);
        let b2 = register_test_bridge(&env, &client, &admin);

        register_test_chain(&client, &env, &admin, 1, b1);
        client.register_chain(&admin, &2, &Symbol::new(&env, "BSC"), &b2);

        let bridges = client.get_all_bridges();
        assert_eq!(bridges.len(), 2);

        let chains = client.get_supported_chains();
        assert_eq!(chains.len(), 2);

        let stats = client.get_stats();
        assert_eq!(stats.total_bridges, 2);
        assert_eq!(stats.total_chains, 2);
    }
}
