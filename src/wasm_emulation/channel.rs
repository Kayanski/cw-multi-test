use tonic::transport::Channel;
use cw_orch::daemon::GrpcChannel;
use tokio::runtime::Runtime;
use ibc_chain_registry::chain::ChainData;
use anyhow::Result as AnyResult;


pub fn get_channel(chain: ChainData) -> AnyResult<(Runtime, Channel)>{
	let rt = Runtime::new()?;
	// We create an instance from a code_id, an address, and we run the code in it
	let channel = rt.block_on(GrpcChannel::connect(&chain.apis.grpc, &chain.chain_id))?;

	Ok((rt, channel))
}