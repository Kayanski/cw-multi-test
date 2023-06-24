use std::collections::HashMap;
use crate::wasm_emulation::wasm::ContractData;
use crate::wasm_emulation::bank::STARGATE_ALL_BANK_QUERY_URL;
use cosmwasm_std::Addr;
use cosmwasm_std::Binary;
use cosmwasm_std::CustomQuery;
use cosmwasm_std::Deps;
use cosmwasm_std::QueryRequest;

use cw_orch::daemon::ChainInfo;
use cw_utils::NativeBalance;


use cosmwasm_std::{Env, MessageInfo, Reply};
use ibc_chain_registry::chain::Apis;
use ibc_chain_registry::chain::ChainData;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use serde::{Serialize, Deserialize};


use crate::wasm_emulation::wasm::STARGATE_ALL_WASM_QUERY_URL;

use super::contract::WasmContract;

use anyhow::Result as AnyResult;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerChainData{
    pub chain_id: ChainId,
    pub apis: Apis,
}

impl From<ChainData> for SerChainData{
	fn from(c: ChainData) -> SerChainData{
		Self{
			chain_id: c.chain_id,
			apis: c.apis
		}
	}
}

impl From<ChainInfo<'_>> for SerChainData{
	fn from(c: ChainInfo) -> SerChainData{
		let data: ChainData = c.into();
		data.into()
	}
}


#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WasmStorage{
	pub contracts: HashMap<String, ContractData>,
	pub codes: HashMap<usize, WasmContract>,
	pub storage: Vec<(Vec<u8>, Vec<u8>)>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct BankStorage{
	pub storage: Vec<(Addr, NativeBalance)>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct QuerierStorage{
    pub wasm: WasmStorage,
    pub bank: Vec<(Addr, NativeBalance)>,
}

pub fn get_querier_storage<QueryC: CustomQuery>(deps: &Deps<QueryC>) -> AnyResult<QuerierStorage>{
    // We get the wasm storage for all wasm contract to make sure we dispatch everything (with the mock Querier)
    let wasm: WasmStorage = deps.querier.query(&QueryRequest::Stargate { path: STARGATE_ALL_WASM_QUERY_URL.to_string(), data: Binary(vec![]) })?;
    let bank = deps.querier.query(&QueryRequest::Stargate { path: STARGATE_ALL_BANK_QUERY_URL.to_string(), data: Binary(vec![]) })?;
    // log::info!("All local contract storage : {:?}", wasm.storage);
    // log::info!("All local bank storage : {:?}", bank);
    Ok(QuerierStorage{
        wasm,
        bank
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InstanceArguments{
	pub function: WasmFunction,
	pub init_storage: Vec<(Vec<u8>, Vec<u8>)>,
	pub querier_storage: QuerierStorage
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WasmFunction{
	Execute(ExecuteArgs),
	Instantiate(InstantiateArgs),
	Query(QueryArgs),
	Sudo(SudoArgs),
	Reply(ReplyArgs),
	Migrate(MigrateArgs),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteArgs{
	pub env: Env,
	pub info: MessageInfo,
	pub msg: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InstantiateArgs{
	pub env: Env,
	pub info: MessageInfo,
	pub msg: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QueryArgs{
	pub env: Env,
	pub msg: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SudoArgs{
	pub env: Env,
	pub msg: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReplyArgs{
	pub env: Env,
	pub reply: Reply
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MigrateArgs{
	pub env: Env,
	pub msg: Vec<u8>
}

impl WasmFunction{
	pub fn get_address(&self) -> Addr{
		match self{
			WasmFunction::Execute(ExecuteArgs { env, .. }) => env.contract.address.clone(),
			WasmFunction::Instantiate(InstantiateArgs { env, .. }) => env.contract.address.clone(),
			WasmFunction::Query(QueryArgs { env, .. }) => env.contract.address.clone(),
			WasmFunction::Reply(ReplyArgs { env, .. }) => env.contract.address.clone(),
			WasmFunction::Sudo(SudoArgs { env, .. }) => env.contract.address.clone(),
			WasmFunction::Migrate(MigrateArgs { env, .. }) => env.contract.address.clone(),
		}
	}
}

