use crate::state::{
    BatchUndelegationRecord, Config, PoolRegistryInfo, BATCH_UNDELEGATION_REGISTRY, POOL_REGISTRY,
};
use crate::ContractError;
use cosmwasm_std::{Addr, Decimal, Env, MessageInfo, QuerierWrapper, Storage, Uint128};
use cw_storage_plus::U64Key;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Verify {
    SenderManager,

    //Info.funds is expected to be one
    NonZeroSingleInfoFund,
    // If info.funds are empty or zero
    // NonEmptyInfoFunds,
    NoFunds,
}

pub fn validate(
    config: &Config,
    info: &MessageInfo,
    _env: &Env,
    checks: Vec<Verify>,
) -> Result<(), ContractError> {
    for check in checks {
        match check {
            Verify::SenderManager => {
                if info.sender != config.manager {
                    return Err(ContractError::Unauthorized {});
                }
            }
            Verify::NonZeroSingleInfoFund => {
                if info.funds.is_empty() || info.funds[0].amount.is_zero() {
                    return Err(ContractError::NoFunds {});
                }
                if info.funds.len() > 1 {
                    return Err(ContractError::MultipleFunds {});
                }
                if info.funds[0].denom != config.vault_denom {
                    return Err(ContractError::InvalidDenom {});
                }
            }
            Verify::NoFunds => {
                if !info.funds.is_empty() {
                    return Err(ContractError::FundsNotExpected {});
                }
            }
        }
    }

    Ok(())
}

pub fn get_verified_pool(
    storage: &mut dyn Storage,
    pool_id: u64,
    active_check: bool,
) -> Result<PoolRegistryInfo, ContractError> {
    let pool_meta_opt = POOL_REGISTRY.may_load(storage, U64Key::new(pool_id))?;
    if pool_meta_opt.is_none() {
        return Err(ContractError::PoolNotFound {});
    }
    let pool_meta = pool_meta_opt.unwrap();
    if active_check && !pool_meta.active {
        return Err(ContractError::PoolInactive {});
    }
    Ok(pool_meta)
}

// Take in validator staked amounts into pool if the pool size is bigger.
pub fn get_validator_for_deposit(
    querier: QuerierWrapper,
    validator_contract: Addr,
    validators: Vec<Addr>,
) -> Result<Addr, ContractError> {
    if validators.is_empty() {
        return Err(ContractError::NoValidatorsInPool {});
    }

    let mut stake_tuples = vec![];
    for val_addr in validators {
        if querier.query_validator(val_addr.clone())?.is_none() {
            // Don't deposit to a jailed validator
            continue;
        }
        let delegation_opt =
            querier.query_delegation(validator_contract.clone(), val_addr.clone())?;

        if delegation_opt.is_none() {
            // No delegation. So use the validator
            return Ok(val_addr);
        }
        stake_tuples.push((
            delegation_opt.unwrap().amount.amount.u128(),
            val_addr.to_string(),
        ))
    }
    if stake_tuples.is_empty() {
        return Err(ContractError::AllValidatorsJailed {});
    }
    stake_tuples.sort();
    Ok(Addr::unchecked(stake_tuples.first().unwrap().clone().1))
}

// Take in validator staked amounts into pool if the pool size is bigger.
pub fn get_active_validators_sorted_by_stake(
    querier: QuerierWrapper,
    validator_contract: Addr,
    validators: Vec<Addr>,
) -> Result<Vec<(Uint128, String)>, ContractError> {
    if validators.is_empty() {
        return Err(ContractError::NoValidatorsInPool {});
    }
    let mut stake_tuples = vec![];
    for val_addr in validators {
        if querier.query_validator(val_addr.clone())?.is_none() {
            // Don't deposit to a jailed validator
            continue;
        }
        let delegation_opt =
            querier.query_delegation(validator_contract.clone(), val_addr.clone())?;
        if delegation_opt.is_none() {
            // No delegation. So can
            stake_tuples.push((Uint128::zero(), val_addr.to_string()));
        } else {
            stake_tuples.push((delegation_opt.unwrap().amount.amount, val_addr.to_string()))
        }
    }
    if stake_tuples.is_empty() {
        return Err(ContractError::AllValidatorsJailed {});
    }
    stake_tuples.sort();
    Ok(stake_tuples)
}

pub fn create_new_undelegation_batch(
    storage: &mut dyn Storage,
    env: Env,
    pool_id: u64,
    pool_meta: &mut PoolRegistryInfo,
) -> Result<(), ContractError> {
    pool_meta.current_undelegation_batch_id += 1;
    let new_batch_id = pool_meta.current_undelegation_batch_id;
    POOL_REGISTRY.save(storage, U64Key::new(pool_id), &pool_meta)?;

    BATCH_UNDELEGATION_REGISTRY.save(
        storage,
        (U64Key::new(pool_id), U64Key::new(new_batch_id)),
        &BatchUndelegationRecord {
            prorated_amount: Decimal::zero(),
            undelegated_amount: Uint128::zero(),
            create_time: env.block.time,
            est_release_time: None,
            reconciled: false,
            last_updated_slashing_pointer: pool_meta.slashing_pointer,
            unbonding_slashing_ratio: Decimal::one(),
        },
    )?;
    Ok(())
}
