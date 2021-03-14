use std::sync::Arc;

use parking_lot::RwLock;
use web3::{
    contract::{
        Contract,
        Error,
        Options,
    },
    types::{
        Address,
        BlockId,
        H256,
        U256,
    },
    Transport,
};

#[derive(Clone)]
pub struct TokenNetworkRegistryProxy<T: Transport> {
    from: Address,
    contract: Contract<T>,
    lock: Arc<RwLock<bool>>,
}

impl<T: Transport> TokenNetworkRegistryProxy<T> {
    pub fn new(contract: Contract<T>, address: Address) -> Self {
        Self {
            from: address,
            contract,
            lock: Arc::new(RwLock::new(true)),
        }
    }

	pub async fn get_token_network(&self, token_address: Address, block: H256) -> Result<Address, Error> {
        self.contract
            .query(
                "token_to_token_networks",
                (token_address,),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
	}

    pub async fn settlement_timeout_min(&self, block: H256) -> Result<U256, Error> {
        self.contract
            .query(
                "settlement_timeout_min",
                (),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
    }

    pub async fn settlement_timeout_max(&self, block: H256) -> Result<U256, Error> {
        self.contract
            .query(
                "settlement_timeout_max",
                (),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
    }
}
