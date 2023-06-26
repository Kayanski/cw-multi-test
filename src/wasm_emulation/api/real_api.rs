use bech32::{ToBase32, Variant};
use bech32::FromBase32;
use cosmwasm_vm::BackendError;
use cosmwasm_vm::GasInfo;
use cosmwasm_vm::BackendApi;

pub fn bytes_from_bech32(address: &str, prefix: &str) -> Result<Vec<u8>, BackendError>{

	if address.is_empty(){
		return Err(BackendError::Unknown { msg: "empty address string is not allowed".to_string() })
	}

	let (hrp, data, _variant) = bech32::decode(address)
		.map_err(|e| BackendError::Unknown { msg: e.to_string() })?;
	if hrp != prefix{
		return Err(BackendError::Unknown { msg: format!("invalid Bech32 prefix; expected {}, got {}", prefix, hrp) })
	}

	Ok(Vec::<u8>::from_base32(&data).unwrap())
}


// Prefixes are limited to 6 chars 
// This allows one to specify a string prefix and still implement Clone

#[derive(Clone, Copy)]
pub struct RealApi{
	pub prefix1: Option<char>,
	pub prefix2: Option<char>,
	pub prefix3: Option<char>,
	pub prefix4: Option<char>,
	pub prefix5: Option<char>,
	pub prefix6: Option<char>,
}


impl RealApi{
	pub fn new(prefix: &str) -> RealApi{
		let mut chars = prefix.chars();
		Self{
			prefix1: chars.next(),
			prefix2: chars.next(),
			prefix3: chars.next(),
			prefix4: chars.next(),
			prefix5: chars.next(),
			prefix6: chars.next(),
		}
	}

	pub fn get_prefix(&self) -> String{
		let collection = [
			self.prefix1,
			self.prefix2,
			self.prefix3,
			self.prefix4,
			self.prefix5,
			self.prefix6,
		];

		collection.iter().filter_map(|e| *e).collect()
	}
}



impl BackendApi for RealApi{

	fn canonical_address(&self, address: &str) -> (Result<Vec<u8>, BackendError>, GasInfo) { 
		let gas_cost = GasInfo::free();
		if address.trim().is_empty(){
			return (Err(BackendError::Unknown { msg: "empty address string is not allowed".to_string() }), gas_cost)
		}

		let bz = bytes_from_bech32(address, &self.get_prefix()).unwrap();

		// err = VerifyAddressFormat(bz) - No address format verification here

		(Ok(bz), gas_cost)
	}
	fn human_address(&self, canon: &[u8]) -> (Result<String, BackendError>, GasInfo) { 
		let gas_cost = GasInfo::free();

		if canon.is_empty(){
			return (Ok("".to_string()), gas_cost);
		}

		let address = bech32::encode(&self.get_prefix(), canon.to_base32(), Variant::Bech32).unwrap();

		(Ok(address), gas_cost)
	 }
}
