use crate::transfer::state::TokenNetworkState;
use crate::transfer::state_change::ContractReceiveTokenNetworkCreated;
use serde::{Deserialize, Serialize};
use web3::types::{Address, H256, U64};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenNetworkCreated {
    pub transaction_hash: Option<H256>,
    pub token_network_registry_address: Address,
    pub token_network: TokenNetworkState,
    pub block_number: U64,
    pub block_hash: H256,
}

impl From<ContractReceiveTokenNetworkCreated> for TokenNetworkCreated {
    fn from(state_change: ContractReceiveTokenNetworkCreated) -> Self {
        TokenNetworkCreated {
            transaction_hash: state_change.transaction_hash,
            token_network_registry_address: state_change.token_network_registry_address,
            token_network: state_change.token_network,
            block_number: state_change.block_number,
            block_hash: state_change.block_hash,
        }
    }
}
