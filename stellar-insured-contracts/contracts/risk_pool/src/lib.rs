#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    MinStake,
    TotalCapital,
    AvailableCapital,
    ClaimsPaid,
    ProviderStake(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolStats {
    pub total_capital: i128,
    pub available_capital: i128,
    pub total_claims_paid: i128,
}

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn get_token(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Token).unwrap()
}

fn get_total_capital(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalCapital).unwrap_or(0)
}

fn get_available_capital(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::AvailableCapital).unwrap_or(0)
}

fn get_provider_stake(env: &Env, provider: &Address) -> i128 {
    env.storage().persistent().get(&DataKey::ProviderStake(provider.clone())).unwrap_or(0)
}

// --------------------------------------------------------

#[contract]
pub struct RiskPoolContract;

#[contractimpl]
impl RiskPoolContract {
    pub fn initialize(env: Env, admin: Address, token: Address, min_stake: i128) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::MinStake, &min_stake);
        env.storage().instance().set(&DataKey::TotalCapital, &0i128);
        env.storage().instance().set(&DataKey::AvailableCapital, &0i128);
        env.storage().instance().set(&DataKey::ClaimsPaid, &0i128);
    }

    pub fn deposit_liquidity(env: Env, provider: Address, amount: i128) {
        provider.require_auth();

        let min_stake: i128 = env.storage().instance().get(&DataKey::MinStake).unwrap();
        if amount < min_stake {
            panic!("Amount below minimum stake");
        }

        let client = soroban_sdk::token::Client::new(&env, &get_token(&env));
        client.transfer(&provider, &env.current_contract_address(), &amount);

        let new_stake = get_provider_stake(&env, &provider) + amount;
        env.storage().persistent().set(&DataKey::ProviderStake(provider), &new_stake);

        env.storage().instance().set(&DataKey::TotalCapital, &(get_total_capital(&env) + amount));
        env.storage().instance().set(&DataKey::AvailableCapital, &(get_available_capital(&env) + amount));

        env.events().publish((symbol_short!("pool"), symbol_short!("deposit")), amount);
    }

    pub fn withdraw_liquidity(env: Env, provider: Address, amount: i128) {
        provider.require_auth();

        let stake = get_provider_stake(&env, &provider);
        if stake < amount {
            panic!("Insufficient stake");
        }

        let avail = get_available_capital(&env);
        if avail < amount {
            panic!("Insufficient available capital in pool");
        }

        let client = soroban_sdk::token::Client::new(&env, &get_token(&env));
        client.transfer(&env.current_contract_address(), &provider, &amount);

        env.storage().persistent().set(&DataKey::ProviderStake(provider), &(stake - amount));
        env.storage().instance().set(&DataKey::TotalCapital, &(get_total_capital(&env) - amount));
        env.storage().instance().set(&DataKey::AvailableCapital, &(avail - amount));

        env.events().publish((symbol_short!("pool"), symbol_short!("withdraw")), amount);
    }

    pub fn payout_claim(env: Env, recipient: Address, amount: i128) {
        let admin = get_admin(&env);
        admin.require_auth();

        let avail = get_available_capital(&env);
        if avail < amount {
            panic!("Insufficient pool funds for payout");
        }

        let client = soroban_sdk::token::Client::new(&env, &get_token(&env));
        client.transfer(&env.current_contract_address(), &recipient, &amount);

        env.storage().instance().set(&DataKey::AvailableCapital, &(avail - amount));

        let paid: i128 = env.storage().instance().get(&DataKey::ClaimsPaid).unwrap_or(0);
        env.storage().instance().set(&DataKey::ClaimsPaid, &(paid + amount));

        env.events().publish((symbol_short!("pool"), symbol_short!("payout")), amount);
    }

    pub fn get_pool_stats(env: Env) -> PoolStats {
        PoolStats {
            total_capital: get_total_capital(&env),
            available_capital: get_available_capital(&env),
            total_claims_paid: env.storage().instance().get(&DataKey::ClaimsPaid).unwrap_or(0),
        }
    }

    pub fn get_provider_info(env: Env, provider: Address) -> i128 {
        get_provider_stake(&env, &provider)
    }
}
