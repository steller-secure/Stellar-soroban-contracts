#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, Vec,
};

#[contracttype]
#[derive(Clone)]
pub struct Appeal {
    pub target: Address,
    pub reason: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub is_frozen: bool,
    pub is_resolved: bool,
    pub deadline: u64,
    pub deposit: i128,
}

#[contract]
pub struct SlashingAppealContract;

#[contractimpl]
impl SlashingAppealContract {

    // Appeal a slashing decision
    pub fn appeal_slashing(
        env: Env,
        target: Address,
        reason: String,
        deposit: i128,
    ) -> u32 {
        target.require_auth();

        // Deposit required to prevent frivolous appeals
        assert!(deposit >= 100, "Minimum deposit is 100");

        let appeal_id: u32 = env
            .storage()
            .instance()
            .get(&"appeal_count")
            .unwrap_or(0) + 1;

        let deadline = env.ledger().timestamp() + 604800; // 7 days

        let appeal = Appeal {
            target,
            reason,
            votes_for: 0,
            votes_against: 0,
            is_frozen: true, // freeze funds immediately
            is_resolved: false,
            deadline,
            deposit,
        };

        env.storage().instance().set(&appeal_id, &appeal);
        env.storage().instance().set(&"appeal_count", &appeal_id);

        appeal_id
    }

    // Vote on an appeal
    pub fn vote_on_appeal(
        env: Env,
        appeal_id: u32,
        voter: Address,
        approve: bool,
    ) {
        voter.require_auth();

        let mut appeal: Appeal = env
            .storage()
            .instance()
            .get(&appeal_id)
            .expect("Appeal not found");

        assert!(!appeal.is_resolved, "Appeal already resolved");
        assert!(
            env.ledger().timestamp() <= appeal.deadline,
            "Appeal deadline passed"
        );

        if approve {
            appeal.votes_for += 1;
        } else {
            appeal.votes_against += 1;
        }

        env.storage().instance().set(&appeal_id, &appeal);
    }

    // Resolve the appeal after voting
    pub fn resolve_appeal(env: Env, appeal_id: u32) -> bool {
        let mut appeal: Appeal = env
            .storage()
            .instance()
            .get(&appeal_id)
            .expect("Appeal not found");

        assert!(!appeal.is_resolved, "Already resolved");
        assert!(
            env.ledger().timestamp() > appeal.deadline,
            "Voting still ongoing"
        );

        let approved = appeal.votes_for > appeal.votes_against;

        appeal.is_resolved = true;
        appeal.is_frozen = false;

        env.storage().instance().set(&appeal_id, &appeal);

        // Return true if appeal succeeded (refund granted)
        approved
    }

    // Check appeal status
    pub fn get_appeal(env: Env, appeal_id: u32) -> Appeal {
        env.storage()
            .instance()
            .get(&appeal_id)
            .expect("Appeal not found")
    }
}