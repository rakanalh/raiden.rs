use std::sync::Arc;

use parking_lot::RwLock;
use web3::{
    contract::{
        Contract,
        Options,
    },
    types::{
        Address,
        BlockId,
    },
    Transport,
};

use crate::types::{
    BlockHash,
    SettleTimeout,
};

use super::{
    contract::TokenNetworkContract,
    ProxyError,
};

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct TokenNetworkRegistryProxy<T: Transport> {
    contract: TokenNetworkContract<T>,
    lock: Arc<RwLock<bool>>,
}

impl<T: Transport> TokenNetworkRegistryProxy<T> {
    pub fn new(contract: Contract<T>) -> Self {
        Self {
            contract: TokenNetworkContract { inner: contract },
            lock: Arc::new(RwLock::new(true)),
        }
    }

    pub async fn get_token_network(&self, token_address: Address, block: BlockHash) -> Result<Address> {
        self.contract
            .query(
                "token_to_token_networks",
                (token_address,),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn settlement_timeout_min(&self, block: BlockHash) -> Result<SettleTimeout> {
        self.contract.settlement_timeout_min(block).await
    }

    pub async fn settlement_timeout_max(&self, block: BlockHash) -> Result<SettleTimeout> {
        self.contract.settlement_timeout_max(block).await
    }
}
