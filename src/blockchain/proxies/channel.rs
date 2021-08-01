use std::sync::Arc;

use tokio::sync::RwLock;
use web3::{
    types::{
        Address,
        H256,
        U256,
    },
    Transport,
    Web3,
};

use crate::blockchain::contracts::GasMetadata;

use super::{
    common::{
        Account,
        Result,
    },
    TokenNetworkProxy,
};

#[derive(Clone)]
pub struct ChannelProxy<T: Transport> {
    pub token_network: TokenNetworkProxy<T>,
    web3: Web3<T>,
    gas_metadata: Arc<GasMetadata>,
    lock: Arc<RwLock<bool>>,
}

impl<T> ChannelProxy<T>
where
    T: Transport + Send + Sync,
    T::Out: Send,
{
    pub fn new(
        token_network: TokenNetworkProxy<T>,
        web3: Web3<T>,
        gas_metadata: Arc<GasMetadata>,
    ) -> Self {
        Self {
            token_network,
            web3,
            gas_metadata,
            lock: Arc::new(RwLock::new(true)),
        }
    }

    pub async fn approve_and_set_total_deposit(
        &self,
        account: Account<T>,
        channel_identifier: U256,
        partner: Address,
        total_deposit: U256,
        block_hash: H256,
    ) -> Result<()> {
        self.token_network
            .approve_and_set_total_deposit(account, channel_identifier, partner, total_deposit, block_hash)
            .await
    }
}
