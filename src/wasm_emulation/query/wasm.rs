
use crate::wasm_emulation::input::SerChainData;
use crate::wasm_emulation::input::QuerierStorage;
use crate::wasm_emulation::input::WasmFunction;
use crate::wasm_emulation::output::WasmOutput;
use crate::wasm_emulation::output::WasmRunnerOutput;
use cosmwasm_std::testing::mock_env;
use cw_orch::prelude::queriers::DaemonQuerier;
use crate::wasm_emulation::contract::WasmContract;
use crate::wasm_emulation::wasm::NAMESPACE_WASM;
use cosmwasm_std::{Empty, ContractResult};
use cosmwasm_std::{Addr, SystemResult, to_binary, ContractInfoResponse};
use cw_orch::prelude::queriers::CosmWasm;


use cosmwasm_std::{
    WasmQuery,
};

use cosmwasm_std::{QuerierResult};


use crate::WasmKeeper;
use crate::wasm_emulation::channel::get_channel;

use crate::wasm_emulation::input::{WasmStorage, InstanceArguments, QueryArgs};


pub struct WasmQuerier {
    chain: SerChainData,
    current_storage: QuerierStorage,
}

impl WasmQuerier {
    pub fn new(chain: impl Into<SerChainData>, storage: Option<QuerierStorage>) -> Self {
        let chain = chain.into();
        Self { 
            chain,
            current_storage: storage.unwrap_or(Default::default())
        }
    }


    fn get_contract_storage_namespace(address: &Addr) -> Vec<u8>{
        let mut namespace = b"contract_data/".to_vec();
        namespace.extend_from_slice(address.as_bytes());
        to_length_prefixed_nested(&[NAMESPACE_WASM, &namespace])
    }


    pub fn query(&self, request: &WasmQuery) -> QuerierResult {

        match request{
            WasmQuery::ContractInfo { contract_addr } =>{
                let addr = Addr::unchecked(contract_addr);
                let data = if let Some(local_contract) = self.current_storage.wasm.contracts.get(contract_addr){
                    local_contract.clone()
                }else{
                    WasmKeeper::<Empty, Empty>::load_distant_contract(self.chain.clone(), &addr).unwrap()
                };
                let mut response = ContractInfoResponse::default();
                response.code_id = data.code_id.try_into().unwrap();
                response.creator = data.creator.to_string();
                response.admin = data.admin.map(|a| a.to_string());
                SystemResult::Ok(to_binary(&response).into())
            },
            WasmQuery::Raw { contract_addr, key } => {
                // We first try to load that information locally
                let mut total_key = Self::get_contract_storage_namespace(&Addr::unchecked(contract_addr)).to_vec();
                total_key.extend_from_slice(key);

                let value: Vec<u8> = if let Some(value) = self.current_storage.wasm.storage.iter().find(|e| e.0 == total_key){
                    value.1.clone()
                }else{

                    let (rt, channel) = get_channel(self.chain.clone()).unwrap();
                    let wasm_querier = CosmWasm::new(channel);
                    let query_result = rt
                        .block_on(
                            wasm_querier.contract_raw_state(contract_addr.to_string(), key.to_vec()),
                        )
                        .map(|query_result| query_result.data);
                    query_result   .unwrap()
                };

                SystemResult::Ok(ContractResult::Ok(value.into()))
            },
            WasmQuery::Smart { contract_addr, msg } => {
                let addr = Addr::unchecked(contract_addr);
                // If the contract is already defined in our storage, we load it from there
                let contract = if let Some(local_contract) = self.current_storage.wasm.contracts.get(contract_addr){
                    if let Some(code_info) = self.current_storage.wasm.codes.get(&local_contract.code_id){
                        // We execute the query
                        code_info.clone()
                    }else{
                        WasmContract::new_distant_code_id(local_contract.code_id.try_into().unwrap(), self.chain.clone())
                    }
                }else{
                    WasmContract::new_distant_contract(contract_addr.to_string(), self.chain.clone())
                };

                let mut env = mock_env();
                env.contract.address = addr.clone();
                // Here we specify empty because we only car about the query result
                let result: WasmRunnerOutput<Empty> = contract.run_contract(InstanceArguments{
                    function: WasmFunction::Query(QueryArgs{
                        env,
                        msg: msg.to_vec()
                    }),
                    querier_storage: QuerierStorage{
                        wasm: self.current_storage.wasm.clone(),
                        bank: self.current_storage.bank.clone(),
                    },
                    init_storage: get_contract_storage(&self.current_storage.wasm, &addr)
                }).unwrap();

                let bin = match result.wasm{
                    WasmOutput::Query(bin) => bin,
                    _ => panic!("Unexpected contract response, not possible")
                };

                SystemResult::Ok(ContractResult::Ok(bin))
            }
            _ => unimplemented!()
        }
    }
}


fn get_contract_storage(storage: &WasmStorage, contract_addr: &Addr) -> Vec<(Vec<u8>, Vec<u8>)>{

    let namespace = WasmQuerier::get_contract_storage_namespace(&Addr::unchecked(contract_addr)).to_vec();
    let namespace_len = namespace.len();
    let keys: Vec<(Vec<u8>, Vec<u8>)> = storage.storage
        .iter()
        // We filter only value in this namespace
        .filter(|(k, _)|  k.len() >= namespace_len && k[..namespace_len] == namespace).cloned()
        // We remove the namespace prefix from the key
        .map(|(k, value)| (k[namespace_len..].to_vec(), value)).collect();

    keys
}

/// Calculates the raw key prefix for a given nested namespace
/// as documented in https://github.com/webmaster128/key-namespacing#nesting
pub fn to_length_prefixed_nested(namespaces: &[&[u8]]) -> Vec<u8> {
    let mut size = 0;
    for &namespace in namespaces {
        size += namespace.len() + 2;
    }

    let mut out = Vec::with_capacity(size);
    for &namespace in namespaces {
        out.extend_from_slice(&encode_length(namespace));
        out.extend_from_slice(namespace);
    }
    out
}


/// Encodes the length of a given namespace as a 2 byte big endian encoded integer
fn encode_length(namespace: &[u8]) -> [u8; 2] {
    if namespace.len() > 0xFFFF {
        panic!("only supports namespaces up to length 0xFFFF")
    }
    let length_bytes = (namespace.len() as u32).to_be_bytes();
    [length_bytes[2], length_bytes[3]]
}