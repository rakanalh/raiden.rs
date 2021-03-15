use std::sync::Arc;

use parking_lot::RwLock;
use web3::{
    contract::{
        Contract,
        Error,
        Options,
    },
    signing::Key,
    types::{
        Address,
        BlockId,
        BlockNumber,
        H256,
        U256,
    },
    Transport,
    Web3,
};

use crate::blockchain::key::PrivateKey;

#[derive(Clone)]
pub struct TokenNetworkProxy<T: Transport> {
    private_key: PrivateKey,
    web3: Web3<T>,
    contract: Contract<T>,
    lock: Arc<RwLock<bool>>,
}

impl<T: Transport> TokenNetworkProxy<T> {
    pub fn new(web3: Web3<T>, contract: Contract<T>, private_key: PrivateKey) -> Self {
        Self {
            web3,
            private_key,
            contract,
            lock: Arc::new(RwLock::new(true)),
        }
    }

    pub async fn address_by_token_address(&self, token_address: Address, block: H256) -> Result<Address, Error> {
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

    pub async fn safety_deprecation_switch(&self, block: H256) -> Result<bool, Error> {
        self.contract
            .query(
                "safety_deprecation_switch",
                (),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
    }

    pub async fn get_channel_identifier(
        &self,
        participant1: Address,
        participant2: Address,
        block: H256,
    ) -> Result<Option<U256>, Error> {
        let channel_identifier: U256 = self
            .contract
            .query(
                "getChannelIdentifier",
                (participant1, participant2),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await?;

        if channel_identifier.is_zero() {
            return Ok(None);
        }

        Ok(Some(channel_identifier))
    }

    pub async fn new_channel(&self, partner: Address, settle_timeout: U256, block: H256) -> Result<U256, Error> {
        let our_address = self.private_key.address();
        let nonce = self
            .web3
            .eth()
            .transaction_count(our_address, Some(BlockNumber::Pending))
            .await?;
        let gas_price = self.web3.eth().gas_price().await?;
        let gas_estimate = self
            .contract
            .estimate_gas(
                "openChannel",
                (our_address, partner, settle_timeout),
                our_address,
                Options::with(|opt| {
                    opt.value = Some(U256::from(0));
                    opt.nonce = Some(nonce);
                    opt.gas_price = Some(gas_price);
                }),
            )
            .await?;

        let receipt = self
            .contract
            .signed_call_with_confirmations(
                "openChannel",
                (our_address, partner, settle_timeout),
                Options::with(|opt| {
                    opt.value = Some(U256::from(0));
                    opt.gas = Some(gas_estimate);
                    opt.nonce = Some(nonce);
                    opt.gas_price = Some(gas_price);
                }),
                1,
                self.private_key.clone(),
            )
            .await?;

        Ok(self
            .get_channel_identifier(our_address, partner, receipt.block_hash.unwrap())
            .await?
            .unwrap())
    }
}
