#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Vec, Symbol};
use stellar_insured_lib::Proposal;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    SlashingContract,
    Proposal(u64),
    ProposalCounter,
    VoterRecord(u64, Address),
    VotingPeriod,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoteRecord {
    pub voter: Address,
    pub weight: i128,
    pub is_yes: bool,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalStats {
    pub yes_votes: i128,
    pub no_votes: i128,
    pub total_votes: i128,
    pub status: Symbol,
}

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn get_voting_period(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::VotingPeriod).unwrap()
}

fn get_proposal_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::ProposalCounter).unwrap_or(0)
}

fn get_proposal_inner(env: &Env, proposal_id: u64) -> Proposal {
    env.storage().persistent().get(&DataKey::Proposal(proposal_id)).expect("Proposal not found")
}

fn set_proposal(env: &Env, proposal_id: u64, proposal: &Proposal) {
    env.storage().persistent().set(&DataKey::Proposal(proposal_id), proposal);
}

// --------------------------------------------------------

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        slashing_contract: Address,
        voting_period: u64,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::SlashingContract, &slashing_contract);
        env.storage().instance().set(&DataKey::VotingPeriod, &voting_period);
        env.storage().instance().set(&DataKey::ProposalCounter, &0u64);

        // #379: emit event for initialization
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("init")),
            admin,
        );
    }

    pub fn create_proposal(
        env: Env,
        creator: Address,
        title: String,
        description: String,
        execution_data: String,
        threshold_percentage: u32,
    ) -> u64 {
        creator.require_auth();

        let mut counter = get_proposal_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ProposalCounter, &counter);

        let proposal = Proposal {
            id: counter,
            title,
            description,
            execution_data,
            creator: creator.clone(),
            expires_at: env.ledger().timestamp() + get_voting_period(&env),
            threshold_percentage,
            yes_votes: 0,
            no_votes: 0,
            is_finalized: false,
            is_executed: false,
        };

        set_proposal(&env, counter, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("created")),
            (counter, creator),
        );

        counter
    }

    pub fn create_slashing_proposal(
        env: Env,
        creator: Address,
        target: Address,
        role: Symbol,
        reason: String,
        amount: i128,
        threshold: u32,
    ) -> u64 {
        creator.require_auth();

        let title = String::from_str(&env, "Slashing Proposal");
        let execution_data = String::from_str(&env, "slash_call");

        let mut counter = get_proposal_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ProposalCounter, &counter);

        let proposal = Proposal {
            id: counter,
            title,
            description: reason,
            execution_data,
            creator: creator.clone(),
            expires_at: env.ledger().timestamp() + get_voting_period(&env),
            threshold_percentage: threshold,
            yes_votes: 0,
            no_votes: 0,
            is_finalized: false,
            is_executed: false,
        };

        set_proposal(&env, counter, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("slash_p")),
            (counter, target, role, amount),
        );

        counter
    }

    pub fn vote(env: Env, voter: Address, proposal_id: u64, weight: i128, is_yes: bool) {
        voter.require_auth();

        let mut proposal = get_proposal_inner(&env, proposal_id);

        if env.ledger().timestamp() > proposal.expires_at {
            panic!("Voting period ended");
        }

        let record_key = DataKey::VoterRecord(proposal_id, voter.clone());
        if env.storage().persistent().has(&record_key) {
            panic!("Already voted");
        }

        if is_yes {
            proposal.yes_votes += weight;
        } else {
            proposal.no_votes += weight;
        }

        let record = VoteRecord {
            voter: voter.clone(),
            weight,
            is_yes,
            timestamp: env.ledger().timestamp(),
        };

        set_proposal(&env, proposal_id, &proposal);
        env.storage().persistent().set(&record_key, &record);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("vote")),
            (proposal_id, voter),
        );
    }

    pub fn finalize_proposal(env: Env, proposal_id: u64) {
        let mut proposal = get_proposal_inner(&env, proposal_id);

        if env.ledger().timestamp() <= proposal.expires_at {
            panic!("Voting period not yet ended");
        }

        proposal.is_finalized = true;
        set_proposal(&env, proposal_id, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("final")),
            proposal_id,
        );
    }

    pub fn execute_proposal(env: Env, proposal_id: u64) {
        let mut proposal = get_proposal_inner(&env, proposal_id);

        if !proposal.is_finalized {
            panic!("Proposal must be finalized first");
        }

        if proposal.is_executed {
            panic!("Already executed");
        }

        let total_votes = proposal.yes_votes + proposal.no_votes;
        if total_votes == 0 || (proposal.yes_votes * 100 / total_votes) < proposal.threshold_percentage as i128 {
            panic!("Threshold not met");
        }

        proposal.is_executed = true;
        set_proposal(&env, proposal_id, &proposal);

        // #379: emit event for admin/governance action
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("exec")),
            proposal_id,
        );
    }

    pub fn execute_slashing_proposal(env: Env, proposal_id: u64) {
        Self::execute_proposal(env, proposal_id);
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        get_proposal_inner(&env, proposal_id)
    }

    pub fn get_active_proposals(env: Env) -> Vec<u64> {
        let counter = get_proposal_counter(&env);
        let mut list = Vec::new(&env);
        let now = env.ledger().timestamp();
        for i in 1..=counter {
            if let Some(p) = env.storage().persistent().get::<DataKey, Proposal>(&DataKey::Proposal(i)) {
                if !p.is_finalized && now <= p.expires_at {
                    list.push_back(i);
                }
            }
        }
        list
    }

    pub fn get_proposal_stats(env: Env, proposal_id: u64) -> ProposalStats {
        let p = get_proposal_inner(&env, proposal_id);
        let now = env.ledger().timestamp();
        let status = if p.is_executed {
            symbol_short!("executed")
        } else if p.is_finalized {
            symbol_short!("finalized")
        } else if now > p.expires_at {
            symbol_short!("expired")
        } else {
            symbol_short!("active")
        };

        ProposalStats {
            yes_votes: p.yes_votes,
            no_votes: p.no_votes,
            total_votes: p.yes_votes + p.no_votes,
            status,
        }
    }

    pub fn get_all_proposals(env: Env) -> Vec<u64> {
        let counter = get_proposal_counter(&env);
        let mut list = Vec::new(&env);
        for i in 1..=counter {
            list.push_back(i);
        }
        list
    }

    pub fn get_vote_record(env: Env, proposal_id: u64, voter: Address) -> Option<VoteRecord> {
        env.storage().persistent().get(&DataKey::VoterRecord(proposal_id, voter))
    }
}
