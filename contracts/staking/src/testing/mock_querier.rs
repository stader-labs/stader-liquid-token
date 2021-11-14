#![allow(dead_code)]

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Binary, Coin, ContractResult, Decimal,
    FullDelegation, OwnedDeps, Querier, QuerierResult, QueryRequest, SystemError, SystemResult,
    Uint128, Validator, WasmQuery,
};
use std::collections::HashMap;

use cw20::BalanceResponse;
use delegator::msg::{QueryMsg as DelegatorQueryMsg, UserPoolResponse};
use stader_utils::coin_utils::{decimal_multiplication_in_256, u128_from_decimal};
use terra_cosmwasm::{
    SwapResponse, TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute,
};
use validator::msg::QueryMsg as ValidatorQueryMsg;

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub fn mock_dependencies_for_delegator_querier(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, DelegatorWasmMockQuerier> {
    let contract_addr = MOCK_CONTRACT_ADDR;
    let custom_querier: DelegatorWasmMockQuerier =
        DelegatorWasmMockQuerier::new(MockQuerier::new(&[(contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub fn mock_dependencies_for_validator_querier(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, ValidatorWasmMockQuerier> {
    let contract_addr = MOCK_CONTRACT_ADDR;
    let custom_querier: ValidatorWasmMockQuerier =
        ValidatorWasmMockQuerier::new(MockQuerier::new(&[(contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

#[derive(Clone, Default)]
pub struct TaxQuerier {
    rate: Decimal,
    caps: HashMap<String, Uint128>,
}

impl TaxQuerier {
    pub fn _new(rate: Decimal, caps: &[(&String, &Uint128)]) -> Self {
        TaxQuerier {
            rate,
            caps: _caps_to_map(caps),
        }
    }
}

pub fn _caps_to_map(caps: &[(&String, &Uint128)]) -> HashMap<String, Uint128> {
    let mut owner_map: HashMap<String, Uint128> = HashMap::new();
    for (denom, cap) in caps.iter() {
        owner_map.insert(denom.to_string(), **cap);
    }
    owner_map
}

pub struct DelegatorWasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    tax_querier: TaxQuerier,
    stader_querier: StaderQuerier,
    swap_querier: SwapQuerier,
}

pub struct ValidatorWasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    tax_querier: TaxQuerier,
    stader_querier: StaderQuerier,
    swap_querier: SwapQuerier,
}

impl Querier for DelegatorWasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(_e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request:"),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl Querier for ValidatorWasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(_e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request:"),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl DelegatorWasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse {
                                rate: self.tax_querier.rate,
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom)
                                .copied()
                                .unwrap_or_default();
                            let res = TaxCapResponse { cap };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => panic!("Terra Treasury route query not implemented!"),
                    }
                } else if &TerraRoute::Market == route {
                    match query_data {
                        TerraQuery::Swap {
                            offer_coin,
                            ask_denom,
                        } => {
                            let offer_coin = offer_coin.clone();
                            let ask_denom = ask_denom.clone();
                            let coin_swap_rate_opt =
                                self.swap_querier.swap_rates.iter().find(|x| {
                                    x.offer_denom.eq(&offer_coin.denom)
                                        && x.ask_denom.eq(&ask_denom)
                                });
                            let swap_res: SwapResponse = if let Some(coin_swap_rate) =
                                coin_swap_rate_opt
                            {
                                let swap_amount = u128_from_decimal(decimal_multiplication_in_256(
                                    Decimal::from_ratio(offer_coin.amount, 1_u128),
                                    coin_swap_rate.swap_rate,
                                ));

                                SwapResponse {
                                    receive: Coin::new(swap_amount, ask_denom),
                                }
                            } else {
                                return SystemResult::Err(SystemError::InvalidRequest {
                                    error: "swap not found".to_string(),
                                    request: Default::default(),
                                });
                            };

                            SystemResult::Ok(ContractResult::from(to_binary(&swap_res)))
                        }
                        _ => {
                            panic!("Terra Market route query not implemented!")
                        }
                    }
                } else {
                    panic!("Terra route not implemented!")
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw {
                contract_addr: _,
                key: _,
            }) => {
                panic!("WASMQUERY::RAW not implemented!")
            }
            QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: _,
                msg,
            }) => match from_binary(msg).unwrap() {
                DelegatorQueryMsg::State { .. } => {
                    SystemResult::Ok(ContractResult::from(to_binary(&Binary::default())))
                }
                DelegatorQueryMsg::User { .. } => {
                    SystemResult::Ok(ContractResult::from(to_binary(&Binary::default())))
                }
                DelegatorQueryMsg::UserPool { .. } => {
                    SystemResult::Ok(ContractResult::from(to_binary(&Binary::default())))
                }
                DelegatorQueryMsg::Config { .. } => {
                    SystemResult::Ok(ContractResult::from(to_binary(&Binary::default())))
                }
                DelegatorQueryMsg::ComputeUndelegationAmounts { .. } => SystemResult::Ok(
                    ContractResult::from(to_binary(&(Uint128::new(100), Uint128::new(80)))),
                ),
                DelegatorQueryMsg::ComputeUserInfo { .. } => {
                    SystemResult::Ok(ContractResult::from(to_binary(&UserPoolResponse {
                        info: None,
                    })))
                }
            },

            _ => self.base.handle_query(request),
        }
    }
    pub fn update_staking(
        &mut self,
        denom: &str,
        validators: &[Validator],
        delegations: &[FullDelegation],
    ) {
        self.base.update_staking(denom, validators, delegations);
    }

    pub fn update_balance(&mut self, addr: Addr, balances: Vec<Coin>) -> Option<Vec<Coin>> {
        self.base.update_balance(addr.to_string(), balances)
    }
}

impl ValidatorWasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse {
                                rate: self.tax_querier.rate,
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom)
                                .copied()
                                .unwrap_or_default();
                            let res = TaxCapResponse { cap };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => panic!("Terra Treasury route query not implemented!"),
                    }
                } else if &TerraRoute::Market == route {
                    match query_data {
                        TerraQuery::Swap {
                            offer_coin,
                            ask_denom,
                        } => {
                            let offer_coin = offer_coin.clone();
                            let ask_denom = ask_denom.clone();
                            let coin_swap_rate_opt =
                                self.swap_querier.swap_rates.iter().find(|x| {
                                    x.offer_denom.eq(&offer_coin.denom)
                                        && x.ask_denom.eq(&ask_denom)
                                });
                            let swap_res: SwapResponse = if let Some(coin_swap_rate) =
                                coin_swap_rate_opt
                            {
                                let swap_amount = u128_from_decimal(decimal_multiplication_in_256(
                                    Decimal::from_ratio(offer_coin.amount, 1_u128),
                                    coin_swap_rate.swap_rate,
                                ));

                                SwapResponse {
                                    receive: Coin::new(swap_amount, ask_denom),
                                }
                            } else {
                                return SystemResult::Err(SystemError::InvalidRequest {
                                    error: "swap not found".to_string(),
                                    request: Default::default(),
                                });
                            };

                            SystemResult::Ok(ContractResult::from(to_binary(&swap_res)))
                        }
                        _ => {
                            panic!("Terra Market route query not implemented!")
                        }
                    }
                } else {
                    panic!("Terra route not implemented!")
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw {
                contract_addr: _,
                key: _,
            }) => {
                panic!("WASMQUERY::RAW not implemented!")
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if contract_addr.starts_with("uair") {
                    return SystemResult::Ok(ContractResult::from(to_binary(&BalanceResponse {
                        balance: Uint128::new(20),
                    })));
                } else {
                    match from_binary(msg).unwrap() {
                        ValidatorQueryMsg::Config {} => {
                            SystemResult::Ok(ContractResult::from(to_binary(&Binary::default())))
                        }
                        ValidatorQueryMsg::ValidatorMeta { .. } => {
                            SystemResult::Ok(ContractResult::from(to_binary(&Binary::default())))
                        }
                        ValidatorQueryMsg::GetUnaccountedBaseFunds {} => SystemResult::Ok(
                            ContractResult::from(to_binary(&Coin::new(81, "utest"))),
                        ),
                    }
                }
            }
            _ => self.base.handle_query(request),
        }
    }
    pub fn update_staking(
        &mut self,
        denom: &str,
        validators: &[Validator],
        delegations: &[FullDelegation],
    ) {
        self.base.update_staking(denom, validators, delegations);
    }

    pub fn update_balance(&mut self, addr: Addr, balances: Vec<Coin>) -> Option<Vec<Coin>> {
        self.base.update_balance(addr.to_string(), balances)
    }
}

#[derive(Clone, Default)]
pub struct SwapRates {
    pub offer_denom: String,
    pub ask_denom: String,
    pub swap_rate: Decimal,
}

#[derive(Clone, Default)]
struct SwapQuerier {
    pub swap_rates: Vec<SwapRates>,
}

impl SwapQuerier {
    fn default() -> Self {
        SwapQuerier { swap_rates: vec![] }
    }

    fn new(swap_rates: Option<Vec<SwapRates>>) -> Self {
        SwapQuerier {
            swap_rates: swap_rates.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Default)]
struct StaderQuerier {
    pub contract_to_tokens: HashMap<Addr, Uint128>,
    pub contract_to_fulfillable_undelegations: HashMap<Addr, Uint128>,
}

impl StaderQuerier {
    fn default() -> Self {
        StaderQuerier {
            contract_to_tokens: HashMap::new(),
            contract_to_fulfillable_undelegations: HashMap::new(),
        }
    }
    fn new(
        contract_to_tokens: Option<HashMap<Addr, Uint128>>,
        contract_to_fulfillable_undelegations: Option<HashMap<Addr, Uint128>>,
    ) -> Self {
        StaderQuerier {
            contract_to_tokens: contract_to_tokens.unwrap_or_default(),
            contract_to_fulfillable_undelegations: contract_to_fulfillable_undelegations
                .unwrap_or_default(),
        }
    }
}

impl DelegatorWasmMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        DelegatorWasmMockQuerier {
            base,
            tax_querier: TaxQuerier::default(),
            stader_querier: StaderQuerier::default(),
            swap_querier: SwapQuerier::default(),
        }
    }

    pub fn update_stader_balances(
        &mut self,
        contract_to_tokens: Option<HashMap<Addr, Uint128>>,
        contract_to_fulfillable_undelegation: Option<HashMap<Addr, Uint128>>,
    ) {
        self.stader_querier =
            StaderQuerier::new(contract_to_tokens, contract_to_fulfillable_undelegation);
    }

    pub fn update_swap_rates(&mut self, swap_rates: Option<Vec<SwapRates>>) {
        self.swap_querier = SwapQuerier::new(swap_rates)
    }

    // configure the tax mock querier
    pub fn _with_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        self.tax_querier = TaxQuerier::_new(rate, caps);
    }
}

impl ValidatorWasmMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        ValidatorWasmMockQuerier {
            base,
            tax_querier: TaxQuerier::default(),
            stader_querier: StaderQuerier::default(),
            swap_querier: SwapQuerier::default(),
        }
    }

    pub fn update_stader_balances(
        &mut self,
        contract_to_tokens: Option<HashMap<Addr, Uint128>>,
        contract_to_fulfillable_undelegation: Option<HashMap<Addr, Uint128>>,
    ) {
        self.stader_querier =
            StaderQuerier::new(contract_to_tokens, contract_to_fulfillable_undelegation);
    }

    pub fn update_swap_rates(&mut self, swap_rates: Option<Vec<SwapRates>>) {
        self.swap_querier = SwapQuerier::new(swap_rates)
    }

    // configure the tax mock querier
    pub fn _with_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        self.tax_querier = TaxQuerier::_new(rate, caps);
    }
}