use std::{
    collections::HashMap,
    sync::Arc,
};

use tokio::sync::{
    Mutex,
    RwLock,
};
use web3::contract::Contract;
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
use crate::blockchain::proxies::transaction::Transaction;

use super::transaction::{
    ChannelSetTotalDepositTransaction,
    ChannelSetTotalDepositTransactionParams,
};
use super::{
    contract::{
        ParticipantDetails,
        TokenNetworkContract,
    },
    transaction::{
        ChannelOpenTransaction,
        ChannelOpenTransactionParams,
    },
    TokenProxy,
};

use super::common::{Account, Result};

#[derive(Clone)]
pub struct TokenNetworkProxy<T: Transport> {
    web3: Web3<T>,
    gas_metadata: Arc<GasMetadata>,
    token_proxy: TokenProxy<T>,
    contract: TokenNetworkContract<T>,
    pub opening_channels_count: u32,
    channel_operations_lock: Arc<RwLock<HashMap<Address, Mutex<bool>>>>,
}

impl<T> TokenNetworkProxy<T>
where
    T: Transport + Send + Sync,
    T::Out: Send,
{
    pub fn new(
        web3: Web3<T>,
        gas_metadata: Arc<GasMetadata>,
        contract: Contract<T>,
        token_proxy: TokenProxy<T>,
    ) -> Self {
        Self {
            web3,
            gas_metadata,
            token_proxy,
            contract: TokenNetworkContract { inner: contract },
            opening_channels_count: 0,
            channel_operations_lock: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn new_channel(&mut self, account: Account<T>, partner: Address, settle_timeout: U256, block: H256) -> Result<U256> {
        let channel_operations_lock = self.channel_operations_lock.write().await;
        let _partner_lock_guard = channel_operations_lock.get(&partner).unwrap().lock().await;

        let open_channel_transaction = ChannelOpenTransaction {
            web3: self.web3.clone(),
            account: account.clone(),
            contract: self.contract.clone(),
            token_proxy: self.token_proxy.clone(),
            gas_metadata: self.gas_metadata.clone(),
        };

        self.opening_channels_count += 1;
        let channel_id = open_channel_transaction
            .execute(
                ChannelOpenTransactionParams {
                    partner,
                    settle_timeout,
                },
                block,
            )
            .await?;
        self.opening_channels_count -= 1;

        Ok(channel_id)
    }

    pub async fn approve_and_set_total_deposit(
        &self,
        account: Account<T>,
        channel_identifier: U256,
        partner: Address,
        total_deposit: U256,
        block_hash: H256,
    ) -> Result<()> {
        let set_total_deposit_transaction = ChannelSetTotalDepositTransaction {
            web3: self.web3.clone(),
            account: account.clone(),
            contract: self.contract.clone(),
            token: self.token_proxy.clone(),
            gas_metadata: self.gas_metadata.clone(),
        };

        Ok(set_total_deposit_transaction
            .execute(
                ChannelSetTotalDepositTransactionParams {
                    channel_identifier,
                    partner,
                    total_deposit,
                },
                block_hash,
            )
            .await?)
    }

    pub async fn address_by_token_address(&self, token_address: Address, block: H256) -> Result<Address> {
        self.contract.address_by_token_address(token_address, block).await
    }

    pub async fn safety_deprecation_switch(&self, block: H256) -> Result<bool> {
        self.contract.safety_deprecation_switch(block).await
    }

    pub async fn channel_participant_deposit_limit(&self, block: H256) -> Result<U256> {
        self.contract.channel_participant_deposit_limit(block).await
    }

    pub async fn get_channel_identifier(
        &self,
        participant1: Address,
        participant2: Address,
        block: H256,
    ) -> Result<Option<U256>> {
        self.contract
            .get_channel_identifier(participant1, participant2, block)
            .await
    }

    pub async fn participants_details(
        &self,
        channel_identifier: U256,
        address: Address,
        partner: Address,
        block: H256,
    ) -> Result<(ParticipantDetails, ParticipantDetails)> {
        self.contract
            .participants_details(channel_identifier, address, partner, block)
            .await
    }

    pub async fn settlement_timeout_min(&self, block: H256) -> Result<U256> {
        self.contract.settlement_timeout_min(block).await
    }

    pub async fn settlement_timeout_max(&self, block: H256) -> Result<U256> {
        self.contract.settlement_timeout_max(block).await
    }

    pub async fn token_network_deposit_limit(&self, block: H256) -> Result<U256> {
        self.contract.token_network_deposit_limit(block).await
    }

    #[allow(dead_code)]
    async fn participant_details(
        &self,
        channel_identifier: U256,
        address: Address,
        partner: Address,
        block: H256,
    ) -> Result<ParticipantDetails> {
        self.contract
            .participant_details(channel_identifier, address, partner, Some(block))
            .await
    }
}
