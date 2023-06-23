


use cosmwasm_std::Addr;
use cosmwasm_std::Empty;
use cw_multi_test::wasm_emulation::contract::WasmContract;
use cw_multi_test::wasm_emulation::contract::DistantContract;
use cw_multi_test::wasm_emulation::contract::run_contract;

use cosmwasm_std::testing::mock_info;
use cw20::Cw20ExecuteMsg;
use cw_multi_test::wasm_emulation::input::ExecuteArgs;


use std::collections::HashMap;
use std::println;



use cosmwasm_std::to_binary;
use cosmwasm_std::testing::mock_env;
use cw_multi_test::wasm_emulation::input::QueryArgs;
use cw_multi_test::wasm_emulation::input::WasmFunction;
use cw_multi_test::wasm_emulation::input::InstanceArguments;

use cw_orch::prelude::networks::PHOENIX_1;
use cw20::Cw20QueryMsg;


type RootStorage =HashMap<String, Vec<(Vec<u8>, Vec<u8>)>>;

pub fn test_scan(storage: &RootStorage, contract_addr: String){


	let query = "terra1e8lqmv3egtgps9nux04vw8gd4pr3qp9h00y7um";

	let mut contract_env = mock_env();
	contract_env.contract.address = Addr::unchecked(contract_addr.clone());
	// Query :
	let query_args = InstanceArguments{
		contract: WasmContract::DistantContract(DistantContract{
			contract_addr: contract_addr.to_string(),
			chain: PHOENIX_1.into(),
		}),

		function: WasmFunction::Query(QueryArgs{
			env: contract_env,
			msg: to_binary(&Cw20QueryMsg::AllAccounts { start_after: Some(query.to_string()), limit: Some(30) } ).unwrap().to_vec()
		}),
		init_storage: storage.get(&contract_addr).cloned().unwrap_or(vec![])
	};

	let decoded_query_result = run_contract::<Empty>(query_args).unwrap();
	println!("Accounts : {:?}", decoded_query_result);
}


pub fn main(){
	env_logger::init();

	// This total storage object stores everything, by contract key
	let mut storage : RootStorage = HashMap::new();
	
	let contract_addr = "terra1lxx40s29qvkrcj8fsa3yzyehy7w50umdvvnls2r830rys6lu2zns63eelv";
	let sender = "terra17c6ts8grcfrgquhj3haclg44le8s7qkx6l2yx33acguxhpf000xqhnl3je";
	let recipient = "terra1e9lqmv3egtgps9nux04vw8gd4pr3qp9h00y8um";

	let mut contract_env = mock_env();
	contract_env.contract.address = Addr::unchecked(contract_addr);

	test_scan(&storage, contract_addr.to_string());

	// Query :
	let query_args = InstanceArguments{
		contract: WasmContract::DistantContract(DistantContract{
			contract_addr: "terra1lxx40s29qvkrcj8fsa3yzyehy7w50umdvvnls2r830rys6lu2zns63eelv".to_string(),
			chain: PHOENIX_1.into(),
		}),
		function: WasmFunction::Query(QueryArgs{
			env: contract_env.clone(),
			msg: to_binary(&Cw20QueryMsg::Balance { address: recipient.to_string() }).unwrap().to_vec()
		}),
		init_storage: storage.get(contract_addr).cloned().unwrap_or(vec![])
	};

	let decoded_query_result = run_contract::<Empty>(query_args).unwrap();
	println!("Balance before : {:?}", decoded_query_result);

	// We start by creating the call object

	// Execute: 
	let execute_args = InstanceArguments{
		contract: WasmContract::DistantContract(DistantContract{
			contract_addr: contract_addr.to_string(),
			chain: PHOENIX_1.into(),
		}),
		function: WasmFunction::Execute(ExecuteArgs{
			env: contract_env.clone(),
			info: mock_info(sender, &[]),
			msg: to_binary(&Cw20ExecuteMsg::Transfer { recipient: recipient.to_string(), amount: 1_000_000u128.into() }).unwrap().to_vec()
		}),
		init_storage: storage.get(contract_addr).cloned().unwrap_or(vec![])
	};

	let decoded_result = run_contract::<Empty>(execute_args).unwrap();
	println!("Result : {:?}", decoded_result);

	let mut storage_before = storage.get(&contract_addr.to_string()).cloned().unwrap_or(vec![]);

	// We change all the values with the output
    for (key, value) in &decoded_result.storage.current_keys{
        if let Some(pos) = storage_before.iter().position(|el| el.0 == key.clone()){
        	storage_before[pos] = (key.clone(), value.clone())
        }else{
        	storage_before.push((key.clone(), value.clone()))
        }
    } 

    // We remove all values that need to be removed from it
    for key in &decoded_result.storage.removed_keys{
        if let Some(pos) = storage_before.iter().position(|el| el.0 == key.clone()){
        	storage_before.remove(pos);
        }
    }

	storage.insert(contract_addr.to_string(), storage_before);

 
	let query_args = InstanceArguments{
		contract: WasmContract::DistantContract(DistantContract{
			contract_addr: "terra1lxx40s29qvkrcj8fsa3yzyehy7w50umdvvnls2r830rys6lu2zns63eelv".to_string(),
			chain: PHOENIX_1.into(),
		}),
		function: WasmFunction::Query(QueryArgs{
			env: contract_env,
			msg: to_binary(&Cw20QueryMsg::Balance { address: recipient.to_string() }).unwrap().to_vec()
		}),
		init_storage: storage.get(contract_addr).cloned().unwrap_or(vec![])
	};

	let decoded_query_result = run_contract::<Empty>(query_args).unwrap();
	println!("Balance after : {:?}", decoded_query_result);


	test_scan(&storage, contract_addr.to_string());
}