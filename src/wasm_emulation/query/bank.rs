use crate::wasm_emulation::input::SerChainData;
use cosmwasm_std::Addr;
use std::str::FromStr;


use cw_orch::prelude::queriers::DaemonQuerier;
use cw_utils::NativeBalance;

use tonic::transport::Channel;
use cw_orch::prelude::queriers::Bank;
use tokio::runtime::Runtime;

use std::collections::HashMap;
use cosmwasm_std::Binary;
use cosmwasm_std::Coin;


use cosmwasm_std::Uint128;
use cosmwasm_std::{
    AllBalanceResponse, BalanceResponse, BankQuery,
};

use cosmwasm_std::{ContractResult, SystemResult};
use cosmwasm_std::{to_binary};

use cosmwasm_std::{QuerierResult};


use crate::wasm_emulation::channel::get_channel;




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
    pub fn new(chain: impl Into<SerChainData>, init: Option<Vec<(Addr, NativeBalance)>>) -> Self {
        let chain = chain.into();
        let balances: HashMap<_, _> = init.unwrap_or(vec![])
            .iter()
            .map(|(s, c)| (s.to_string(), c.clone().into_vec()))
            .collect();

        let (_rt, channel) = get_channel(chain).unwrap();

        BankQuerier {
            supplies: Self::calculate_supplies(&balances),
            balances,
            channel
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