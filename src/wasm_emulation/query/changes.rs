


pub struct StateAnalyzer{

}


impl StateAnalyzer{
	// We get all bank changes

	pub fn new(&app: App){
		let wasm: WasmStorage = app.wrap().
	}


	pub fn get_bank_data()


}



pub fn get_querier_storage<QueryC: CustomQuery>(deps: &Deps<QueryC>) -> AnyResult<QuerierStorage>{
    // We get the wasm storage for all wasm contract to make sure we dispatch everything (with the mock Querier)
    let wasm: WasmStorage = deps.querier.query(&QueryRequest::Stargate { path: STARGATE_ALL_WASM_QUERY_URL.to_string(), data: Binary(vec![]) })?;
    let bank = deps.querier.query(&QueryRequest::Stargate { path: STARGATE_ALL_BANK_QUERY_URL.to_string(), data: Binary(vec![]) })?;
    // log::info!("All local contract storage : {:?}", wasm.storage);
    // log::info!("All local bank storage : {:?}", bank);
    Ok(QuerierStorage{
        wasm,
        bank
    })
}