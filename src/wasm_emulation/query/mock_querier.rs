use std::str::FromStr;
use cw_orch::prelude::queriers::DaemonQuerier;
use ibc_chain_registry::chain::ChainData;
use cw_orch::daemon::GrpcChannel;
use tonic::transport::Channel;
use cw_orch::prelude::queriers::Bank;
use tokio::runtime::Runtime;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use cosmwasm_std::Binary;
use cosmwasm_std::Coin;

use cosmwasm_std::SystemError;
use cosmwasm_std::Uint128;
use cosmwasm_std::{
    AllBalanceResponse, BalanceResponse, BankQuery, CustomQuery, QueryRequest, WasmQuery,
};
use cosmwasm_std::{
    AllDelegationsResponse, AllValidatorsResponse, BondedDenomResponse, DelegationResponse,
    FullDelegation, StakingQuery, Validator, ValidatorResponse,
};
use cosmwasm_std::{ContractResult, Empty, SystemResult};
use cosmwasm_std::{from_slice, to_binary};

use cosmwasm_std::{Querier, QuerierResult};
use cosmwasm_std::Attribute;

/// The same type as cosmwasm-std's QuerierResult, but easier to reuse in
/// cosmwasm-vm. It might diverge from QuerierResult at some point.
pub type MockQuerierCustomHandlerResult = SystemResult<ContractResult<Binary>>;

/// MockQuerier holds an immutable table of bank balances
/// and configurable handlers for Wasm queries and custom queries.
pub struct MockQuerier<C: DeserializeOwned = Empty> {
    bank: BankQuerier,
    
    staking: StakingQuerier,
    wasm: WasmQuerier,
    /// A handler to handle custom queries. This is set to a dummy handler that
    /// always errors by default. Update it via `with_custom_handler`.
    ///
    /// Use box to avoid the need of another generic type
    custom_handler: Box<dyn for<'a> Fn(&'a C) -> MockQuerierCustomHandlerResult>,
}

impl<C: DeserializeOwned> MockQuerier<C> {
    pub fn new(chain: ChainData, balances: &[(&str, &[Coin])]) -> Self {
        MockQuerier {
            bank: BankQuerier::new(chain, balances),
            
            staking: StakingQuerier::default(),
            wasm: WasmQuerier::default(),
            // strange argument notation suggested as a workaround here: https://github.com/rust-lang/rust/issues/41078#issuecomment-294296365
            custom_handler: Box::from(|_: &_| -> MockQuerierCustomHandlerResult {
                SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: "custom".to_string(),
                })
            }),
        }
    }

    // set a new balance for the given address and return the old balance
    pub fn update_balance(
        &mut self,
        addr: impl Into<String>,
        balance: Vec<Coin>,
    ) -> Option<Vec<Coin>> {
        self.bank.update_balance(addr, balance)
    }

    
    pub fn update_staking(
        &mut self,
        denom: &str,
        validators: &[Validator],
        delegations: &[FullDelegation],
    ) {
        self.staking = StakingQuerier::new(denom, validators, delegations);
    }

    pub fn update_wasm<WH: 'static>(&mut self, handler: WH)
    where
        WH: Fn(&WasmQuery) -> QuerierResult,
    {
        self.wasm.update_handler(handler)
    }

    pub fn with_custom_handler<CH: 'static>(mut self, handler: CH) -> Self
    where
        CH: Fn(&C) -> MockQuerierCustomHandlerResult,
    {
        self.custom_handler = Box::from(handler);
        self
    }
}

impl<C: CustomQuery + DeserializeOwned> Querier for MockQuerier<C> {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<C> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl<C: CustomQuery + DeserializeOwned> MockQuerier<C> {
    pub fn handle_query(&self, request: &QueryRequest<C>) -> QuerierResult {
        match &request {
            QueryRequest::Bank(bank_query) => self.bank.query(bank_query),
            QueryRequest::Custom(custom_query) => (*self.custom_handler)(custom_query),
            
            QueryRequest::Staking(staking_query) => self.staking.query(staking_query),
            QueryRequest::Wasm(msg) => self.wasm.query(msg),
            QueryRequest::Stargate { .. } => SystemResult::Err(SystemError::UnsupportedRequest {
                kind: "Stargate".to_string(),
            }),
            &_ => panic!("Query Type Not implemented")
        }
    }
}

struct WasmQuerier {
    /// A handler to handle Wasm queries. This is set to a dummy handler that
    /// always errors by default. Update it via `with_custom_handler`.
    ///
    /// Use box to avoid the need of generic type.
    handler: Box<dyn for<'a> Fn(&'a WasmQuery) -> QuerierResult>,
}

impl WasmQuerier {
    fn new(handler: Box<dyn for<'a> Fn(&'a WasmQuery) -> QuerierResult>) -> Self {
        Self { handler }
    }

    fn update_handler<WH: 'static>(&mut self, handler: WH)
    where
        WH: Fn(&WasmQuery) -> QuerierResult,
    {
        self.handler = Box::from(handler)
    }

    fn query(&self, request: &WasmQuery) -> QuerierResult {
        (*self.handler)(request)
    }
}

impl Default for WasmQuerier {
    fn default() -> Self {
        let handler = Box::from(|request: &WasmQuery| -> QuerierResult {
            let err = match request {
                WasmQuery::Smart { contract_addr, .. } => SystemError::NoSuchContract {
                    addr: contract_addr.clone(),
                },
                WasmQuery::Raw { contract_addr, .. } => SystemError::NoSuchContract {
                    addr: contract_addr.clone(),
                },
                WasmQuery::ContractInfo { contract_addr, .. } => SystemError::NoSuchContract {
                    addr: contract_addr.clone(),
                },
                &_ => panic!("Not implemented {:?}", request)
            };
            SystemResult::Err(err)
        });
        Self::new(handler)
    }
}

#[derive(Clone)]
pub struct BankQuerier {
    #[allow(dead_code)]
    /// HashMap<denom, amount>
    supplies: HashMap<String, Uint128>,
    /// HashMap<address, coins>
    balances: HashMap<String, Vec<Coin>>,
    channel: Channel,
}

impl BankQuerier {
    pub fn new(chain: ChainData, balances: &[(&str, &[Coin])]) -> Self {
        let balances: HashMap<_, _> = balances
            .iter()
            .map(|(s, c)| (s.to_string(), c.to_vec()))
            .collect();

        let rt = Runtime::new().unwrap();

        BankQuerier {
            supplies: Self::calculate_supplies(&balances),
            balances,
            channel: rt.block_on(GrpcChannel::connect(&chain.apis.grpc, &chain.chain_id)).unwrap()
        }
    }

    pub fn update_balance(
        &mut self,
        addr: impl Into<String>,
        balance: Vec<Coin>,
    ) -> Option<Vec<Coin>> {
        let result = self.balances.insert(addr.into(), balance);
        self.supplies = Self::calculate_supplies(&self.balances);

        result
    }

    fn calculate_supplies(balances: &HashMap<String, Vec<Coin>>) -> HashMap<String, Uint128> {
        let mut supplies = HashMap::new();

        let all_coins = balances.iter().flat_map(|(_, coins)| coins);

        for coin in all_coins {
            *supplies
                .entry(coin.denom.clone())
                .or_insert_with(Uint128::zero) += coin.amount;
        }

        supplies
    }

    pub fn query(&self, request: &BankQuery) -> QuerierResult {

        let rt = Runtime::new().unwrap();
        let querier = Bank::new(self.channel.clone());
        let contract_result: ContractResult<Binary> = match request {
            BankQuery::Balance { address, denom } => {
                // proper error on not found, serialize result on found
                let mut amount = self
                    .balances
                    .get(address)
                    .and_then(|v| v.iter().find(|c| &c.denom == denom).map(|c| c.amount));

                // If the amount is not available, we query it from the distant chain
                if amount.is_none(){
                	let query_result = 
                        rt
                        .block_on(querier.balance(address, Some(denom.clone())))
                        .map(|result| {
                            Uint128::from_str(&result[0].amount).unwrap()
                        });

                    if let Ok(distant_amount) = query_result{
                    	amount = Some(distant_amount)
                    }
                }

                let bank_res = BalanceResponse {
                    amount: Coin {
                        amount: amount.unwrap(),
                        denom: denom.to_string(),
                    },
                };
                to_binary(&bank_res).into()
            }
            BankQuery::AllBalances { address } => {
                // proper error on not found, serialize result on found
                let mut amount = self.balances.get(address).cloned();

                // We query only if the bank balance doesn't exist
                if amount.is_none(){
                	let query_result: Result<Vec<Coin>, _> = rt
                            .block_on(querier.balance(address, None))
                            .map(|result| result
                                    .into_iter()
                                    .map(|c| Coin {
                                        amount: Uint128::from_str(&c.amount).unwrap(),
                                        denom: c.denom,
                                    })
                                    .collect()
                            );
                    if let Ok(distant_amount) = query_result{
                    	amount = Some(distant_amount)
                    }
                }

                let bank_res = AllBalanceResponse {
                    amount: amount.unwrap()
                };
                to_binary(&bank_res).into()
            },
            &_ => panic!("Not implemented {:?}", request)
        };
        // system result is always ok in the mock implementation
        SystemResult::Ok(contract_result)
    }
}

#[derive(Clone, Default)]
pub struct StakingQuerier {
    denom: String,
    validators: Vec<Validator>,
    delegations: Vec<FullDelegation>,
}

impl StakingQuerier {
    pub fn new(denom: &str, validators: &[Validator], delegations: &[FullDelegation]) -> Self {
        StakingQuerier {
            denom: denom.to_string(),
            validators: validators.to_vec(),
            delegations: delegations.to_vec(),
        }
    }

    pub fn query(&self, request: &StakingQuery) -> QuerierResult {
        let contract_result: ContractResult<Binary> = match request {
            StakingQuery::BondedDenom {} => {
                let res = BondedDenomResponse {
                    denom: self.denom.clone(),
                };
                to_binary(&res).into()
            }
            StakingQuery::AllValidators {} => {
                let res = AllValidatorsResponse {
                    validators: self.validators.clone(),
                };
                to_binary(&res).into()
            }
            StakingQuery::Validator { address } => {
                let validator: Option<Validator> = self
                    .validators
                    .iter()
                    .find(|validator| validator.address == *address)
                    .cloned();
                let res = ValidatorResponse { validator };
                to_binary(&res).into()
            }
            StakingQuery::AllDelegations { delegator } => {
                let delegations: Vec<_> = self
                    .delegations
                    .iter()
                    .filter(|d| d.delegator.as_str() == delegator)
                    .cloned()
                    .map(|d| d.into())
                    .collect();
                let res = AllDelegationsResponse { delegations };
                to_binary(&res).into()
            }
            StakingQuery::Delegation {
                delegator,
                validator,
            } => {
                let delegation = self
                    .delegations
                    .iter()
                    .find(|d| d.delegator.as_str() == delegator && d.validator == *validator);
                let res = DelegationResponse {
                    delegation: delegation.cloned(),
                };
                to_binary(&res).into()
            },
             &_ => panic!("Not implemented {:?}", request)
        };
        // system result is always ok in the mock implementation
        SystemResult::Ok(contract_result)
    }
}

pub fn digit_sum(input: &[u8]) -> usize {
    input.iter().fold(0, |sum, val| sum + (*val as usize))
}

/// Only for test code. This bypasses assertions in new, allowing us to create _*
/// Attributes to simulate responses from the blockchain
pub fn mock_wasmd_attr(key: impl Into<String>, value: impl Into<String>) -> Attribute {
    Attribute {
        key: key.into(),
        value: value.into(),
    }
}