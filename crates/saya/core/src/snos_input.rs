use std::collections::HashMap;

use anyhow::Context;
use bitvec::order::Msb0;
use blockifier::state::cached_state::CachedState;
use cairo_vm::Felt252;
use katana_primitives::ContractAddress;
use pathfinder_common::StorageAddress;
use pathfinder_crypto::Felt;
use pathfinder_merkle_tree::ContractsStorageTree;
use pathfinder_merkle_tree::tree::MerkleTree;
use prove_block::compute_class_commitment;
use prove_block::reexecute::{
    ProverPerContractStorage, format_commitment_facts, reexecute_transactions_with_blockifier,
};
use prove_block::rpc_utils::{get_class_proofs, get_storage_proofs};
use prove_block::types::starknet_rs_tx_to_internal_tx;
use rpc_client::RpcClient;
use rpc_replay::block_context::build_block_context;
use rpc_replay::rpc_state_reader::AsyncRpcStateReader;
use rpc_replay::transactions::starknet_rs_to_blockifier;
use starknet::core::types::{
    BlockId, MaybePendingBlockWithTxHashes, MaybePendingBlockWithTxs, StarknetError,
};
use starknet::providers::{Provider, ProviderError};
use starknet_os::config::{STORED_BLOCK_HASH_BUFFER, StarknetGeneralConfig, StarknetOsConfig};
use starknet_os::crypto::pedersen::PedersenHash;
use starknet_os::execution::helper::ContractStorageMap;
use starknet_os::io::input::StarknetOsInput;
use starknet_os::starknet::business_logic::fact_state::contract_state_objects::ContractState;
use starknet_os::starknet::starknet_storage::CommitmentInfo;
use starknet_os::starkware_utils::commitment_tree::base_types::Height;
use starknet_os::starkware_utils::commitment_tree::patricia_tree::patricia_tree::PatriciaTree;
use starknet_os_types::chain_id::chain_id_from_felt;
use tracing::debug;

use crate::snos::state_utils::get_formatted_state_update;

async fn prepare_snos_input(block_number: u64) -> StarknetOsInput {
    let block_id = BlockId::Number(block_number);
    let previous_block_id = BlockId::Number(block_number - 1);
    let rpc_client = RpcClient::new("http://localhost:5050");

    let provider = rpc_client.starknet_rpc();
    let chain_id = provider.chain_id().await.unwrap();

    let chain_id = chain_id_from_felt(chain_id);
    println!("Chain ID: {:?}", chain_id);
    let mut block_with_txs =
        match rpc_client.starknet_rpc().get_block_with_txs(block_id).await.unwrap() {
            MaybePendingBlockWithTxs::Block(block_with_txs) => block_with_txs,
            MaybePendingBlockWithTxs::PendingBlock(_) => {
                panic!("Block is still pending!");
            }
        };
    block_with_txs.l1_data_gas_price.price_in_wei = Felt252::from(1);
    block_with_txs.l1_data_gas_price.price_in_fri = Felt252::from(1);
    let previous_block = match rpc_client
        .starknet_rpc()
        .get_block_with_tx_hashes(previous_block_id)
        .await
        .unwrap()
    {
        MaybePendingBlockWithTxHashes::Block(block_with_txs) => block_with_txs,
        MaybePendingBlockWithTxHashes::PendingBlock(_) => {
            panic!("Block is still pending!");
        }
    };
    let older_block = match rpc_client
        .starknet_rpc()
        .get_block_with_tx_hashes(BlockId::Number(block_number - STORED_BLOCK_HASH_BUFFER))
        .await
        .unwrap()
    {
        MaybePendingBlockWithTxHashes::Block(block_with_txs_hashes) => block_with_txs_hashes,
        MaybePendingBlockWithTxHashes::PendingBlock(_) => {
            panic!("Block is still pending!");
        }
    };

    let block_context = build_block_context(
        chain_id.clone(),
        &block_with_txs,
        blockifier::versioned_constants::StarknetVersion::V0_13_2,
    );

    let old_block_number = Felt252::from(older_block.block_number);
    let old_block_hash = older_block.block_hash;

    let transactions: Vec<_> = block_with_txs
        .transactions
        .clone()
        .into_iter()
        .map(starknet_rs_tx_to_internal_tx)
        .collect();

    let (processed_state_update, traces) =
        get_formatted_state_update(&rpc_client, previous_block_id, block_id).await.unwrap();

    let class_hash_to_compiled_class_hash =
        processed_state_update.class_hash_to_compiled_class_hash;

    let blockifier_state_reader =
        AsyncRpcStateReader::new(rpc_client.clone(), BlockId::Number(block_number - 1));

    let mut blockifier_state = CachedState::new(blockifier_state_reader);
    assert_eq!(
        block_with_txs.transactions.len(),
        traces.len(),
        "Transactions and traces must have the same length"
    );
    let mut txs = Vec::new();
    for (tx, trace) in block_with_txs.transactions.iter().zip(traces.iter()) {
        let transaction = starknet_rs_to_blockifier(
            tx,
            trace,
            &block_context.block_info().gas_prices,
            &rpc_client,
            block_number,
        )
        .await
        .unwrap();
        txs.push(transaction);
    }
    let tx_execution_infos =
        reexecute_transactions_with_blockifier(&mut blockifier_state, &block_context, txs).unwrap();

    // let storage_proofs: HashMap<Felt252, PathfinderProof> = Default::default();
    // let previous_storage_proofs: HashMap<Felt252, PathfinderProof> = Default::default();
    // TODO: make this work

    // struct ContractStorage {
    //     contract: ContractAddress,
    //     state: HashMap<u64, (pathfinder_storage::StoredNode, pathfinder_crypto::Felt)>,
    // }

    // impl pathfinder_merkle_tree::storage::Storage for ContractStorage {
    //     fn get(&self, index: u64) -> anyhow::Result<Option<pathfinder_storage::StoredNode>> {
    //         Ok(self.state.get(&index).map(|(node, _)| node.clone()))
    //     }

    //     fn hash(&self, index: u64) -> anyhow::Result<Option<pathfinder_crypto::Felt>> {
    //         Ok(self.state.get(&index).map(|(_, hash)| hash.clone()))
    //     }

    //     fn leaf(
    //         &self,
    //         path: &bitvec::slice::BitSlice<u8, Msb0>,
    //     ) -> anyhow::Result<Option<pathfinder_crypto::Felt>> {
    //         assert!(path.len() == 251);

    //         let key = StorageAddress(
    //             Felt::from_bits(path).context("Mapping leaf path to storage address")?,
    //         );

    //         let value = self.tx.storage_value(block.into(), self.contract, key)?.map(|x| x.0);
    //         // let value = self.tx.storage_value(block.into(), self.contract, key)?.map(|x| x.0);

    //         Ok(value)
    //     }
    // }

    // let merkle_tree = MerkleTree::<pathfinder_common::hash::PedersenHash, 251>::empty();
    // let storage = merkle_tree.commit(storage);
    // let contract_address = ContractAddress(Felt252::ZERO);
    // let storage_proof = merkle_tree.get_proof(0, &contract_address, key);
    // let contract_data = ContractsStorageTree::empty(&tx, Height(251));

    let storage_proofs =
        get_storage_proofs(&rpc_client, block_number, &tx_execution_infos, old_block_number)
            .await
            .expect("Failed to fetch storage proofs");
    let previous_storage_proofs =
        get_storage_proofs(&rpc_client, block_number - 1, &tx_execution_infos, old_block_number)
            .await
            .expect("Failed to fetch storage proofs");

    let default_general_config = StarknetGeneralConfig::default();

    let general_config = StarknetGeneralConfig {
        starknet_os_config: StarknetOsConfig {
            chain_id,
            fee_token_address: block_context
                .chain_info()
                .fee_token_addresses
                .strk_fee_token_address,
            deprecated_fee_token_address: block_context
                .chain_info()
                .fee_token_addresses
                .eth_fee_token_address,
        },
        ..default_general_config
    };

    let mut contract_states = HashMap::new();
    let mut contract_storages = ContractStorageMap::new();

    for (contract_address, storage_proof) in storage_proofs.clone() {
        let previous_storage_proof = previous_storage_proofs
            .get(&contract_address)
            .expect("failed to find previous storage proof");
        let contract_storage_root = previous_storage_proof
            .contract_data
            .as_ref()
            .map(|contract_data| contract_data.root)
            .unwrap_or(Felt252::ZERO)
            .into();

        debug!(
            "Storage root 0x{:x} for contract 0x{:x}",
            Into::<Felt252>::into(contract_storage_root),
            contract_address
        );

        let previous_tree = PatriciaTree { root: contract_storage_root, height: Height(251) };

        let contract_storage = ProverPerContractStorage::new(
            rpc_client.clone(),
            previous_block_id,
            contract_address,
            previous_tree.root.into(),
            storage_proof,
            previous_storage_proof.clone(),
        )
        .unwrap();
        contract_storages.insert(contract_address, contract_storage);

        let (previous_class_hash, previous_nonce) =
            if [Felt252::ZERO, Felt252::ONE].contains(&contract_address) {
                (Felt252::ZERO, Felt252::ZERO)
            } else {
                let previous_class_hash = match rpc_client
                    .starknet_rpc()
                    .get_class_hash_at(previous_block_id, contract_address)
                    .await
                {
                    Ok(class_hash) => Ok(class_hash),
                    Err(ProviderError::StarknetError(StarknetError::ContractNotFound)) => {
                        Ok(Felt252::ZERO)
                    }
                    Err(e) => Err(e),
                }
                .unwrap();

                let previous_nonce = match rpc_client
                    .starknet_rpc()
                    .get_nonce(previous_block_id, contract_address)
                    .await
                {
                    Ok(nonce) => Ok(nonce),
                    Err(ProviderError::StarknetError(StarknetError::ContractNotFound)) => {
                        Ok(Felt252::ZERO)
                    }
                    Err(e) => Err(e),
                }
                .unwrap();
                (previous_class_hash, previous_nonce)
            };

        let contract_state = ContractState {
            contract_hash: previous_class_hash.to_bytes_be().to_vec(),
            storage_commitment_tree: previous_tree,
            nonce: previous_nonce,
        };

        contract_states.insert(contract_address, contract_state);
    }

    let compiled_classes = processed_state_update.compiled_classes;
    let deprecated_compiled_classes = processed_state_update.deprecated_compiled_classes;
    let declared_class_hash_component_hashes: HashMap<_, _> = processed_state_update
        .declared_class_hash_component_hashes
        .into_iter()
        .map(|(class_hash, component_hashes)| (class_hash, component_hashes.to_vec()))
        .collect();

    // query storage proofs for each accessed contract
    let class_hashes: Vec<&Felt252> = class_hash_to_compiled_class_hash.keys().collect();
    // TODO: we fetch proofs here for block-1, but we probably also need to fetch at the current
    //       block, likely for contracts that are deployed in this block
    let class_proofs = get_class_proofs(&rpc_client, block_number, &class_hashes[..])
        .await
        .expect("Failed to fetch class proofs");
    let previous_class_proofs = get_class_proofs(&rpc_client, block_number - 1, &class_hashes[..])
        .await
        .expect("Failed to fetch previous class proofs");

    let visited_pcs: HashMap<Felt252, Vec<Felt252>> = blockifier_state
        .visited_pcs
        .iter()
        .map(|(class_hash, visited_pcs)| {
            (class_hash.0, visited_pcs.iter().copied().map(Felt252::from).collect::<Vec<_>>())
        })
        .collect();

    // We can extract data from any storage proof, use the one of the block hash contract
    let block_hash_storage_proof = storage_proofs
        .get(&Felt252::ONE)
        .expect("there should be a storage proof for the block hash contract");
    let previous_block_hash_storage_proof = previous_storage_proofs
        .get(&Felt252::ONE)
        .expect("there should be a previous storage proof for the block hash contract");

    let previous_contract_trie_root =
        previous_block_hash_storage_proof.contract_proof[0].hash::<PedersenHash>();
    let current_contract_trie_root =
        block_hash_storage_proof.contract_proof[0].hash::<PedersenHash>();

    let previous_contract_proofs: Vec<_> =
        previous_storage_proofs.values().map(|proof| proof.contract_proof.clone()).collect();
    let previous_state_commitment_facts =
        format_commitment_facts::<PedersenHash>(&previous_contract_proofs);
    let current_contract_proofs: Vec<_> =
        storage_proofs.values().map(|proof| proof.contract_proof.clone()).collect();
    let current_state_commitment_facts =
        format_commitment_facts::<PedersenHash>(&current_contract_proofs);

    let global_state_commitment_facts: HashMap<_, _> =
        previous_state_commitment_facts.into_iter().chain(current_state_commitment_facts).collect();

    let contract_state_commitment_info = CommitmentInfo {
        previous_root: previous_contract_trie_root,
        updated_root: current_contract_trie_root,
        tree_height: 251,
        commitment_facts: global_state_commitment_facts,
    };

    let contract_class_commitment_info =
        compute_class_commitment(&previous_class_proofs, &class_proofs);

    StarknetOsInput {
        contract_state_commitment_info,
        contract_class_commitment_info,
        deprecated_compiled_classes,
        compiled_classes,
        compiled_class_visited_pcs: visited_pcs,
        contracts: contract_states,
        class_hash_to_compiled_class_hash,
        general_config,
        transactions,
        declared_class_hash_to_component_hashes: declared_class_hash_component_hashes,
        new_block_hash: block_with_txs.block_hash,
        prev_block_hash: previous_block.block_hash,
        full_output: true,
    }
}

#[tokio::test]
async fn test_prepare_snos_input() {
    prepare_snos_input(239242).await;
}
