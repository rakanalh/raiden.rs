use crate::state_machine::{
    state::{
        CanonicalIdentifier,
        ChannelState,
        TokenNetworkRegistryState,
        TokenNetworkState,
    },
    types::ChainID,
};
use serde::{
    Deserialize,
    Serialize,
};
use web3::types::{
    Address,
    Bytes,
    H256,
    U256,
    U64,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum StateChange {
    Block(Block),
    ActionInitChain(ActionInitChain),
    ContractReceiveTokenNetworkRegistry(ContractReceiveTokenNetworkRegistry),
    ContractReceiveTokenNetworkCreated(ContractReceiveTokenNetworkCreated),
    ContractReceiveChannelOpened(ContractReceiveChannelOpened),
    ContractReceiveChannelClosed(ContractReceiveChannelClosed),
    ContractReceiveChannelSettled(ContractReceiveChannelSettled),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    pub block_number: U64,
    pub block_hash: H256,
    pub gas_limit: U256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitChain {
    pub chain_id: ChainID,
    pub block_number: U64,
    pub block_hash: H256,
    pub our_address: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkRegistry {
    pub transaction_hash: Option<H256>,
    pub token_network_registry: TokenNetworkRegistryState,
    pub block_number: U64,
    pub block_hash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkCreated {
    pub transaction_hash: Option<H256>,
    pub token_network_registry_address: Address,
    pub token_network: TokenNetworkState,
    pub block_number: U64,
    pub block_hash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelOpened {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub channel_state: ChannelState,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelClosed {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub transaction_from: Address,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelSettled {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub canonical_identifier: CanonicalIdentifier,
    pub our_onchain_locksroot: Bytes,
    pub partner_onchain_locksroot: Bytes,
}
