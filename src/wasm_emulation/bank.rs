use std::str::FromStr;
use cosmwasm_std::{Uint128, Order};
use cw_orch::prelude::queriers::DaemonQuerier;

use ibc_chain_registry::chain::ChainData;



use anyhow::{bail, Result as AnyResult};
use itertools::Itertools;
use schemars::JsonSchema;

use cosmwasm_std::{
    coin, to_binary, Addr, AllBalanceResponse, Api, BalanceResponse, BankMsg, BankQuery, Binary,
    BlockInfo, Coin, Event, Querier, Storage,
};
use cw_storage_plus::Map;
use cw_utils::NativeBalance;

use crate::app::CosmosRouter;
use crate::executor::AppResponse;
use crate::module::Module;
use crate::prefixed_storage::{prefixed, prefixed_read};

use super::channel::get_channel;

const BALANCES: Map<&Addr, NativeBalance> = Map::new("balances");

pub const NAMESPACE_BANK: &[u8] = b"bank";

#[derive(Clone, std::fmt::Debug, PartialEq, Eq, JsonSchema)]
pub enum BankSudo {
    Mint {
        to_address: String,
        amount: Vec<Coin>,
    },
}
pub const STARGATE_ALL_BANK_QUERY_URL: &str = "/local.bank.all";

pub enum AccessibleBankQuery{
    BankQuery(BankQuery),
    AllQuery()
}

pub trait Bank: Module<ExecT = BankMsg, QueryT = AccessibleBankQuery, SudoT = BankSudo> {}

pub struct BankKeeper {
    chain: Option<ChainData>
}

impl Default for BankKeeper {
    fn default() -> Self {
        Self::new()
    }
}

impl BankKeeper {
    pub fn new() -> Self {
        BankKeeper{
            chain: None
        }
    }

    pub fn set_chain(&mut self, chain: ChainData){
       self.chain = Some(chain)
    }

    // this is an "admin" function to let us adjust bank accounts in genesis
    pub fn init_balance(
        &self,
        storage: &mut dyn Storage,
        account: &Addr,
        amount: Vec<Coin>,
    ) -> AnyResult<()> {
        let mut bank_storage = prefixed(storage, NAMESPACE_BANK);
        self.set_balance(&mut bank_storage, account, amount)
    }

    fn set_balance(
        &self,
        bank_storage: &mut dyn Storage,
        account: &Addr,
        amount: Vec<Coin>,
    ) -> AnyResult<()> {
        let mut balance = NativeBalance(amount);
        balance.normalize();
        BALANCES
            .save(bank_storage, account, &balance)
            .map_err(Into::into)
    }

    // this is an "admin" function to let us adjust bank accounts
    fn get_balance(&self, bank_storage: &dyn Storage, account: &Addr) -> AnyResult<Vec<Coin>> {
        // If there is no balance present, we query it on the distant chain
        if !BALANCES.has(bank_storage, account){
            let (rt, channel) = get_channel(self.chain.clone().unwrap())?;
            let querier = cw_orch::daemon::queriers::Bank::new(channel);
            let distant_amounts: Vec<Coin> = rt
                .block_on(querier.balance(account, None))
                .map(|result| result
                        .into_iter()
                        .map(|c| Coin {
                            amount: Uint128::from_str(&c.amount).unwrap(),
                            denom: c.denom,
                        })
                        .collect()
                )
                .unwrap();
            Ok(distant_amounts)
        }else{
            let val = BALANCES.may_load(bank_storage, account)?;
            Ok(val.unwrap_or_default().into_vec())
        }
    }

    fn send(
        &self,
        bank_storage: &mut dyn Storage,
        from_address: Addr,
        to_address: Addr,
        amount: Vec<Coin>,
    ) -> AnyResult<()> {
        self.burn(bank_storage, from_address, amount.clone())?;
        self.mint(bank_storage, to_address, amount)
    }

    fn mint(
        &self,
        bank_storage: &mut dyn Storage,
        to_address: Addr,
        amount: Vec<Coin>,
    ) -> AnyResult<()> {
        let amount = self.normalize_amount(amount)?;
        let b = self.get_balance(bank_storage, &to_address)?;
        let b = NativeBalance(b) + NativeBalance(amount);
        self.set_balance(bank_storage, &to_address, b.into_vec())
    }

    fn burn(
        &self,
        bank_storage: &mut dyn Storage,
        from_address: Addr,
        amount: Vec<Coin>,
    ) -> AnyResult<()> {
        let amount = self.normalize_amount(amount)?;
        let a = self.get_balance(bank_storage, &from_address)?;
        let a = (NativeBalance(a) - amount)?;
        self.set_balance(bank_storage, &from_address, a.into_vec())
    }

    /// Filters out all 0 value coins and returns an error if the resulting Vec is empty
    fn normalize_amount(&self, amount: Vec<Coin>) -> AnyResult<Vec<Coin>> {
        let res: Vec<_> = amount.into_iter().filter(|x| !x.amount.is_zero()).collect();
        if res.is_empty() {
            bail!("Cannot transfer empty coins amount")
        } else {
            Ok(res)
        }
    }
}

fn coins_to_string(coins: &[Coin]) -> String {
    coins
        .iter()
        .map(|c| format!("{}{}", c.amount, c.denom))
        .join(",")
}

impl Bank for BankKeeper {}

impl Module for BankKeeper {
    type ExecT = BankMsg;
    type QueryT = AccessibleBankQuery;
    type SudoT = BankSudo;

    fn execute<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        sender: Addr,
        msg: BankMsg,
    ) -> AnyResult<AppResponse> {
        let mut bank_storage = prefixed(storage, NAMESPACE_BANK);
        match msg {
            BankMsg::Send { to_address, amount } => {
                // see https://github.com/cosmos/cosmos-sdk/blob/v0.42.7/x/bank/keeper/send.go#L142-L147
                let events = vec![Event::new("transfer")
                    .add_attribute("recipient", &to_address)
                    .add_attribute("sender", &sender)
                    .add_attribute("amount", coins_to_string(&amount))];
                self.send(
                    &mut bank_storage,
                    sender,
                    Addr::unchecked(to_address),
                    amount,
                )?;
                Ok(AppResponse { events, data: None })
            }
            BankMsg::Burn { amount } => {
                // burn doesn't seem to emit any events
                self.burn(&mut bank_storage, sender, amount)?;
                Ok(AppResponse::default())
            }
            m => bail!("Unsupported bank message: {:?}", m),
        }
    }

    fn sudo<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        msg: BankSudo,
    ) -> AnyResult<AppResponse> {
        let mut bank_storage = prefixed(storage, NAMESPACE_BANK);
        match msg {
            BankSudo::Mint { to_address, amount } => {
                let to_address = api.addr_validate(&to_address)?;
                self.mint(&mut bank_storage, to_address, amount)?;
                Ok(AppResponse::default())
            }
        }
    }

    fn query(
        &self,
        api: &dyn Api,
        storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: AccessibleBankQuery,
    ) -> AnyResult<Binary> {
        let bank_storage = prefixed_read(storage, NAMESPACE_BANK);
        match request {

            AccessibleBankQuery::AllQuery() => {
                let balances: Result<Vec<_>, _> = BALANCES
                    .range(&bank_storage, None, None, Order::Ascending)
                    .collect();
                Ok(to_binary(&balances?)?)
            },
            AccessibleBankQuery::BankQuery(request)=> {
                match request{
                    BankQuery::AllBalances { address } => {
                        let address = api.addr_validate(&address)?;
                        let amount = self.get_balance(&bank_storage, &address)?;
                        let res = AllBalanceResponse { amount };
                        Ok(to_binary(&res)?)
                    }
                    BankQuery::Balance { address, denom } => {
                        let address = api.addr_validate(&address)?;
                        let all_amounts = self.get_balance(&bank_storage, &address)?;
                        let amount = all_amounts
                            .into_iter()
                            .find(|c| c.denom == denom)
                            .unwrap_or_else(|| coin(0, denom));
                        let res = BalanceResponse { amount };
                        Ok(to_binary(&res)?)
                    }
                    q => bail!("Unsupported bank query: {:?}", q),
                }
            }
        }
    }
}