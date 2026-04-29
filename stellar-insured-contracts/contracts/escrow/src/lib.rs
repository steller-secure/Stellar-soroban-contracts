#![no_std]

mod storage;
mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Vec};

use storage::DataKey;
use types::{ApprovalType, EscrowData, EscrowStatus, MultiSigConfig};
use validation::{
    get_admin, require_future_timestamp, require_non_zero_address, require_non_zero_u64,
    require_not_paused, require_positive_amount, require_valid_multisig,
};

const CONTRACT_VERSION: u32 = 1;
const MAX_PARTICIPANTS: u32 = 10;

#[contract]
pub struct AdvancedEscrow;

#[contractimpl]
impl AdvancedEscrow {
    pub fn init(env: Env, admin: Address) {
        require_non_zero_address(&admin);
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Version, &CONTRACT_VERSION);
        env.storage().instance().set(&DataKey::EscrowCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);

        env.events()
            .publish((symbol_short!("escrow"), symbol_short!("init")), admin);
    }

    pub fn set_pause(env: Env, admin: Address, paused: bool) {
        admin.require_auth();
        require_non_zero_address(&admin);
        // Use shared helper to read admin — one read, no duplication (#351, #353).
        if admin != get_admin(&env) {
            panic!("Unauthorized");
        }
        env.storage().instance().set(&DataKey::Paused, &paused);

        env.events()
            .publish((symbol_short!("escrow"), symbol_short!("pause")), paused);
    }

    pub fn create_escrow_advanced(
        env: Env,
        property_id: u64,
        amount: i128,
        buyer: Address,
        seller: Address,
        participants: Vec<Address>,
        required_signatures: u32,
        release_time_lock: Option<u64>,
        nonce: u64,
    ) -> u64 {
        require_not_paused(&env);
        require_non_zero_u64(property_id, "property_id");
        require_positive_amount(amount, "amount");

        // Nonce validation for replay protection (#349)
        let current_nonce: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Nonce(buyer.clone()))
            .unwrap_or(0);
        if nonce != current_nonce + 1 {
            panic!("Invalid nonce");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Nonce(buyer.clone()), &nonce);

        if participants.len() > MAX_PARTICIPANTS {
            panic!("Too many participants");
        }
        require_valid_multisig(required_signatures, participants.len());
        require_non_zero_address(&buyer);
        require_non_zero_address(&seller);
        for participant in participants.iter() {
            require_non_zero_address(participant);
        }
        if let Some(time_lock) = release_time_lock {
            require_future_timestamp(time_lock, env.ledger().timestamp(), "release_time_lock");
        }

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCount)
            .unwrap_or(0);
        count += 1;
        env.storage().instance().set(&DataKey::EscrowCount, &count);

        let escrow_data = EscrowData {
            id: count,
            property_id,
            buyer,
            seller,
            amount,
            deposited_amount: 0,
            status: EscrowStatus::Created,
            created_at: env.ledger().timestamp(),
            release_time_lock,
            participants: participants.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(count), &escrow_data);

        let config = MultiSigConfig {
            required_signatures,
            signers: participants,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MultiSig(count), &config);

        // Standardized event format: (contract, action) topics + structured payload (#352).
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("created")),
            (count, property_id, amount),
        );

        count
    }

    pub fn deposit_funds(env: Env, escrow_id: u64, amount: i128) {
        require_not_paused(&env);
        require_non_zero_u64(escrow_id, "escrow_id");
        require_positive_amount(amount, "amount");

        let mut escrow: EscrowData = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .expect("Escrow not found");

        if escrow.status != EscrowStatus::Created && escrow.status != EscrowStatus::Funded {
            panic!("Invalid status");
        }
        let new_deposit_total = escrow
            .deposited_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic!("Deposit exceeds escrow amount"));
        if new_deposit_total > escrow.amount {
            panic!("Deposit exceeds escrow amount");
        }

        escrow.deposited_amount = new_deposit_total;
        escrow.status = if escrow.deposited_amount >= escrow.amount {
            EscrowStatus::Active
        } else {
            EscrowStatus::Funded
        };

        // Single write after all mutations — avoids intermediate writes (#351).
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("funded")),
            (escrow_id, amount),
        );
    }

    pub fn release_funds(env: Env, escrow_id: u64) {
        require_not_paused(&env);
        require_non_zero_u64(escrow_id, "escrow_id");

        let mut escrow: EscrowData = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .expect("Escrow not found");

        if escrow.status != EscrowStatus::Active {
            panic!("Invalid status");
        }

        if let Some(time_lock) = escrow.release_time_lock {
            if env.ledger().timestamp() < time_lock {
                panic!("Time lock active");
            }
        }

        let sig_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SigCount(escrow_id, ApprovalType::Release))
            .unwrap_or(0);
        let config: MultiSigConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiSig(escrow_id))
            .unwrap();

        if sig_count < config.required_signatures {
            panic!("Signature threshold not met");
        }

        let amount = escrow.deposited_amount;
        escrow.status = EscrowStatus::Released;
        escrow.deposited_amount = 0;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("released")),
            (escrow_id, amount),
        );
    }

    pub fn sign_approval(env: Env, escrow_id: u64, approval_type: ApprovalType, signer: Address) {
        require_not_paused(&env);
        require_non_zero_u64(escrow_id, "escrow_id");
        signer.require_auth();
        require_non_zero_address(&signer);

        let config: MultiSigConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiSig(escrow_id))
            .expect("Escrow not found");

        if !config.signers.contains(signer.clone()) {
            panic!("Unauthorized");
        }

        if env.storage().persistent().has(&DataKey::Signature(
            escrow_id,
            approval_type,
            signer.clone(),
        )) {
            panic!("Already signed");
        }

        env.storage().persistent().set(
            &DataKey::Signature(escrow_id, approval_type, signer.clone()),
            &true,
        );

        // Read-increment-write in one place; no separate read before the set (#351).
        let mut count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SigCount(escrow_id, approval_type))
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::SigCount(escrow_id, approval_type), &count);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("signed")),
            (escrow_id, signer, count),
        );
    }
}

#[contractimpl]
impl AdvancedEscrow {
    pub fn version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(CONTRACT_VERSION)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> Option<EscrowData> {
        env.storage().persistent().get(&DataKey::Escrow(escrow_id))
    }

    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::EscrowCount).unwrap_or(0)
    }

    pub fn get_multisig_config(env: Env, escrow_id: u64) -> Option<MultiSigConfig> {
        env.storage().persistent().get(&DataKey::MultiSig(escrow_id))
    }

    pub fn get_sig_count(env: Env, escrow_id: u64, approval_type: ApprovalType) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::SigCount(escrow_id, approval_type))
            .unwrap_or(0)
    }

    pub fn get_nonce(env: Env, address: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Nonce(address))
            .unwrap_or(0)
    }
}
