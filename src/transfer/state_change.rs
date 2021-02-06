use crate::enums::ChainID;
use crate::transfer::state::{ChannelState, TokenNetworkRegistryState, TokenNetworkState};
use serde::{Deserialize, Serialize};
use web3::types::{Address, H256, U64};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    pub chain_id: ChainID,
    pub block_number: U64,
}

impl Block {
    pub fn new(chain_id: ChainID, block_number: U64) -> Block {
        Block { chain_id, block_number }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitChain {
    pub chain_id: ChainID,
    pub block_number: U64,
    pub our_address: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkRegistry {
    pub transaction_hash: Option<H256>,
    pub token_network_registry: TokenNetworkRegistryState,
    pub block_number: U64,
    pub block_hash: H256,
}

impl ContractReceiveTokenNetworkRegistry {
    pub fn new(
        transaction_hash: H256,
        token_network_registry: TokenNetworkRegistryState,
        block_number: U64,
        block_hash: H256,
    ) -> Self {
        ContractReceiveTokenNetworkRegistry {
            transaction_hash: Some(transaction_hash),
            token_network_registry,
            block_number,
            block_hash,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkCreated {
    pub transaction_hash: Option<H256>,
    pub token_network_registry_address: Address,
    pub token_network: TokenNetworkState,
    pub block_number: U64,
    pub block_hash: H256,
}

impl ContractReceiveTokenNetworkCreated {
    pub fn new(
        transaction_hash: H256,
        token_network_registry_address: Address,
        token_network: TokenNetworkState,
        block_number: U64,
        block_hash: H256,
    ) -> Self {
        ContractReceiveTokenNetworkCreated {
            transaction_hash: Some(transaction_hash),
            token_network_registry_address,
            token_network,
            block_number,
            block_hash,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelOpened {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub channel_state: ChannelState,
}

impl ContractReceiveChannelOpened {
    pub fn new(transaction_hash: H256, block_number: U64, block_hash: H256, channel_state: ChannelState) -> Self {
        ContractReceiveChannelOpened {
            transaction_hash: Some(transaction_hash),
            block_number,
            block_hash,
            channel_state,
        }
    }
}
