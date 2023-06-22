use cosmwasm_std::{Response, Binary};
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Debug)]
pub enum WasmOutput<T>{
	Execute(Response<T>),
	Instantiate(Response<T>),
	Query(Binary),
	Sudo(Response<T>),
	Reply(Response<T>),
	Migrate(Response<T>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StorageChanges{
	pub current_keys: Vec<(Vec<u8>, Vec<u8>)>,
	pub removed_keys: Vec<Vec<u8>>
}


#[derive(Serialize, Deserialize, Debug)]
pub struct WasmRunnerOutput<T>{
	pub wasm: WasmOutput<T>,
	pub storage: StorageChanges
}