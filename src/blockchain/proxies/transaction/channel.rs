use std::sync::Arc;

use web3::{
    contract::Options,
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

use crate::{
    blockchain::{
        contracts::GasMetadata,
        proxies::{
            common::Account,
            contract::{
                ChannelData,
                ParticipantDetails,
                TokenNetworkContract,
            },
            ProxyError,
            TokenProxy,
        },
    },
    state_machine::types::ChannelStatus,
};

use super::Transaction;

#[derive(Clone)]
pub struct ChannelOpenTransactionData {
    channel_identifier: Option<U256>,
    settle_timeout_min: U256,
    settle_timeout_max: U256,
    token_network_deposit_limit: U256,
    token_network_balance: U256,
    safety_deprecation_switch: bool,
}

#[derive(Clone)]
pub struct ChannelOpenTransactionParams {
    pub(crate) partner: Address,
    pub(crate) settle_timeout: U256,
}

pub struct ChannelOpenTransaction<T: Transport> {
    pub(crate) web3: Web3<T>,
    pub(crate) account: Account<T>,
    pub(crate) contract: TokenNetworkContract<T>,
    pub(crate) token_proxy: TokenProxy<T>,
    pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelOpenTransaction<T>
where
    T: Transport + Send + Sync,
    T::Out: Send,
{
    type Output = U256;
    type Params = ChannelOpenTransactionParams;
    type Data = ChannelOpenTransactionData;

    async fn onchain_data(&self, params: Self::Params, at_block_hash: H256) -> Result<Self::Data, ProxyError> {
        let settle_timeout_min = self.contract.settlement_timeout_min(at_block_hash).await?;
        let settle_timeout_max = self.contract.settlement_timeout_max(at_block_hash).await?;
        let token_network_deposit_limit = self.contract.token_network_deposit_limit(at_block_hash).await?;
        let token_network_balance = self
            .token_proxy
            .balance_of(self.account.address(), Some(at_block_hash))
            .await?;
        let safety_deprecation_switch = self.contract.safety_deprecation_switch(at_block_hash).await?;
        let channel_identifier = self
            .contract
            .get_channel_identifier(self.account.address(), params.partner, at_block_hash)
            .await?
            .ok_or(ProxyError::BrokenPrecondition("Block not found".to_string()))?;

        Ok(ChannelOpenTransactionData {
            channel_identifier: Some(channel_identifier),
            settle_timeout_min,
            settle_timeout_max,
            token_network_deposit_limit,
            token_network_balance,
            safety_deprecation_switch,
        })
    }

    async fn validate_preconditions(
        &self,
        params: Self::Params,
        data: Self::Data,
        _block: H256,
    ) -> Result<(), ProxyError> {
        if params.settle_timeout < data.settle_timeout_min || params.settle_timeout > data.settle_timeout_max {
            return Err(ProxyError::BrokenPrecondition(format!(
                "settle_timeout must be in range [{}, {}]. Value: {}",
                data.settle_timeout_min, data.settle_timeout_max, params.settle_timeout,
            )));
        }

        if let Some(channel_identifier) = data.channel_identifier {
            return Err(ProxyError::BrokenPrecondition(format!(
                "A channel with identifier: {} already exists with partner {}",
                channel_identifier, params.partner
            )));
        }

        if data.token_network_balance >= data.token_network_deposit_limit {
            return Err(ProxyError::BrokenPrecondition(format!(
                "Cannot open another channe, token network deposit limit reached",
            )));
        }

        if data.safety_deprecation_switch {
            return Err(ProxyError::BrokenPrecondition(format!(
                "This token network is deprecated",
            )));
        }

        Ok(())
    }

    async fn estimate_gas(&self, params: Self::Params, _data: Self::Data) -> Result<(U256, U256), ()> {
        let nonce = self.account.peek_next_nonce().await;
        let gas_price = self.web3.eth().gas_price().await.map_err(|_| ())?;

        self.contract
            .estimate_gas(
                "openChannel",
                (self.account.address(), params.partner, params.settle_timeout),
                self.account.address(),
                Options::with(|opt| {
                    opt.value = Some(U256::from(0));
                    opt.nonce = Some(nonce);
                    opt.gas_price = Some(gas_price);
                }),
            )
            .await
            .map(|estimate| (estimate, gas_price))
            .map_err(|_| ())
    }

    async fn submit(
        &self,
        params: Self::Params,
        _data: Self::Data,
        gas_estimate: U256,
        gas_price: U256,
    ) -> Result<Self::Output, ProxyError> {
        let nonce = self.account.next_nonce().await;

        let receipt = self
            .contract
            .signed_call_with_confirmations(
                "openChannel",
                (self.account.address(), params.partner, params.settle_timeout),
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
            .contract
            .get_channel_identifier(self.account.address(), params.partner, receipt.block_hash.unwrap())
            .await?
            .unwrap())
    }

    async fn validate_postconditions(&self, params: Self::Params, _block: H256) -> Result<Self::Output, ProxyError> {
        let failed_at = self
            .web3
            .eth()
            .block(BlockId::Number(BlockNumber::Latest))
            .await
            .map_err(ProxyError::Web3)?
            .ok_or(ProxyError::Recoverable("Block not found".to_string()))?;

        let failed_at_blocknumber = failed_at.number.unwrap();
        let failed_at_blockhash = failed_at.hash.unwrap();

        self.account
            .check_for_insufficient_eth(
                self.gas_metadata.get("TokenNetwork.openChannel").into(),
                failed_at_blocknumber,
            )
            .await?;

        let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

        if let Some(channel_identifier) = data.channel_identifier {
            return Err(ProxyError::Recoverable(format!(
                "A channel with identifier: {} already exists with partner {}",
                channel_identifier, params.partner
            )));
        }

        if data.token_network_balance >= data.token_network_deposit_limit {
            return Err(ProxyError::Recoverable(format!(
                "Cannot open another channe, token network deposit limit reached",
            )));
        }

        if data.safety_deprecation_switch {
            return Err(ProxyError::Recoverable(format!("This token network is deprecated",)));
        }

        Err(ProxyError::Recoverable(format!(
            "Creating a new channel failed. Gas estimation failed for
            unknown reason. Reference block {} - {}",
            failed_at_blockhash, failed_at_blocknumber,
        )))
    }
}

#[derive(Clone)]
pub struct ChannelSetTotalDepositTransactionData {
    pub(crate) channel_identifier: U256,
    pub(crate) amount_to_deposit: U256,
    pub(crate) channel_onchain_details: ChannelData,
    pub(crate) our_details: ParticipantDetails,
    pub(crate) partner_details: ParticipantDetails,
    pub(crate) network_balance: U256,
    pub(crate) safety_deprecation_switch: bool,
    pub(crate) token_network_deposit_limit: U256,
    pub(crate) channel_participant_deposit_limit: U256,
    pub(crate) network_total_deposit: U256,
}

#[derive(Clone)]
pub struct ChannelSetTotalDepositTransactionParams {
    pub(crate) channel_identifier: U256,
    pub(crate) partner: Address,
    pub(crate) total_deposit: U256,
}

pub struct ChannelSetTotalDepositTransaction<T: Transport> {
    pub(crate) web3: Web3<T>,
    pub(crate) account: Account<T>,
    pub(crate) contract: TokenNetworkContract<T>,
    pub(crate) token: TokenProxy<T>,
    pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelSetTotalDepositTransaction<T>
where
    T: Transport + Send + Sync,
    T::Out: Send,
{
    type Output = ();
    type Params = ChannelSetTotalDepositTransactionParams;
    type Data = ChannelSetTotalDepositTransactionData;

    async fn onchain_data(&self, params: Self::Params, at_block_hash: H256) -> Result<Self::Data, ProxyError> {
        let channel_identifier = self
            .contract
            .get_channel_identifier(self.account.address(), params.partner, at_block_hash)
            .await?
            .ok_or(ProxyError::BrokenPrecondition("Block not found".to_string()))?;

        let channel_onchain_details = self
            .contract
            .channel_details(
                Some(channel_identifier),
                self.account.address(),
                params.partner,
                at_block_hash,
            )
            .await?;

        let our_details = match self
            .contract
            .participant_details(
                channel_identifier,
                self.account.address(),
                params.partner,
                Some(at_block_hash),
            )
            .await
        {
            Ok(our_details) => our_details,
            Err(_) => {
                self.contract
                    .participant_details(channel_identifier, self.account.address(), params.partner, None)
                    .await?
            }
        };

        let partner_details = self
            .contract
            .participant_details(
                channel_identifier,
                params.partner,
                self.account.address(),
                Some(at_block_hash),
            )
            .await?;

        let network_balance = self
            .token
            .balance_of(self.account.address(), Some(at_block_hash))
            .await?;

        let safety_deprecation_switch = self.contract.safety_deprecation_switch(at_block_hash).await?;

        let token_network_deposit_limit = self.contract.token_network_deposit_limit(at_block_hash).await?;

        let channel_participant_deposit_limit = self.contract.channel_participant_deposit_limit(at_block_hash).await?;

        let network_total_deposit = self
            .token
            .balance_of(self.account.address(), Some(at_block_hash))
            .await?;

        let amount_to_deposit = params.total_deposit - our_details.deposit;

        Ok(ChannelSetTotalDepositTransactionData {
            channel_identifier,
            channel_onchain_details,
            amount_to_deposit,
            our_details,
            partner_details,
            network_balance,
            safety_deprecation_switch,
            token_network_deposit_limit,
            channel_participant_deposit_limit,
            network_total_deposit,
        })
    }

    async fn validate_preconditions(
        &self,
        params: Self::Params,
        data: Self::Data,
        at_block_hash: H256,
    ) -> Result<(), ProxyError> {
        if data.channel_identifier != params.channel_identifier {
            return Err(ProxyError::BrokenPrecondition(format!(
                "There is a channel open between \
                {} and {}. However the channel id \
                on-chain {} and the provided \
                id {} do not match.",
                self.account.address(),
                params.partner,
                params.channel_identifier,
                data.channel_identifier,
            )));
        }

        if data.safety_deprecation_switch {
            return Err(ProxyError::BrokenPrecondition(format!(
                "This token network has been deprecated."
            )));
        }

        if data.channel_onchain_details.status != ChannelStatus::Opened {
            return Err(ProxyError::BrokenPrecondition(format!(
                "The channel was not opened at the provided block \
                ({}). This call should never have been attempted.",
                at_block_hash
            )));
        }

        if params.total_deposit <= data.our_details.deposit {
            return Err(ProxyError::BrokenPrecondition(format!(
                "Current total deposit ({}) is already larger \
                than the requested total deposit amount ({})",
                data.our_details.deposit, params.total_deposit,
            )));
        }

        let (_, total_channel_deposit_overflow) = params.total_deposit.overflowing_add(data.partner_details.deposit);
        if total_channel_deposit_overflow {
            return Err(ProxyError::BrokenPrecondition(format!("Deposit overflow")));
        }

        if params.total_deposit > data.channel_participant_deposit_limit {
            return Err(ProxyError::BrokenPrecondition(format!(
                "Deposit of {} is larger than the \
                channel participant deposit limit",
                params.total_deposit,
            )));
        }

        if data.network_total_deposit + data.amount_to_deposit > data.token_network_deposit_limit {
            return Err(ProxyError::BrokenPrecondition(format!(
                "Deposit of {} will have \
                exceeded the token network deposit limit.",
                data.amount_to_deposit,
            )));
        }

        if data.network_balance < data.amount_to_deposit {
            return Err(ProxyError::BrokenPrecondition(format!(
                "new_total_deposit - previous_total_deposit =  {} can \
                not be larger than the available balance {}, \
                for token at address {}",
                data.amount_to_deposit,
                data.network_balance,
                self.account.address(),
            )));
        }

        Ok(())
    }

    async fn submit(
        &self,
        params: Self::Params,
        data: Self::Data,
        gas_estimate: U256,
        gas_price: U256,
    ) -> Result<Self::Output, ProxyError> {
        let allowance = data.amount_to_deposit + 1;
        self.token
            .approve(self.account.clone(), self.contract.address(), allowance)
            .await?;
        let nonce = self.account.next_nonce().await;

        self.contract
            .signed_call_with_confirmations(
                "setTotalDeposit",
                (
                    params.channel_identifier,
                    self.account.address(),
                    params.total_deposit,
                    params.partner,
                ),
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
        Ok(())
    }

    async fn validate_postconditions(
        &self,
        params: Self::Params,
        _at_block_hash: H256,
    ) -> Result<Self::Output, ProxyError> {
        let failed_at = self
            .web3
            .eth()
            .block(BlockId::Number(BlockNumber::Latest))
            .await
            .map_err(ProxyError::Web3)?
            .ok_or(ProxyError::Recoverable("Block not found".to_string()))?;

        let failed_at_blocknumber = failed_at.number.unwrap();
        let failed_at_blockhash = failed_at.hash.unwrap();

        self.account
            .check_for_insufficient_eth(
                self.gas_metadata.get("TokenNetwork.openChannel").into(),
                failed_at_blocknumber,
            )
            .await?;

        let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

        if data.channel_onchain_details.status == ChannelStatus::Closed {
            return Err(ProxyError::Recoverable(format!(
                "Deposit failed because the channel was closed meanwhile",
            )));
        }

        if data.channel_onchain_details.status == ChannelStatus::Settled {
            return Err(ProxyError::Recoverable(format!(
                "Deposit failed because the channel was settled meanwhile",
            )));
        }

        if data.channel_onchain_details.status == ChannelStatus::Removed {
            return Err(ProxyError::Recoverable(format!(
                "Deposit failed because the channel was settled and unlocked meanwhile",
            )));
        }

        let (_, total_channel_deposit_overflow) = params.total_deposit.overflowing_add(data.partner_details.deposit);
        if total_channel_deposit_overflow {
            return Err(ProxyError::Recoverable(format!("Deposit overflow")));
        }

        if data.our_details.deposit >= params.total_deposit {
            return Err(ProxyError::Recoverable(format!(
                "Requested total deposit was already performed"
            )));
        }

        if data.network_total_deposit + data.amount_to_deposit > data.token_network_deposit_limit {
            return Err(ProxyError::Recoverable(format!(
                "Deposit of {} will have \
                exceeded the token network deposit limit.",
                data.amount_to_deposit,
            )));
        }

        if params.total_deposit > data.channel_participant_deposit_limit {
            return Err(ProxyError::Recoverable(format!(
                "Deposit of {} is larger than the \
                channel participant deposit limit",
                params.total_deposit,
            )));
        }

        if data.network_balance < data.amount_to_deposit {
            return Err(ProxyError::Recoverable(format!(
                "new_total_deposit - previous_total_deposit =  {} can \
                not be larger than the available balance {}, \
                for token at address {}",
                data.amount_to_deposit,
                data.network_balance,
                self.account.address(),
            )));
        }

        let has_sufficient_balance = self
            .token
            .balance_of(self.contract.address(), Some(failed_at_blockhash))
            .await?
            < data.amount_to_deposit;
        if !has_sufficient_balance {
            return Err(ProxyError::Recoverable(format!(
                "The account does not have enough balance to complete the deposit"
            )));
        }

        let allowance = self
            .token
            .allowance(self.contract.address(), self.account.address(), failed_at_blockhash)
            .await?;

        if allowance < data.amount_to_deposit {
            return Err(ProxyError::Recoverable(format!(
                "The allowance of the {} deposit changed. \
                Check concurrent deposits \
                for the same token network but different proxies.",
                data.amount_to_deposit,
            )));
        }

        let latest_deposit = self
            .contract
            .participant_details(
                params.channel_identifier,
                self.account.address(),
                params.partner,
                Some(failed_at_blockhash),
            )
            .await?
            .deposit;
        if latest_deposit < params.total_deposit {
            return Err(ProxyError::Recoverable(format!("The tokens were not transferred")));
        }

        return Err(ProxyError::Recoverable(format!("deposit failed for an unknown reason")));
    }

    async fn estimate_gas(&self, params: Self::Params, _data: Self::Data) -> Result<(U256, U256), ()> {
        let nonce = self.account.peek_next_nonce().await;
        let gas_price = self.web3.eth().gas_price().await.map_err(|_| ())?;

        self.contract
            .estimate_gas(
                "setTotalDeposit",
                (
                    params.channel_identifier,
                    self.account.address(),
                    params.total_deposit,
                    params.partner,
                ),
                self.account.address(),
                Options::with(|opt| {
                    opt.value = Some(U256::from(0));
                    opt.nonce = Some(nonce);
                    opt.gas_price = Some(gas_price);
                }),
            )
            .await
            .map(|estimate| (estimate, gas_price))
            .map_err(|_| ())
    }
}
