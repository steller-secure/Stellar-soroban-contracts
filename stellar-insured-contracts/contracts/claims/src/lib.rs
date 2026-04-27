#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};
use stellar_insured_lib::{InsuranceClaim, ClaimStatus, InsurancePolicy, PolicyStatus};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PolicyContract,
    RiskPool,
    Claim(u64),
    ClaimCounter,
}

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn get_claim_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::ClaimCounter).unwrap_or(0)
}

fn get_claim_inner(env: &Env, claim_id: u64) -> InsuranceClaim {
    env.storage().persistent().get(&DataKey::Claim(claim_id)).expect("Claim not found")
}

fn set_claim(env: &Env, claim_id: u64, claim: &InsuranceClaim) {
    env.storage().persistent().set(&DataKey::Claim(claim_id), claim);
}

// --------------------------------------------------------

#[contract]
pub struct ClaimsContract;

#[contractimpl]
impl ClaimsContract {
    pub fn initialize(env: Env, admin: Address, policy_contract: Address, risk_pool: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::PolicyContract, &policy_contract);
        env.storage().instance().set(&DataKey::RiskPool, &risk_pool);
        env.storage().instance().set(&DataKey::ClaimCounter, &0u64);
    }

    pub fn submit_claim(env: Env, policy_id: u64, amount: i128) -> u64 {
        // #381: fetch policy and validate consistency before accepting claim
        let policy_contract: Address = env.storage().instance().get(&DataKey::PolicyContract).unwrap();
        let policy: InsurancePolicy = env.invoke_contract(
            &policy_contract,
            &symbol_short!("get_pol"),
            (policy_id,).into(),
        );

        // Consistency check: policy must be active
        if policy.status != PolicyStatus::Active && policy.status != PolicyStatus::Renewed {
            panic!("Policy is not active");
        }

        // Consistency check: claim amount must not exceed coverage
        if amount <= 0 || amount > policy.coverage_amount {
            panic!("Claim amount invalid or exceeds coverage");
        }

        let claimant = policy.holder.clone();
        claimant.require_auth();

        let mut counter = get_claim_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ClaimCounter, &counter);

        let claim = InsuranceClaim {
            claim_id: counter,
            policy_id,
            claimant,
            amount,
            status: ClaimStatus::Submitted,
            submitted_at: env.ledger().timestamp(),
        };

        set_claim(&env, counter, &claim);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("submitted")),
            (counter, policy_id),
        );

        counter
    }

    pub fn start_review(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::Submitted {
            panic!("Invalid claim status for review");
        }

        claim.status = ClaimStatus::UnderReview;
        set_claim(&env, claim_id, &claim);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("review")),
            claim_id,
        );
    }

    pub fn approve_claim(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::UnderReview {
            panic!("Claim must be under review to approve");
        }

        claim.status = ClaimStatus::Approved;
        set_claim(&env, claim_id, &claim);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("approved")),
            claim_id,
        );
    }

    pub fn reject_claim(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::UnderReview {
            panic!("Claim must be under review to reject");
        }

        claim.status = ClaimStatus::Rejected;
        set_claim(&env, claim_id, &claim);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("rejected")),
            claim_id,
        );
    }

    pub fn settle_claim(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::Approved {
            panic!("Only approved claims can be settled");
        }

        let risk_pool: Address = env.storage().instance().get(&DataKey::RiskPool).unwrap();

        env.invoke_contract::<()>(
            &risk_pool,
            &symbol_short!("payout"),
            (claim.claimant.clone(), claim.amount).into(),
        );

        claim.status = ClaimStatus::Settled;
        set_claim(&env, claim_id, &claim);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("settled")),
            claim_id,
        );
    }

    pub fn get_claim(env: Env, claim_id: u64) -> InsuranceClaim {
        get_claim_inner(&env, claim_id)
    }

    pub fn get_stats(env: Env) -> u64 {
        get_claim_counter(&env)
    }
}
