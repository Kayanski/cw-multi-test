use cosmwasm_vm::testing::MockApi;
use cosmwasm_vm::BackendApi;
use cosmwasm_vm::BackendError;
use cosmwasm_vm::GasInfo;

use self::real_api::RealApi;


pub mod real_api;

#[derive(Clone, Copy)]
pub struct MixedApi{
    pub real_api: RealApi,
    pub mock_api: MockApi,
}

impl BackendApi for MixedApi{

    fn canonical_address(&self, address: &str) -> (Result<Vec<u8>, BackendError>, GasInfo) { 

        // First we try with the real API
        let real_api_result = self.real_api.canonical_address(address);
        if real_api_result.0.is_ok(){
            real_api_result
        }else{
            // Then with the mock API
            self.mock_api.canonical_address(address)
        }
    }
    fn human_address(&self, canon: &[u8]) -> (Result<String, BackendError>, GasInfo) {
        // First we try with the real API
        let real_api_result = self.real_api.human_address(canon);
        if real_api_result.0.is_ok(){
            real_api_result
        }else{
            // Then with the mock API
            self.mock_api.human_address(canon)
        }
     }
}
