use cosmwasm_std::{CustomMsg};


use cosmwasm_vm::Querier;
use cosmwasm_vm::Storage;
use cosmwasm_vm::BackendApi;


use cosmwasm_vm::{call_execute, call_query, call_instantiate, call_reply};

use serde::de::DeserializeOwned;
use crate::wasm_emulation::output::WasmOutput;
use crate::wasm_emulation::input::{WasmFunction};




use cosmwasm_vm::Instance;

use anyhow::Result as AnyResult;


pub fn execute_function<
	A: BackendApi + 'static, 
	B: Storage + 'static, 
	C: Querier + 'static,
	ExecC: CustomMsg + DeserializeOwned
>
	(instance: &mut Instance<A,B,C>, function: WasmFunction) -> AnyResult<WasmOutput<ExecC>>{

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

