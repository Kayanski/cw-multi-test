
use crate::wasm_emulation::channel::get_channel;
use crate::wasm_emulation::input::SerChainData;

use super::mock_storage::MockStorage;
use num_bigint::{BigInt, Sign};
use cosmwasm_vm::BackendError;
use cosmwasm_vm::GasInfo;
use std::collections::HashMap;
use cosmrs::proto::cosmos::base::query::v1beta1::PageRequest;
use cosmrs::proto::cosmwasm::wasm::v1::Model;
use cosmwasm_std::Order;
use cosmwasm_std::Record;
use cosmwasm_vm::BackendResult;
use cosmwasm_vm::Storage;
use cw_orch::prelude::queriers::DaemonQuerier;


use cw_orch::prelude::queriers::CosmWasm;




use std::collections::HashSet;

use anyhow::Result as AnyResult;
const DISTANT_LIMIT: u64 = 5u64;

#[derive(Default, Debug)]
struct DistantIter{
   data: Vec<Model>,
   position: usize,
   key: Option<Vec<u8>>, // if set to None, there is no more keys to investigate in the distant container
   reverse: bool
}

/// Iterator to get multiple keys
#[derive(Default, Debug)]
struct Iter {
	distant_iter: DistantIter,
	local_iter: u32
}

pub struct DualStorage{
	pub local_storage: MockStorage,
	pub removed_keys: HashSet<Vec<u8>>,
	pub chain: SerChainData,
	pub contract_addr: String,
    iterators: HashMap<u32, Iter>,
}

impl DualStorage{
	pub fn new(chain: impl Into<SerChainData>, contract_addr: String, init: Option<Vec<(Vec<u8>, Vec<u8>)>>) -> AnyResult<DualStorage>{
		// We create an instance from a code_id, an address, and we run the code in it
		
		let mut local_storage = MockStorage::default();
		for (key, value) in init.unwrap(){
			local_storage.set(&key, &value).0?;
		}

		Ok(Self{
			local_storage,
			chain: chain.into(),
			removed_keys: HashSet::default(),
			contract_addr,
			iterators: HashMap::new()
		})
	}

	pub fn get_all_storage(&mut self) -> AnyResult<Vec<(Vec<u8>, Vec<u8>)>>{
		let iterator_id = self.local_storage.scan(None, None, Order::Ascending).0?;
		let all_records = self.local_storage.all(iterator_id);

		Ok(all_records.0?)
	}

}

impl Storage for DualStorage{
    fn get(&self, key: &[u8]) -> BackendResult<Option<Vec<u8>>>{
    	// First we try to get the value locally
    	let (mut value, gas_info) = self.local_storage.get(key);
    	// If it's not available, we query it online if it was not removed locally
    	if !self.removed_keys.contains(key) && value.as_ref().unwrap().is_none(){

    		let (rt, channel) = get_channel(self.chain.clone()).unwrap();
			let wasm_querier = CosmWasm::new(channel);

    		let distant_result = rt.block_on(wasm_querier.contract_raw_state(self.contract_addr.clone(), key.to_vec()));
    		if let Ok(result) = distant_result{
    			if !result.data.is_empty(){
    				value = Ok(Some(result.data))
    			}
    		}
    	}
    	(value, gas_info)
    }

    // Distant query not implemented yet : TODO
    fn scan(
        &mut self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> BackendResult<u32>{
    	let iterator_id = self.local_storage.scan(start, end, order).0.unwrap();

    	let order_i32: i32 = order.try_into().unwrap();
    	let descending_order: i32 = Order::Descending.try_into().unwrap();

    	let iter = Iter{
    		local_iter: iterator_id,
    		distant_iter: DistantIter { 
    			data: vec![],
    			position: 0,
    			key: Some(start.map(|s| s.to_vec()).unwrap_or(vec![])),
    			reverse: order_i32 == descending_order
    		}
    	};

        let last_id: u32 = self
            .iterators
            .len()
            .try_into()
            .expect("Found more iterator IDs than supported");
        let new_id = last_id + 1;
        self.iterators.insert(new_id, iter);

        (Ok(new_id), GasInfo::free())
    }

    fn next(&mut self, iterator_id: u32) -> BackendResult<Option<Record>>{
    	// In order to get the next element on the iterator, we need to compose with the two iterators we have
		let iterator = match self.iterators.get_mut(&iterator_id) {
            Some(i) => i,
            None => {
                return (
                    Err(BackendError::iterator_does_not_exist(iterator_id)),
                    GasInfo::free(),
                )
            }
        };
    	// 1. We verify that there is enough elements in the distant iterator
    	if iterator.distant_iter.position == iterator.distant_iter.data.len() && iterator.distant_iter.key.is_some(){

    		let (rt, channel) = get_channel(self.chain.clone()).unwrap();
			let wasm_querier = CosmWasm::new(channel);
    		let new_keys = rt.block_on(wasm_querier.all_contract_state(self.contract_addr.clone(), Some(PageRequest { 
	    		key: iterator.distant_iter.key.clone().unwrap(),
	    		offset: 0, 
	    		limit: DISTANT_LIMIT,
	    		count_total: false, 
	    		reverse: iterator.distant_iter.reverse
	    	}))).unwrap();

    		iterator.distant_iter.data.extend(new_keys.models);
    		iterator.distant_iter.key = Some(new_keys.pagination.unwrap().next_key);
    	}

    	// 2. We find the first key in order between distant and local storage
    	let next_local = self.local_storage.peak(iterator.local_iter).unwrap();
    	let next_distant = iterator.distant_iter.data.get(iterator.distant_iter.position);

    	if let Some(local) = next_local{
    		if let Some(distant) = next_distant{
    			// We compare the two keys with the order and return the higher key
    			let key_local = BigInt::from_bytes_be(Sign::Plus, &local.0);
    			let key_distant = BigInt::from_bytes_be(Sign::Plus, &distant.key);
    			if (key_local < key_distant) == iterator.distant_iter.reverse{
    				iterator.distant_iter.position += 1;
    				(Ok(Some((distant.key.clone(), distant.value.clone()))), GasInfo::free())
    			}else{
    				self.local_storage.next(iterator.local_iter)
    			}

    		}else{
    			self.local_storage.next(iterator.local_iter)
    		}
    	}else if let Some(distant) = next_distant{
    		iterator.distant_iter.position += 1;
    		(Ok(Some((distant.key.clone(), distant.value.clone()))), GasInfo::free())
    	}else{
    		(Ok(None), GasInfo::free())
    	}
    }

    fn set(&mut self, key: &[u8], value: &[u8]) -> BackendResult<()>{
    	self.removed_keys.remove(key); // It's not locally removed anymore, because we set it locally
    	self.local_storage.set(key, value) 
    }

    fn remove(&mut self, key: &[u8]) -> BackendResult<()>{
    	self.removed_keys.insert(key.to_vec()); // We indicate locally if it's removed. So that we can remove keys and not query them on the distant chain
    	self.local_storage.remove(key)
    }
}