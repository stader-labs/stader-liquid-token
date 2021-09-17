use crate::state::{Config, UserPoolInfo};
use crate::ContractError;
use cosmwasm_std::{Env, MessageInfo, Addr, DepsMut, Response, Uint128, Storage, Decimal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cw_storage_plus::U64Key;
use stader_utils::coin_utils::{merge_dec_coin_vector, DecCoinVecOp, Operation, deccoin_vec_to_coin_vec, multiply_deccoin_vector_with_uint128, multiply_u128_with_decimal, decimal_subtraction_in_256, DecCoin};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Verify {
    // If info.sender != manager, throw error
    SenderManager,

    SenderPoolsContract,

    //Info.funds is expected to be one
    NonZeroSingleInfoFund,

    //make sure there are no sent funds
    NoFunds,
}

pub fn validate(
    config: &Config,
    info: &MessageInfo,
    env: &Env,
    checks: Vec<Verify>,
) -> Result<(), ContractError> {
    for check in checks {
        match check {
            Verify::SenderManager => {
                if info.sender != config.manager {
                    return Err(ContractError::Unauthorized {});
                }
            }
            Verify::SenderPoolsContract => {
                if info.sender != config.pools_contract {
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

// Does an error here stop execution?
pub fn update_user_pointers(user_info: &mut UserPoolInfo, airdrops_pointer: Vec<DecCoin>, rewards_pointer: Decimal) {
    let airdrop_pointer_difference = merge_dec_coin_vector(&airdrops_pointer, DecCoinVecOp {
        operation: Operation::Sub,
        fund: user_info.airdrops_pointer.clone()
    });
    user_info.pending_airdrops = deccoin_vec_to_coin_vec(&multiply_deccoin_vector_with_uint128(&airdrop_pointer_difference, user_info.deposit.staked));
    user_info.airdrops_pointer = airdrops_pointer;
    user_info.pending_rewards = Uint128::new(multiply_u128_with_decimal(
        user_info.deposit.staked.u128(), decimal_subtraction_in_256(rewards_pointer.clone(), user_info.rewards_pointer)));
    user_info.rewards_pointer = rewards_pointer;
}
