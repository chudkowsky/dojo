use rpc_client::RpcClient;
use starknet::core::types::BlockId;
use starknet_os_types::chain_id::chain_id_from_felt;

async fn prepare_snos_input(block_number: u64,){
    let block_id = BlockId::Number(block_number);
    let previous_block_id = BlockId::Number(block_number - 1);
    let rpc_client = RpcClient::new("http://localhost:5050");
    let chain_id = chain_id_from_felt(rpc_client.starknet_rpc().chain_id().await);
    println!("Chain ID: {:?}", chain_id);
}