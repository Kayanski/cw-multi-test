pub mod mock_querier;
pub mod bank;
pub mod wasm;
pub mod staking;
use serde::{de::DeserializeOwned};
use cosmwasm_std::Storage;

pub use mock_querier::MockQuerier;
pub mod gas;

use anyhow::Result as AnyResult;
use serde::Serialize;

pub trait AllQuerier{
	type Output: Serialize + Clone + DeserializeOwned + Default;
	fn query_all(&self, storage: &dyn Storage,) -> AnyResult<Self::Output>;
}