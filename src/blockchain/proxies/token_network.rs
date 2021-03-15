use std::{
    collections::HashMap,
    sync::Arc,
};

use tokio::sync::{
    Mutex,
    RwLock,
};
use web3::{
    contract::{
        Contract,
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

use super::ProxyError;

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct TokenNetworkProxy<T: Transport> {
    private_key: PrivateKey,
    web3: Web3<T>,
    contract: Contract<T>,
    channel_operations_lock: Arc<RwLock<HashMap<Address, Mutex<bool>>>>,
}

impl<T: Transport> TokenNetworkProxy<T> {
    pub fn new(web3: Web3<T>, contract: Contract<T>, private_key: PrivateKey) -> Self {
        Self {
            web3,
            private_key,
            contract,
            channel_operations_lock: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn address_by_token_address(&self, token_address: Address, block: H256) -> Result<Address> {
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

    pub async fn safety_deprecation_switch(&self, block: H256) -> Result<bool> {
        self.contract
            .query(
                "safety_deprecation_switch",
                (),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn get_channel_identifier(
        &self,
        participant1: Address,
        participant2: Address,
        block: H256,
    ) -> Result<Option<U256>> {
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

    pub async fn new_channel(&self, partner: Address, settle_timeout: U256, block: H256) -> Result<U256> {
        let our_address = self.private_key.address();
        let timeout_min = self.settlement_timeout_min(block).await?;
        let timeout_max = self.settlement_timeout_max(block).await?;

        if settle_timeout < timeout_min || settle_timeout > timeout_max {
            return Err(ProxyError::BrokenPrecondition(format!(
                "settle_timeout must be in range [{}, {}]. Value: {}",
                timeout_min, timeout_max, settle_timeout,
            )));
        }

        if !self.channel_operations_lock.read().await.contains_key(&partner) {
            self.channel_operations_lock
                .write()
                .await
                .insert(partner, Mutex::new(true));
        }
        let channel_operations_lock = self.channel_operations_lock.read().await;
        let partner_lock_guard = channel_operations_lock.get(&partner).unwrap().lock().await;

        let nonce = self
            .web3
            .eth()
            .transaction_count(our_address, Some(BlockNumber::Pending))
            .await
            .map_err(ProxyError::Web3)?;

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
            .await;

        match gas_estimate {
            Ok(gas_estimate) => {
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
            Err(_) => Err(ProxyError::BrokenPrecondition(format!("something something"))),
        }
    }

    pub async fn settlement_timeout_min(&self, block: H256) -> Result<U256> {
        self.contract
            .query(
                "settlement_timeout_min",
                (),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn settlement_timeout_max(&self, block: H256) -> Result<U256> {
        self.contract
            .query(
                "settlement_timeout_max",
                (),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
            .map_err(Into::into)
    }
}
