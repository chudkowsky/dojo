// Sourced from: https://github.com/starkware-libs/sequencer/blob/15fe7cf0169e4e00f2d5667aeaf44e3a9d7559d8/crates/starknet_patricia/src/storage/map_storage.rs#L1-L40

use std::collections::HashMap;

use cairo_vm::Felt252;
use serde::{Serialize, Serializer};

#[derive(Serialize, Debug, Default)]
pub struct MapStorage {
    pub storage: HashMap<StorageKey, StorageValue>,
}

impl Storage for MapStorage {
    fn get(&self, key: &StorageKey) -> Option<&StorageValue> {
        self.storage.get(key)
    }

    fn set(&mut self, key: StorageKey, value: StorageValue) -> Option<StorageValue> {
        self.storage.insert(key, value)
    }

    fn mget(&self, keys: &[StorageKey]) -> Vec<Option<&StorageValue>> {
        keys.iter().map(|key| self.get(key)).collect::<Vec<_>>()
    }

    fn mset(&mut self, key_to_value: HashMap<StorageKey, StorageValue>) {
        self.storage.extend(key_to_value);
    }

    fn delete(&mut self, key: &StorageKey) -> Option<StorageValue> {
        self.storage.remove(key)
    }
}

impl From<HashMap<StorageKey, StorageValue>> for MapStorage {
    fn from(storage: HashMap<StorageKey, StorageValue>) -> Self {
        Self { storage }
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct StorageKey(pub Vec<u8>);

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct StorageValue(pub Vec<u8>);

pub trait Storage: From<HashMap<StorageKey, StorageValue>> {
    /// Returns value from storage, if it exists.
    fn get(&self, key: &StorageKey) -> Option<&StorageValue>;

    /// Sets value in storage. If key already exists, its value is overwritten and the old value is
    /// returned.
    fn set(&mut self, key: StorageKey, value: StorageValue) -> Option<StorageValue>;

    /// Returns values from storage in same order of given keys. Value is None for keys that do not
    /// exist.
    fn mget(&self, keys: &[StorageKey]) -> Vec<Option<&StorageValue>>;

    /// Sets values in storage.
    fn mset(&mut self, key_to_value: HashMap<StorageKey, StorageValue>);

    /// Deletes value from storage and returns its value if it exists. Returns None if not.
    fn delete(&mut self, key: &StorageKey) -> Option<StorageValue>;
}

// TODO(Aviv, 17/07/2024); Split between Storage prefix representation (trait) and node
// specific implementation (enum).
#[derive(Clone, Debug)]
pub enum StarknetPrefix {
    InnerNode,
    StorageLeaf,
    StateTreeLeaf,
    CompiledClassLeaf,
}

/// Describes a storage prefix as used in Aerospike DB.
impl StarknetPrefix {
    pub fn to_bytes(&self) -> &'static [u8] {
        match self {
            Self::InnerNode => b"patricia_node",
            Self::StorageLeaf => b"starknet_storage_leaf",
            Self::StateTreeLeaf => b"contract_state",
            Self::CompiledClassLeaf => b"contract_class_leaf",
        }
    }

    pub fn to_storage_prefix(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }
}

impl From<Felt252> for StorageKey {
    fn from(value: Felt252) -> Self {
        StorageKey(value.to_bytes_be().to_vec())
    }
}

/// To send storage to Python storage, it is necessary to serialize it.
impl Serialize for StorageKey {
    /// Serializes `StorageKey` to hexadecimal string representation.
    /// Needed since serde's Serialize derive attribute only works on
    /// HashMaps with String keys.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert Vec<u8> to hexadecimal string representation and serialize it.
        serializer.serialize_str(&hex::encode(&self.0))
    }
}

/// Returns a `StorageKey` from a prefix and a suffix.
pub(crate) fn create_db_key(prefix: Vec<u8>, suffix: &[u8]) -> StorageKey {
    StorageKey([prefix, b":".to_vec(), suffix.to_vec()].concat())
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, derive_more::BitAnd, derive_more::Sub, PartialOrd, Ord,
)]
pub struct NodeIndex(pub U256);

#[derive(Debug, PartialEq)]
pub struct OriginalSkeletonTreeImpl<'a> {
    pub nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
}

impl<'a> OriginalSkeletonTree<'a> for OriginalSkeletonTreeImpl<'a> {
    fn create<L: Leaf>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &impl OriginalSkeletonTreeConfig<L>,
        leaf_modifications: &LeafModifications<L>,
    ) -> OriginalSkeletonTreeResult<Self> {
        Self::create_impl(storage, root_hash, sorted_leaf_indices, config, leaf_modifications)
    }

    fn get_nodes(&self) -> &OriginalSkeletonNodeMap {
        &self.nodes
    }

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap {
        &mut self.nodes
    }

    fn create_and_get_previous_leaves<L: Leaf>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &impl OriginalSkeletonTreeConfig<L>,
        leaf_modifications: &LeafModifications<L>,
    ) -> OriginalSkeletonTreeResult<(Self, HashMap<NodeIndex, L>)> {
        Self::create_and_get_previous_leaves_impl(
            storage,
            root_hash,
            sorted_leaf_indices,
            leaf_modifications,
            config,
        )
    }

    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a> {
        self.sorted_leaf_indices
    }
}
