use crate::wasm_emulation::input::IsolatedChainData;
use cw_orch::prelude::queriers::DaemonQuerier;
use cw_orch::prelude::queriers::CosmWasm;
use schemars::JsonSchema;
use cosmwasm_std::Empty;
use cosmwasm_std::Order;
use cosmwasm_std::Storage;

use cosmwasm_std::from_binary;


use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::process::Command;
use crate::wasm_emulation::input::InstanceArguments;
use crate::wasm_emulation::output::WasmRunnerOutput;
use cosmwasm_std::to_binary;


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

pub fn run_contract<ExecC: DeserializeOwned>(args: InstanceArguments) -> AnyResult<WasmRunnerOutput<ExecC>>{

    let serialized_args = to_binary(&args).unwrap().to_base64();

    let result = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--bin")
        .arg("wasm_runner")
        .arg(serialized_args)
        .output();


    let stdout = String::from_utf8_lossy(&result.as_ref().unwrap().stdout).to_string();
    let binary_stdout = Binary::from_base64(&stdout).map(|s| from_binary(&s));
    if binary_stdout.is_err() || binary_stdout.as_ref().unwrap().is_err(){
        panic!("Err when calling contract, {:?}", result)
    }
    let decoded_result: WasmRunnerOutput<ExecC> = binary_stdout??;

    Ok(decoded_result)
}

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistantContract{
    pub contract_addr: String,
    pub chain: IsolatedChainData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistantCodeId{
    pub code_id: u64,
    pub chain: IsolatedChainData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalContract{
    pub code: Vec<u8>,
    pub chain: IsolatedChainData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WasmContract{
    Local(LocalContract),
    DistantContract(DistantContract),
    DistantCodeId(DistantCodeId)
}

impl WasmContract{
    pub fn new_local(code: Vec<u8>, chain: IsolatedChainData) -> Self{
        check_wasm(&code, &HashSet::default()).unwrap();
        Self::Local(LocalContract { code, chain })

    }

    pub fn new_distant_contract(contract_addr: String, chain: IsolatedChainData) -> Self{
        Self::DistantContract(DistantContract{
            contract_addr,
            chain
        })
    }

    pub fn new_distant_code_id(code_id: u64, chain: IsolatedChainData) -> Self{
        Self::DistantCodeId(DistantCodeId{
            code_id,
            chain
        })

    }

    pub fn get_chain(&self) -> IsolatedChainData{
        match self{
            WasmContract::Local(LocalContract { chain, .. }) => chain.clone(),
            WasmContract::DistantContract(DistantContract { chain, .. }) => chain.clone(),
            WasmContract::DistantCodeId(DistantCodeId { chain, .. }) => chain.clone(),
        }
    }

    pub fn get_code(self) -> AnyResult<Vec<u8>>{
        match self{
            WasmContract::Local(LocalContract { code, .. }) => Ok(code),
            WasmContract::DistantContract(DistantContract { chain, contract_addr }) => {
                let (rt, channel) = get_channel(chain)?;
                let wasm_querier = CosmWasm::new(channel);

                let code_info = rt.block_on(wasm_querier.contract_info(contract_addr))?;
                let code = rt.block_on(wasm_querier.code_data(code_info.code_id))?;
                Ok(code)
            }
            WasmContract::DistantCodeId(DistantCodeId { chain,code_id }) => {
                let (rt, channel) = get_channel(chain)?;
                let wasm_querier = CosmWasm::new(channel);

                let code = rt.block_on(wasm_querier.code_data(code_id))?;
                Ok(code)
            }
        }
    }
}

impl<ExecC, QueryC> Contract<ExecC, QueryC> for WasmContract
where
    ExecC: Clone + std::fmt::Debug + PartialEq + JsonSchema + DeserializeOwned,
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
            contract:self.clone(),
            function: WasmFunction::Execute(ExecuteArgs{
                env,
                info,
                msg,
            }),
            init_storage: deps.storage.range(None, None, Order::Ascending).collect()
        };

        let decoded_result = run_contract(execute_args)?;

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
            contract:self.clone(),
            function: WasmFunction::Instantiate(InstantiateArgs{
                env,
                info,
                msg,
            }),
            init_storage: deps.storage.range(None, None, Order::Ascending).collect()
        };

        let decoded_result = run_contract(instantiate_arguments)?;

        apply_storage_changes(deps.storage, &decoded_result);

        match decoded_result.wasm{
            WasmOutput::Instantiate(x)=> Ok(x),
            _ => panic!("Wrong kind of answer from wasm container")
        }
    }

    fn query(&self, deps: Deps<QueryC>, env: Env, msg: Vec<u8>) -> AnyResult<Binary> {
        // We start by building the dependencies we will pass through the wasm executer
        let query_arguments = InstanceArguments{
            contract:self.clone(),
            function: WasmFunction::Query(QueryArgs{
                env,
                msg,
            }),
            init_storage: deps.storage.range(None, None, Order::Ascending).collect()
        };

        let decoded_result: WasmRunnerOutput<Empty> = run_contract(query_arguments)?;

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
