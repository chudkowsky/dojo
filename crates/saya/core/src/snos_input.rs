use rpc_client::RpcClient;
use starknet::providers::Provider;
use starknet_os_types::chain_id::chain_id_from_felt;
use starknet::core::types::{BlockId, MaybePendingBlockWithTxHashes, MaybePendingBlockWithTxs, StarknetError};
use std::collections::HashMap;

use blockifier::state::cached_state::CachedState;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use cairo_vm::Felt252;
use rpc_client::pathfinder::proofs::PathfinderClassProof;
use rpc_replay::block_context::build_block_context;
use rpc_replay::rpc_state_reader::AsyncRpcStateReader;
use rpc_replay::transactions::{starknet_rs_to_blockifier, ToBlockifierError};
use starknet_api::StarknetApiError;
use starknet_os::config::{StarknetGeneralConfig, StarknetOsConfig, STORED_BLOCK_HASH_BUFFER};
use starknet_os::crypto::pedersen::PedersenHash;
use starknet_os::crypto::poseidon::PoseidonHash;
use starknet_os::error::SnOsError::{self};
use starknet_os::execution::helper::{ContractStorageMap, ExecutionHelperWrapper};
use starknet_os::io::input::StarknetOsInput;
use starknet_os::io::output::StarknetOsOutput;
use starknet_os::starknet::business_logic::fact_state::contract_state_objects::ContractState;
use starknet_os::starknet::starknet_storage::CommitmentInfo;
use starknet_os::starkware_utils::commitment_tree::base_types::Height;
use starknet_os::starkware_utils::commitment_tree::errors::TreeError;
use starknet_os::starkware_utils::commitment_tree::patricia_tree::patricia_tree::PatriciaTree;
use starknet_os::{config, run_os};
use starknet_os_types::error::ContractClassError;
use starknet_os_types::starknet_core_addons::LegacyContractDecompressionError;
use thiserror::Error;

use crate::snos::state_utils::get_formatted_state_update;


async fn prepare_snos_input(block_number: u64) {
    let block_id = BlockId::Number(block_number);
    let previous_block_id = BlockId::Number(block_number - 1);
    let rpc_client = RpcClient::new("http://localhost:5050");

    let provider = rpc_client.starknet_rpc();
    let chain_id = provider.chain_id().await.unwrap();

    let chain_id = chain_id_from_felt(chain_id);
    println!("Chain ID: {:?}", chain_id);
    let mut block_with_txs = match rpc_client.starknet_rpc().get_block_with_txs(block_id).await.unwrap() {
        MaybePendingBlockWithTxs::Block(block_with_txs) => block_with_txs,
        MaybePendingBlockWithTxs::PendingBlock(_) => {
            panic!("Block is still pending!");
        } 
    };
    block_with_txs.l1_data_gas_price.price_in_wei = Felt252::from(1);
    block_with_txs.l1_data_gas_price.price_in_fri = Felt252::from(1);
    let previous_block = match rpc_client.starknet_rpc().get_block_with_tx_hashes(previous_block_id).await.unwrap() {
        MaybePendingBlockWithTxHashes::Block(block_with_txs) => block_with_txs,
        MaybePendingBlockWithTxHashes::PendingBlock(_) => {
            panic!("Block is still pending!");
        }
    };
    let older_block = match rpc_client
    .starknet_rpc()
    .get_block_with_tx_hashes(BlockId::Number(block_number - STORED_BLOCK_HASH_BUFFER))
    .await.unwrap()
{
    MaybePendingBlockWithTxHashes::Block(block_with_txs_hashes) => block_with_txs_hashes,
    MaybePendingBlockWithTxHashes::PendingBlock(_) => {
        panic!("Block is still pending!");
    }
};
    println!("\n");
    println!("Block with txs: {:?}", block_with_txs);
    println!("\n");
    println!("Block with txs: {:?}", block_with_txs);
    println!("\n");
    println!("Older block: {:?}", older_block);
    println!("\n");
    let block_context = build_block_context(chain_id.clone(), &block_with_txs, blockifier::versioned_constants::StarknetVersion::V0_13_2);
    println!("Block context: {:?}", block_context);
    println!("\n");
    let old_block_number = Felt252::from(older_block.block_number);
    let old_block_hash = older_block.block_hash;
    let (processed_state_update, traces) = get_formatted_state_update(&rpc_client, previous_block_id, block_id).await.unwrap();
    println!("Processed state update: {:?}", processed_state_update);
    println!("\n");
    println!("Traces: {:?}", traces);
    
}

#[tokio::test]
async fn test_prepare_snos_input() {
    prepare_snos_input(239242).await;
}