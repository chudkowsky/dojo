use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use super::allocation::{GenesisAccountAlloc, GenesisAllocation, GenesisContractAlloc};
use super::json::{GenesisJson, GenesisJsonError};
use super::{FeeTokenConfig, Genesis, GenesisClass, UniversalDeployerConfig};
use crate::block::{BlockHash, BlockNumber, GasPrices};
use crate::class::ClassHash;
use crate::contract::ContractAddress;
use crate::genesis::json::parse_genesis_class_artifacts;
use crate::FieldElement;

#[derive(Debug, thiserror::Error)]
pub enum GenesisBuilderError {
    #[error("parent hash not set")]
    ParentHashNotSet,
    #[error("state root not set")]
    StateRootNotSet,
    #[error("timestamp not set")]
    TimestampNotSet,
    #[error("block number not set")]
    NumberNotSet,
    #[error("sequencer address not set")]
    SequencerAddressNotSet,
    #[error("l1 gas prices not set")]
    L1GasPricesNotSet,
    #[error("fee token not set")]
    FeeTokenNotSet,
    #[error("no class found with hash {class_hash:#x} for contract {address}")]
    UnknownClassHash { address: ContractAddress, class_hash: ClassHash },
    #[error("contract allocation is missing a class hash")]
    MissingClassHash,
    #[error("cannot allocate zero address")]
    InvalidZeroAddress,
    #[error("error parsing the class artifact: {0}")]
    ClassParsingError(#[from] GenesisJsonError),
}

/// A convenient builder for creating a genesis state. This is the recommended way to programmatically
/// create a genesis state as it enforces the required fields.
#[derive(Debug, Clone)]
pub struct Builder {
    parent_hash: Option<BlockHash>,
    state_root: Option<FieldElement>,
    number: Option<BlockNumber>,
    timestamp: Option<u64>,
    sequencer_address: Option<ContractAddress>,
    gas_prices: Option<GasPrices>,
    fee_token: Option<FeeTokenConfig>,
    udc: Option<UniversalDeployerConfig>,
    raw_classes: Vec<(Value, Option<ClassHash>)>,
    allocations: BTreeMap<ContractAddress, GenesisAllocation>,
    // for compatibility when creating a new builder from an existing genesis
    classes: HashMap<ClassHash, GenesisClass>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            parent_hash: None,
            state_root: None,
            number: None,
            timestamp: None,
            sequencer_address: None,
            gas_prices: None,
            fee_token: None,
            udc: None,
            raw_classes: Vec::new(),
            allocations: BTreeMap::new(),
            classes: HashMap::new(),
        }
    }

    pub fn parent_hash(self, hash: BlockHash) -> Self {
        Self { parent_hash: Some(hash), ..self }
    }

    pub fn state_root(self, state_root: FieldElement) -> Self {
        Self { state_root: Some(state_root), ..self }
    }

    pub fn number(self, number: BlockNumber) -> Self {
        Self { number: Some(number), ..self }
    }

    pub fn timestamp(self, timestamp: u64) -> Self {
        Self { timestamp: Some(timestamp), ..self }
    }

    // TODO: need to ensure the sequencer address is one of the allocated accounts.
    // but this is not enforced in the json config (WARN).
    pub fn sequencer_address(self, address: ContractAddress) -> Self {
        Self { sequencer_address: Some(address), ..self }
    }

    pub fn gas_prices(self, gas_prices: GasPrices) -> Self {
        Self { gas_prices: Some(gas_prices), ..self }
    }

    pub fn fee_token(self, fee_token: FeeTokenConfig) -> Self {
        Self { fee_token: Some(fee_token), ..self }
    }

    pub fn universal_deployer(self, udc: UniversalDeployerConfig) -> Self {
        Self { udc: Some(udc), ..self }
    }

    pub fn add_classes<I>(mut self, classes: I) -> Self
    where
        I: IntoIterator<Item = (Value, Option<ClassHash>)>,
    {
        self.raw_classes.extend(classes);
        self
    }

    pub fn add_accounts<I>(mut self, accounts: I) -> Self
    where
        I: IntoIterator<Item = (ContractAddress, GenesisAccountAlloc)>,
    {
        let accounts = accounts
            .into_iter()
            .map(|(address, alloc)| (address, GenesisAllocation::Account(alloc)));
        self.allocations.extend(accounts);
        self
    }

    pub fn add_contracts<I>(mut self, contracts: I) -> Self
    where
        I: IntoIterator<Item = (ContractAddress, GenesisContractAlloc)>,
    {
        let contracts = contracts
            .into_iter()
            .map(|(address, alloc)| (address, GenesisAllocation::Contract(alloc)));
        self.allocations.extend(contracts);
        self
    }

    pub fn build(mut self) -> Result<Genesis, GenesisBuilderError> {
        let number = self.number.unwrap_or_default();
        let timestamp = self.timestamp.unwrap_or_default();
        let state_root = self.state_root.unwrap_or_default();
        let parent_hash = self.parent_hash.unwrap_or_default();

        let gas_prices = self.gas_prices.ok_or(GenesisBuilderError::L1GasPricesNotSet)?;
        let seq_addr = self.sequencer_address.ok_or(GenesisBuilderError::SequencerAddressNotSet)?;

        for (class, hash) in self.raw_classes {
            let (hash, class) = parse_genesis_class_artifacts(hash, class)?;
            self.classes.entry(hash).or_insert(class);
        }

        for (address, alloc) in &mut self.allocations {
            let class_hash = alloc.class_hash().ok_or(GenesisBuilderError::MissingClassHash)?;
            if !self.classes.contains_key(&class_hash) {
                return Err(GenesisBuilderError::UnknownClassHash {
                    class_hash,
                    address: *address,
                });
            }
        }

        let fee_token = {
            let Some(token) = self.fee_token else {
                return Err(GenesisBuilderError::FeeTokenNotSet);
            };

            if token.address != ContractAddress::default() {
                return Err(GenesisBuilderError::InvalidZeroAddress);
            }

            if self.classes.get(&token.class_hash).is_none() {
                return Err(GenesisBuilderError::UnknownClassHash {
                    address: token.address,
                    class_hash: token.class_hash,
                });
            }

            token
        };

        if let Some(udc) = &self.udc {
            if self.classes.get(&udc.class_hash).is_none() {
                return Err(GenesisBuilderError::UnknownClassHash {
                    address: udc.address,
                    class_hash: udc.class_hash,
                });
            }
        }

        Ok(Genesis {
            parent_hash,
            state_root,
            number,
            timestamp,
            gas_prices,
            fee_token,
            classes: self.classes,
            sequencer_address: seq_addr,
            universal_deployer: self.udc,
            allocations: self.allocations,
        })
    }

    /// Similar to [Builder::build] but returns the JSON representation of the genesis.
    #[allow(dead_code)]
    fn json(self) -> Result<GenesisJson, GenesisBuilderError> {
        todo!("should build the genesis into GenesisJson format")
    }
}

// TODO: implement Default for builder with the Genesis's default
impl From<Genesis> for Builder {
    fn from(value: Genesis) -> Self {
        Self {
            parent_hash: Some(value.parent_hash),
            state_root: Some(value.state_root),
            number: Some(value.number),
            timestamp: Some(value.timestamp),
            sequencer_address: Some(value.sequencer_address),
            gas_prices: Some(value.gas_prices),
            fee_token: Some(value.fee_token),
            udc: value.universal_deployer,
            raw_classes: Vec::new(),
            allocations: value.allocations,
            classes: value.classes,
        }
    }
}

#[cfg(test)]
mod tests {
    use starknet::macros::felt;

    use crate::genesis::allocation::GenesisAccount;

    use super::*;

    #[test]
    fn build_genesis() {
        let number = 789;
        let timestamp = 1337;
        let state_root = 222u8.into();
        let parent_hash = 111u8.into();
        let sequencer_address = felt!("0x999").into();
        let gas_prices = GasPrices { eth: 1, strk: 2 };
        let fee_token = FeeTokenConfig::default();

        let classes = [];
        let accounts = [];
        let contracts = [];

        let genesis = Builder::new()
            .number(number)
            .timestamp(timestamp)
            .state_root(state_root)
            .parent_hash(parent_hash)
            .fee_token(fee_token.clone())
            .gas_prices(gas_prices.clone())
            .sequencer_address(sequencer_address)
            .add_classes(classes)
            .add_accounts(accounts)
            .add_contracts(contracts)
            .build()
            .unwrap();

        assert_eq!(genesis.number, number);
        assert_eq!(genesis.timestamp, timestamp);
        assert_eq!(genesis.fee_token, fee_token);
        assert_eq!(genesis.gas_prices, gas_prices);
        assert_eq!(genesis.state_root, state_root);
        assert_eq!(genesis.parent_hash, parent_hash);
        assert_eq!(genesis.sequencer_address, sequencer_address);
        assert_eq!(genesis.classes, HashMap::new());
        assert_eq!(genesis.allocations, BTreeMap::new());
    }

    #[test]
    fn default_builder() {}

    #[test]
    fn cant_build_with_missing_class() {
        let builder = Builder::new()
            .gas_prices(GasPrices::default())
            .sequencer_address(ContractAddress::default());

        // cant build genesis with account that has no class
        let (address, account) = GenesisAccount::new(Default::default(), 23u8.into());
        let b = builder.clone().add_accounts([(address, account.clone().into())]);
        assert!(b.build().unwrap_err().to_string().contains("no class found"));

        // cant build genesis with contract that has no class
        let contract = GenesisContractAlloc { class_hash: Some(23u8.into()), ..Default::default() };
        let b = builder.clone().add_contracts([(felt!("1").into(), contract)]);
        assert!(b.build().unwrap_err().to_string().contains("no class found"));

        // cant build genesis with fee token (contract) that has no class
        let token = FeeTokenConfig { class_hash: 1u8.into(), ..Default::default() };
        let b = builder.clone().fee_token(token.clone());
        assert!(b.build().unwrap_err().to_string().contains("no class found"));

        // cant build genesis with udc (contract) that has no class
        let udc = UniversalDeployerConfig { class_hash: 1u8.into(), ..Default::default() };
        let b = builder.fee_token(token).universal_deployer(udc);
        assert!(b.build().unwrap_err().to_string().contains("no class found"));
    }
}
