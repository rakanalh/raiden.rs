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

use crate::blockchain::contracts::GasMetadata;

use super::{
    common::Account,
    ProxyError,
    TokenProxy,
};

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct TokenNetworkProxy<T: Transport> {
    account: Account<T>,
    web3: Web3<T>,
    gas_metadata: Arc<GasMetadata>,
    token_proxy: TokenProxy<T>,
    contract: Contract<T>,
    channel_operations_lock: Arc<RwLock<HashMap<Address, Mutex<bool>>>>,
}

impl<T: Transport> TokenNetworkProxy<T> {
    pub fn new(
        web3: Web3<T>,
        account: Account<T>,
        gas_metadata: Arc<GasMetadata>,
        contract: Contract<T>,
        token_proxy: TokenProxy<T>,
    ) -> Self {
        Self {
            web3,
            account,
            gas_metadata,
            token_proxy,
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
        let our_address = self.account.address();
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
        let _partner_lock_guard = channel_operations_lock.get(&partner).unwrap().lock().await;

        if let Ok(Some(channel_identifier)) = self.get_channel_identifier(our_address, partner, block).await {
            return Err(ProxyError::BrokenPrecondition(format!(
                "A channel with identifier: {} already exists with partner {}",
                channel_identifier, partner
            )));
        }

        let token_network_deposit_limit = self.token_network_deposit_limit(block).await?;
        let token_network_balance = self.token_proxy.balance_of(self.contract.address(), block).await?;

        if token_network_balance >= token_network_deposit_limit {
            return Err(ProxyError::BrokenPrecondition(format!(
                "Cannot open another channe, token network deposit limit reached",
            )));
        }

        let safety_deprecation_switch = self.safety_deprecation_switch(block).await?;
        if safety_deprecation_switch {
            return Err(ProxyError::BrokenPrecondition(format!(
                "This token network is deprecated",
            )));
        }

        let nonce = self.account.next_nonce().await;

        let gas_price = self.web3.eth().gas_price().await?;

        match self
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
            .await
        {
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
                        self.account.private_key(),
                    )
                    .await?;

                Ok(self
                    .get_channel_identifier(our_address, partner, receipt.block_hash.unwrap())
                    .await?
                    .unwrap())
            }
            Err(_) => {
                let failed_at = self
                    .web3
                    .eth()
                    .block(BlockId::Number(BlockNumber::Latest))
                    .await
                    .ok()
                    .flatten()
                    .unwrap();
                let failed_at_blocknumber = failed_at.number.unwrap();
                let failed_at_blockhash = failed_at.hash.unwrap();

                self.account.check_for_insufficient_eth(
                    self.gas_metadata.get("TokenNetwork.openChannel").into(),
                    failed_at_blocknumber,
                ).await?;

                if let Ok(Some(channel_identifier)) = self
                    .get_channel_identifier(our_address, partner, failed_at_blockhash)
                    .await
                {
                    return Err(ProxyError::Recoverable(format!(
                        "A channel with identifier: {} already exists with partner {}",
                        channel_identifier, partner
                    )));
                }

                let token_network_deposit_limit = self.token_network_deposit_limit(failed_at_blockhash).await?;
                let token_network_balance = self
                    .token_proxy
                    .balance_of(self.contract.address(), failed_at_blockhash)
                    .await?;

                if token_network_balance >= token_network_deposit_limit {
                    return Err(ProxyError::Recoverable(format!(
                        "Cannot open another channe, token network deposit limit reached",
                    )));
                }

                let safety_deprecation_switch = self.safety_deprecation_switch(failed_at_blockhash).await?;
                if safety_deprecation_switch {
                    return Err(ProxyError::Recoverable(format!("This token network is deprecated",)));
                }

                Err(ProxyError::Recoverable(format!(
                    "Creating a new channel failed. Gas estimation failed for
					unknown reason. Reference block {} - {}",
                    failed_at_blockhash, failed_at_blocknumber,
                )))
            }
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

    pub async fn token_network_deposit_limit(&self, block: H256) -> Result<U256> {
        self.contract
            .query(
                "token_network_deposit_limit",
                (),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
            .map_err(Into::into)
    }
}
