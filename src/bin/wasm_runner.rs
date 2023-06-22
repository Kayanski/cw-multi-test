
use cw_multi_test::wasm_emulation::output::StorageChanges;
use cw_multi_test::wasm_emulation::storage::DualStorage;

use cosmwasm_std::Binary;
use cosmwasm_std::{to_binary, from_binary};
use cosmwasm_std::Empty;
use cosmwasm_vm::testing::MockQuerier;
use cosmwasm_vm::Querier;
use cosmwasm_vm::Storage;
use cosmwasm_vm::BackendApi;
use cosmwasm_vm::InstanceOptions;
use cosmwasm_vm::Backend;
use cosmwasm_vm::{Size, call_execute, call_query, call_instantiate, call_reply};
use cosmwasm_vm::testing::MockApi;
use cw_multi_test::wasm_emulation::output::WasmOutput;
use cw_multi_test::wasm_emulation::input::{WasmFunction, InstanceArguments};
use cw_multi_test::wasm_emulation::output::WasmRunnerOutput;
use cw_orch::prelude::queriers::DaemonQuerier;

use cw_orch::daemon::GrpcChannel;
use cw_orch::prelude::queriers::CosmWasm;

use tokio::runtime::Runtime;
use std::env;

use cosmwasm_vm::Instance;


use anyhow::Result as AnyResult;


/// Taken from cosmwasm_vm::testing
/// This gas limit is used in integration tests and should be high enough to allow a reasonable
/// number of contract executions and queries on one instance. For this reason it is significatly
/// higher than the limit for a single execution that we have in the production setup.
const DEFAULT_GAS_LIMIT: u64 = 500_000_000_000; // ~0.5ms
const DEFAULT_MEMORY_LIMIT: Option<Size> = Some(Size::mebi(16));
const DEFAULT_PRINT_DEBUG: bool = true;

pub fn main() -> AnyResult<()>{
	// Parsing arguments (serde serialized and base64 encoded, only 1 argument)
	let args: Vec<_> = env::args().collect();
    if args.len() <= 1 {
    	panic!("The argument must be of length 1 and valid base64");
    }

    let base64_arg = &args[1];
    let InstanceArguments {chain, address, function, init_storage } = from_binary(&Binary::from_base64(base64_arg)?)?;
    let rt = Runtime::new()?;
	// We create an instance from a code_id, an address, and we run the code in it
	let channel = rt.block_on(GrpcChannel::connect(&chain.apis.grpc, &chain.chain_id))?;
	let wasm_querier = CosmWasm::new(channel);

	let code_info = rt.block_on(wasm_querier.contract_info(address.clone()))?;
	let code = rt.block_on(wasm_querier.code_data(code_info.code_id))?;

	// We create the backend here from outside information;
	let backend = Backend {
        api: MockApi::default(), // TODO need to change this to validate the addresses ?
        storage: DualStorage::new(rt, chain, address, Some(init_storage))?,
        querier: MockQuerier::<Empty>::new(&[])
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

	let encoded_result = to_binary(&wasm_result)?.to_base64();
	print!("{}", encoded_result);

	Ok(())
}

fn execute_function<
	A: BackendApi + 'static, 
	B: Storage + 'static, 
	C: Querier + 'static
>
	(instance: &mut Instance<A,B,C>, function: WasmFunction) -> AnyResult<WasmOutput<Empty>>{

	match function{
		WasmFunction::Execute(args) => {
			let result = call_execute(instance, &args.env, &args.info, &args.msg)?.into_result().unwrap();
			Ok(WasmOutput::Execute(result))
		},
		WasmFunction::Query(args) => {
			let result = call_query(instance, &args.env, &args.msg)?.into_result().unwrap();
			Ok(WasmOutput::Query(result))
		},
		WasmFunction::Instantiate(args) => {
			let result = call_instantiate(instance, &args.env, &args.info, &args.msg)?.into_result().unwrap();
			Ok(WasmOutput::Instantiate(result))
		},
		WasmFunction::Reply(args) => {
			let result = call_reply(instance, &args.env, &args.reply)?.into_result().unwrap();
			Ok(WasmOutput::Reply(result))
		},
		_ => panic!("Not implemented")
	}
}

