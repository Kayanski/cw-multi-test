
use crate::wasm_emulation::input::SerChainData;
use crate::wasm_emulation::runner::execute_function;
use cosmwasm_vm::Size;
use cosmwasm_vm::InstanceOptions;
use cosmwasm_vm::Instance;
use cosmwasm_vm::Backend;
use cosmwasm_vm::testing::MockApi;
use crate::wasm_emulation::storage::DualStorage;
use crate::wasm_emulation::query::MockQuerier;
use crate::wasm_emulation::output::StorageChanges;
use crate::wasm_emulation::input::get_querier_storage;
use cosmwasm_std::CustomMsg;
use cw_orch::prelude::queriers::DaemonQuerier;
use cw_orch::prelude::queriers::CosmWasm;

use cosmwasm_std::Empty;
use cosmwasm_std::Order;
use cosmwasm_std::Storage;


use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::wasm_emulation::input::InstanceArguments;
use crate::wasm_emulation::output::WasmRunnerOutput;



use std::collections::HashSet;
use cosmwasm_vm::internals::check_wasm;

use crate::Contract;

use cosmwasm_std::{
    Binary, CustomQuery, Deps, DepsMut, Env, MessageInfo, Reply, Response,
};

use anyhow::{Result as AnyResult};

use super::channel::get_channel;
use super::input::ExecuteArgs;
use super::input::InstantiateArgs;
use super::input::QueryArgs;
use super::input::WasmFunction;
use super::output::WasmOutput;

fn apply_storage_changes<ExecC>(storage: &mut dyn Storage, output: &WasmRunnerOutput<ExecC>){

    // We change all the values with the output
    for (key, value) in &output.storage.current_keys{
        storage.set(key, value);
    }

    // We remove all values that need to be removed from it
    for key in &output.storage.removed_keys{
        storage.remove(key);
    }
} 

/// Taken from cosmwasm_vm::testing
/// This gas limit is used in integration tests and should be high enough to allow a reasonable
/// number of contract executions and queries on one instance. For this reason it is significatly
/// higher than the limit for a single execution that we have in the production setup.
const DEFAULT_GAS_LIMIT: u64 = 500_000_000_000; // ~0.5ms
const DEFAULT_MEMORY_LIMIT: Option<Size> = Some(Size::mebi(16));
const DEFAULT_PRINT_DEBUG: bool = true;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistantContract{
    pub contract_addr: String,
    pub chain: SerChainData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistantCodeId{
    pub code_id: u64,
    pub chain: SerChainData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalContract{
    pub code: Vec<u8>,
    pub chain: SerChainData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WasmContract{
    Local(LocalContract),
    DistantContract(DistantContract),
    DistantCodeId(DistantCodeId),
}

impl WasmContract{
    pub fn new_local(code: Vec<u8>, chain: impl Into<SerChainData>) -> Self{
        check_wasm(&code, &HashSet::from(["iterator".to_string()])).unwrap();
        Self::Local(LocalContract { code, chain: chain.into() })
    }

    pub fn new_distant_contract(contract_addr: String, chain: impl Into<SerChainData>) -> Self{
        Self::DistantContract(DistantContract{
            contract_addr,
            chain: chain.into()
        })
    }

    pub fn new_distant_code_id(code_id: u64, chain: impl Into<SerChainData>) -> Self{
        Self::DistantCodeId(DistantCodeId{
            code_id,
            chain: chain.into()
        })

    }

    pub fn get_chain(&self) -> SerChainData{
        match self{
            WasmContract::Local(LocalContract { chain, .. }) => chain.clone(),
            WasmContract::DistantContract(DistantContract { chain, .. }) => chain.clone(),
            WasmContract::DistantCodeId(DistantCodeId { chain, .. }) => chain.clone(),
        }
    }

    pub fn get_code(&self) -> AnyResult<Vec<u8>>{
        match self{
            WasmContract::Local(LocalContract { code, .. }) => Ok(code.clone()),
            WasmContract::DistantContract(DistantContract { chain, contract_addr }) => {
                let (rt, channel) = get_channel(chain.clone())?;
                let wasm_querier = CosmWasm::new(channel);

                let code_info = rt.block_on(wasm_querier.contract_info(contract_addr))?;
                let code = rt.block_on(wasm_querier.code_data(code_info.code_id))?;
                Ok(code)
            }
            WasmContract::DistantCodeId(DistantCodeId { chain,code_id }) => {
                let (rt, channel) = get_channel(chain.clone())?;
                let wasm_querier = CosmWasm::new(channel);

                let code = rt.block_on(wasm_querier.code_data(*code_id))?;
                Ok(code)
            }
        }
    }

    pub fn run_contract<ExecC: CustomMsg + DeserializeOwned>(&self, args: InstanceArguments) -> AnyResult<WasmRunnerOutput<ExecC>>{

        let InstanceArguments {function, init_storage, querier_storage } = args;
        let chain = self.get_chain();
        let address = function.get_address();
        let code = self.get_code()?;

        log::debug!("Calling local contract, address: {}, chain : {:?}", address, chain);


        // We create the backend here from outside information;
        let backend = Backend {
            api: MockApi::default(), // TODO need to change this to validate the addresses ?
            storage: DualStorage::new(chain.clone(), address.to_string(), Some(init_storage))?,
            querier: MockQuerier::<Empty>::new(chain, Some(querier_storage)) 
        };
        let options = InstanceOptions {
            gas_limit: DEFAULT_GAS_LIMIT,
            print_debug: DEFAULT_PRINT_DEBUG,
        };
        let memory_limit = DEFAULT_MEMORY_LIMIT;

        // Then we create the instance
        let mut instance = Instance::from_code(&code, backend, options, memory_limit)?;

        // Then we call the function that we wanted to call
        let result = execute_function(&mut instance, function)?;

        // We return the code response + any storage change (or the whole local storage object), with serializing
        let mut recycled_instance = instance.recycle().unwrap();

        let wasm_result = WasmRunnerOutput{
            storage: StorageChanges{
                current_keys: recycled_instance.storage.get_all_storage()?,
                removed_keys: recycled_instance.storage.removed_keys.into_iter().collect(),
            },
            wasm: result,
        };

        Ok(wasm_result)
    }
}



impl<ExecC, QueryC> Contract<ExecC, QueryC> for WasmContract
where
    ExecC: CustomMsg + DeserializeOwned,
    QueryC: CustomQuery,
{
    fn execute(
        &self,
        deps: DepsMut<QueryC>,
        env: Env,
        info: MessageInfo,
        msg: Vec<u8>,
    ) -> AnyResult<Response<ExecC>> {

        // We start by building the dependencies we will pass through the wasm executer
        let execute_args = InstanceArguments{
            function: WasmFunction::Execute(ExecuteArgs{
                env,
                info,
                msg,
            }),
            init_storage: deps.storage.range(None, None, Order::Ascending).collect(),
            querier_storage: get_querier_storage(&deps.as_ref())?,
        };

        let decoded_result = self.run_contract(execute_args)?;

        apply_storage_changes(deps.storage, &decoded_result);

        match decoded_result.wasm{
            WasmOutput::Execute(x)=> Ok(x),
            _ => panic!("Wrong kind of answer from wasm container")
        }

    }

    fn instantiate(
        &self,
        deps: DepsMut<QueryC>,
        env: Env,
        info: MessageInfo,
        msg: Vec<u8>,
    ) -> AnyResult<Response<ExecC>> {
        // We start by building the dependencies we will pass through the wasm executer
        let instantiate_arguments = InstanceArguments{
            function: WasmFunction::Instantiate(InstantiateArgs{
                env,
                info,
                msg,
            }),
            init_storage: deps.storage.range(None, None, Order::Ascending).collect(),
            querier_storage: get_querier_storage(&deps.as_ref())?,
        };

        let decoded_result = self.run_contract(instantiate_arguments)?;

        apply_storage_changes(deps.storage, &decoded_result);

        match decoded_result.wasm{
            WasmOutput::Instantiate(x)=> Ok(x),
            _ => panic!("Wrong kind of answer from wasm container")
        }
    }

    fn query(&self, deps: Deps<QueryC>, env: Env, msg: Vec<u8>) -> AnyResult<Binary> {
        // We start by building the dependencies we will pass through the wasm executer
        let query_arguments = InstanceArguments{
            function: WasmFunction::Query(QueryArgs{
                env,
                msg,
            }),
            init_storage: deps.storage.range(None, None, Order::Ascending).collect(),
            querier_storage: get_querier_storage(&deps)?,
        };

        let decoded_result: WasmRunnerOutput<Empty> = self.run_contract(query_arguments)?;

        match decoded_result.wasm{
            WasmOutput::Query(x)=> Ok(x),
            _ => panic!("Wrong kind of answer from wasm container")
        }
    }

    // this returns an error if the contract doesn't implement sudo
    fn sudo(&self, _deps: DepsMut<QueryC>, _env: Env, _msg: Vec<u8>) -> AnyResult<Response<ExecC>> {

        panic!("Not Implemted")
    }

    // this returns an error if the contract doesn't implement reply
    fn reply(&self, _deps: DepsMut<QueryC>, _env: Env, _reply_data: Reply) -> AnyResult<Response<ExecC>> {
        panic!("Not Implemted")
    }

    // this returns an error if the contract doesn't implement migrate
    fn migrate(&self, _deps: DepsMut<QueryC>, _env: Env, _msg: Vec<u8>) -> AnyResult<Response<ExecC>> {
        panic!("Not Implemted")
    }
}
