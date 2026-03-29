#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, token, symbol_short};

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct SubDaoPool {
    pub main_dao: Address,
    pub sub_admin: Address,
    pub grant_asset: Address,
    pub total_allocation: i128,
    pub distributed_amount: i128,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum SubDaoDataKey {
    Pool(Address),
    SubGrant(Address, Address),
}

#[contract]
pub struct SubDaoAuthority;

#[contractimpl]
impl SubDaoAuthority {
    pub fn create_delegated_pool(env: Env, main_dao: Address, sub_admin: Address, grant_asset: Address, amount: i128) {
        main_dao.require_auth();
        if amount <= 0 { panic!("Invalid amount"); }
        let pool = SubDaoPool { main_dao: main_dao.clone(), sub_admin: sub_admin.clone(), grant_asset: grant_asset.clone(), total_allocation: amount, distributed_amount: 0 };
        env.storage().instance().set(&SubDaoDataKey::Pool(sub_admin.clone()), &pool);
        env.events().publish((symbol_short!("pool_created"), sub_admin), (main_dao, grant_asset, amount));
    }
    
    pub fn distribute_sub_grant(env: Env, sub_admin: Address, recipient: Address, amount: i128) {
        sub_admin.require_auth();
        if amount <= 0 { panic!("Invalid amount"); }
        let mut pool: SubDaoPool = env.storage().instance().get(&SubDaoDataKey::Pool(sub_admin.clone())).expect("Pool not found");
        if pool.distributed_amount + amount > pool.total_allocation { panic!("Exceeds allocation"); }
        
        pool.distributed_amount += amount;
        env.storage().instance().set(&SubDaoDataKey::Pool(sub_admin.clone()), &pool);
        
        let current_grant: i128 = env.storage().instance().get(&SubDaoDataKey::SubGrant(sub_admin.clone(), recipient.clone())).unwrap_or(0);
        env.storage().instance().set(&SubDaoDataKey::SubGrant(sub_admin.clone(), recipient.clone()), &(current_grant + amount));
        token::Client::new(&env, &pool.grant_asset).transfer(&env.current_contract_address(), &recipient, &amount);
        env.events().publish((symbol_short!("sub_grant"), sub_admin), (recipient, amount));
    }
    
    pub fn revoke_sub_grant(env: Env, sub_admin: Address, recipient: Address, amount: i128) {
        sub_admin.require_auth();
        if amount <= 0 { panic!("Invalid amount"); }
        let mut pool: SubDaoPool = env.storage().instance().get(&SubDaoDataKey::Pool(sub_admin.clone())).expect("Pool not found");
        let current_grant: i128 = env.storage().instance().get(&SubDaoDataKey::SubGrant(sub_admin.clone(), recipient.clone())).unwrap_or(0);
        if amount > current_grant { panic!("Exceeds current grant"); }
        
        pool.distributed_amount -= amount;
        env.storage().instance().set(&SubDaoDataKey::Pool(sub_admin.clone()), &pool);
        env.storage().instance().set(&SubDaoDataKey::SubGrant(sub_admin.clone(), recipient.clone()), &(current_grant - amount));
    }
}