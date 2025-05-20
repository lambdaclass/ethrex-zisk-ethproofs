use std::{env, path::Path, sync::Arc, time::Instant};

use alloy_provider::{network::Ethereum, RootProvider};
use anyhow::{anyhow, Ok, Result};
use reth_chainspec::ChainSpec;
use rsp_host_executor::EthHostExecutor;
use rsp_primitives::genesis::Genesis;
use rsp_rpc_db::RpcDb;
use url::Url;

use crate::INPUT_FOLDER;

/// Generate the input file for the given block number and return the time taken in milliseconds
pub async fn generate_input_file(block_number: u64) -> Result<u128> {
    // Load RPC URL from environment variable
    let rpc_url = Url::parse(env::var("RPC_URL").unwrap().as_str()).expect("RPC_URL must be set");

    // Create a new host executor
    let provider = RootProvider::<Ethereum>::new_http(rpc_url);
    let rpc_db = RpcDb::new(provider.clone(), block_number - 1);
    let genesis = &Genesis::Mainnet;
    let chain_spec: Arc<ChainSpec> = Arc::new(genesis.try_into().unwrap());
    let host_executor = EthHostExecutor::eth(chain_spec.clone(), None);

    let start = Instant::now();

    // Execute the host to get the client input
    let client_input = match host_executor
        .execute(
            block_number,
            &rpc_db,
            &provider,
            genesis.clone(),
            None,
            false,
        )
        .await
    {
        std::result::Result::Ok(client_input) => client_input,
        Err(e) => return Err(anyhow!("Error fetching input file data: {}", e)),
    };

    // Create the inputs folder if it doesn't exist
    let input_folder = Path::new(INPUT_FOLDER);
    std::fs::create_dir_all(input_folder)?;

    // Serialize the client input to a binary file
    let input_path = input_folder.join(format!("{}.bin", block_number));
    let mut input_file = std::fs::File::create(input_path.clone())?;
    bincode::serialize_into(&mut input_file, &client_input)?;

    let input_file_time = start.elapsed().as_millis();

    Ok(input_file_time)
}
