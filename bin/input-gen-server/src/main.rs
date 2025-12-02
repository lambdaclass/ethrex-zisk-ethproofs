use std::io::Write;
use std::{env, path::Path};
use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use dotenv::dotenv;
use ethrex_common::types::block_execution_witness::ExecutionWitness;
use ethrex_common::types::{Block, BlockHeader, ChainConfig, ELASTICITY_MULTIPLIER};
use ethrex_config::networks::{
    Network, PublicNetwork, HOLESKY_CHAIN_ID, HOODI_CHAIN_ID, MAINNET_CHAIN_ID, SEPOLIA_CHAIN_ID,
};
use ethrex_guest::input::ProgramInput;
use ethrex_rlp::decode::RLPDecode;
use ethrex_rpc::debug::execution_witness::execution_witness_from_rpc_chain_config;
use ethrex_rpc::types::block_identifier::BlockIdentifier;
use ethrex_rpc::EthClient;
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::broadcast::{self, Receiver, Sender},
    task,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use url::Url;

const WS_ADDR: &str = "0.0.0.0:8765";

async fn get_block(eth_client: &EthClient, block_number: u64) -> Result<Block> {
    let rpc_block = eth_client
        .get_block_by_number(BlockIdentifier::Number(block_number), true)
        .await?;

    rpc_block
        .try_into()
        .map_err(|e| anyhow::anyhow!("Error converting RPC block: {:?}", e))
}

async fn get_block_execution_witness(
    eth_client: &EthClient,
    block_number: u64,
    chain_config: ChainConfig,
) -> Result<ExecutionWitness> {
    let rpc_execution_witness = eth_client
        .get_witness(BlockIdentifier::Number(block_number), None)
        .await?;

    let initial_state_root = rpc_execution_witness
        .headers
        .iter()
        .map(|h| BlockHeader::decode(h).unwrap())
        .find(|h| h.number == block_number - 1)
        .map(|h| h.state_root)
        .context("failed to get initial state root")?;

    execution_witness_from_rpc_chain_config(
        rpc_execution_witness,
        chain_config,
        block_number,
        initial_state_root,
    )
    .context("failed to create execution witness")
}

/// Generate the input file for the given block number and return the time taken in milliseconds
pub async fn generate_input_file(
    eth_client: &EthClient,
    chain_config: ChainConfig,
    block: Block,
    block_number: u64,
    inputs_folder: String,
) -> Result<u128> {
    let start = std::time::Instant::now();

    let block_execution_witness =
        get_block_execution_witness(eth_client, block_number, chain_config).await?;

    let ethrex_guest_input = ProgramInput {
        blocks: vec![block],
        execution_witness: block_execution_witness,
        elasticity_multiplier: ELASTICITY_MULTIPLIER,
        fee_configs: None,
    };

    // Create the inputs folder if it doesn't exist
    let input_folder = Path::new(&inputs_folder);
    std::fs::create_dir_all(input_folder)?;

    // Serialize the client input to a binary file
    let input_path = input_folder.join(format!("{}.bin", block_number));
    let mut input_file = std::fs::File::create(input_path.clone())?;
    let ethrex_zisk_guest_input = rkyv::to_bytes::<rkyv::rancor::Error>(&ethrex_guest_input)?;
    input_file.write_all(&ethrex_zisk_guest_input)?;

    let input_file_time = start.elapsed().as_millis();

    Ok(input_file_time)
}

/// Listens for new blocks on the Ethereum network and generates input files for them
async fn block_listener(tx: Sender<String>) -> Result<()> {
    let rpc_https_url = env::var("RPC_URL").expect("RPC_URL must be set");
    let inputs_folder = env::var("INPUTS_FOLDER").unwrap_or("inputs".to_string());
    let block_modulus: u64 = env::var("BLOCK_MODULUS")
        .unwrap_or("100".to_string())
        .parse()
        .expect("BLOCK_MODULUS must be a valid integer");

    let eth_client = EthClient::new(Url::parse(&rpc_https_url).unwrap()).unwrap();

    let chain_id = eth_client.get_chain_id().await.unwrap().as_u64();

    let network = match chain_id {
        MAINNET_CHAIN_ID => Network::PublicNetwork(PublicNetwork::Mainnet),
        HOLESKY_CHAIN_ID => Network::PublicNetwork(PublicNetwork::Holesky),
        HOODI_CHAIN_ID => Network::PublicNetwork(PublicNetwork::Hoodi),
        SEPOLIA_CHAIN_ID => Network::PublicNetwork(PublicNetwork::Sepolia),
        _ => panic!("Unsupported chain id: {chain_id}"),
    };

    let chain_config = network.get_genesis()?.config;

    loop {
        info!("Listening for new blocks on Ethereum Mainnet...");

        let mut block_number = eth_client.get_block_number().await.unwrap().as_u64();

        while block_number % block_modulus != 0 {
            let next_block_number = block_number.next_multiple_of(block_modulus);

            let wait_seconds = next_block_number.saturating_sub(block_number) * 12;

            info!(
                "Received block number {block_number}, waiting {wait_seconds} seconds until block {next_block_number}...",
            );

            tokio::time::sleep(std::time::Duration::from_secs(wait_seconds)).await;

            block_number = eth_client.get_block_number().await.unwrap().as_u64();
        }

        let start = std::time::Instant::now();

        info!("Received block number {}, processing...", block_number);

        if let Err(e) = async {
            let block = get_block(&eth_client, block_number).await?;

            if tx.send(format!("queued {}", block_number)).is_err() {
                info!(
                    "No active receivers, skipping input file generation for block {}",
                    block_number
                );
                return Ok::<(), anyhow::Error>(());
            }

            info!(
                "Generating input file for block {}, txs: {}, gas: {}",
                block_number,
                block.body.transactions.len(),
                block.header.gas_used
            );

            let input_file_time = generate_input_file(
                &eth_client,
                chain_config,
                block,
                block_number,
                inputs_folder.clone(),
            )
            .await?;
            info!(
                "Input file generated for block {}, time: {}ms",
                block_number, input_file_time
            );

            if tx.send(format!("input {}.bin", block_number)).is_err() {
                warn!("No active receivers for broadcast channel");
            }

            Ok::<(), anyhow::Error>(())
        }
        .await
        {
            error!("Error processing block {}, error: {:?}", block_number, e);
        }

        // Wait until 12 seconds have passed to avoid getting the same block again
        tokio::time::sleep(std::time::Duration::from_secs(12).saturating_sub(start.elapsed()))
            .await;
    }
}

/// Handles a single WebSocket client connection
async fn handle_client(stream: TcpStream, mut rx: Receiver<String>) {
    let inputs_folder = env::var("INPUTS_FOLDER").unwrap_or("inputs".to_string());

    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    info!("Client connected");
    let (mut ws_sender, _) = ws_stream.split();

    while let Ok(command) = rx.recv().await {
        let args: Vec<&str> = command.split_whitespace().collect();

        match args.as_slice() {
            ["queued", block_number] => {
                info!("Sending block {} queued", block_number);

                let payload = format!("queued {}", block_number);
                if ws_sender.send(Message::Text(payload)).await.is_err() {
                    error!("Client disconnected");
                    break;
                }
            }
            ["input", file] => {
                info!("Sending input file: {}", file);

                let filepath = PathBuf::from(&inputs_folder).join(file);
                match fs::read(&filepath) {
                    Ok(content) => {
                        let payload = format!("{}\n", file)
                            .into_bytes()
                            .into_iter()
                            .chain(content)
                            .collect::<Vec<u8>>();

                        if ws_sender.send(Message::Binary(payload)).await.is_err() {
                            error!("Client disconnected");
                            break;
                        }

                        info!("Input file sent to client: {}", file);
                    }
                    Err(e) => {
                        error!("Error reading input file {}: {}", file, e);
                    }
                }
            }
            _ => {
                error!("Unknown command received: {}", command);
                continue;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv().ok();

    // Check if LOG_RUST is set; if not, set it to "info"
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "input_gen_server=info");
    }

    // Initialize the logger
    env_logger::init();

    // Load environment variables from .env file
    dotenv().ok();

    // Create broadcast channel for sending input file names to clients
    let (tx, _) = broadcast::channel::<String>(100);

    // Launch eth block input file generator
    let tx_clone = tx.clone();
    task::spawn(async move {
        let _ = block_listener(tx_clone).await;
    });

    // Start listening for WebSocket clients
    let listener = TcpListener::bind(WS_ADDR).await.unwrap();
    info!("WebSocket server listening on ws://{}", WS_ADDR);

    while let Ok((stream, _)) = listener.accept().await {
        let tx_for_client = tx.clone();
        let rx = tx_for_client.subscribe();
        task::spawn(handle_client(stream, rx));
    }
}
