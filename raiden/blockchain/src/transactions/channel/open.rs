use std::sync::Arc;

use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockId,
	ChannelIdentifier,
	GasLimit,
	GasPrice,
	SettleTimeout,
	TokenAmount,
	U256,
};
use web3::{
	contract::Options,
	types::BlockNumber,
	Transport,
	Web3,
};

use crate::{
	contracts::GasMetadata,
	proxies::{
		Account,
		ProxyError,
		TokenNetworkProxy,
		TokenProxy,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct ChannelOpenTransactionData {
	channel_identifier: Option<ChannelIdentifier>,
	settle_timeout_min: SettleTimeout,
	settle_timeout_max: SettleTimeout,
	token_network_deposit_limit: TokenAmount,
	token_network_balance: TokenAmount,
	safety_deprecation_switch: bool,
}

#[derive(Clone)]
pub struct ChannelOpenTransactionParams {
	pub(crate) partner: Address,
	pub(crate) settle_timeout: SettleTimeout,
}

pub struct ChannelOpenTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) token_proxy: TokenProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelOpenTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = ChannelIdentifier;
	type Params = ChannelOpenTransactionParams;
	type Data = ChannelOpenTransactionData;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let settle_timeout_min = self.token_network.settlement_timeout_min(at_block_hash).await?;
		let settle_timeout_max = self.token_network.settlement_timeout_max(at_block_hash).await?;
		let token_network_deposit_limit =
			self.token_network.token_network_deposit_limit(at_block_hash).await?;
		let token_network_balance =
			self.token_proxy.balance_of(self.account.address(), Some(at_block_hash)).await?;
		let safety_deprecation_switch =
			self.token_network.safety_deprecation_switch(at_block_hash).await?;
		let channel_identifier = self
			.token_network
			.get_channel_identifier(self.account.address(), params.partner, at_block_hash)
			.await?;

		Ok(ChannelOpenTransactionData {
			channel_identifier,
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
		_block: BlockHash,
	) -> Result<(), ProxyError> {
		if params.settle_timeout < data.settle_timeout_min ||
			params.settle_timeout > data.settle_timeout_max
		{
			return Err(ProxyError::BrokenPrecondition(format!(
				"settle_timeout must be in range [{}, {}]. Value: {}",
				data.settle_timeout_min, data.settle_timeout_max, params.settle_timeout,
			)))
		}

		if let Some(channel_identifier) = data.channel_identifier {
			return Err(ProxyError::BrokenPrecondition(format!(
				"A channel with identifier: {} already exists with partner {}",
				channel_identifier, params.partner
			)))
		}

		if data.token_network_balance >= data.token_network_deposit_limit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Cannot open another channe, token network deposit limit reached",
			)))
		}

		if data.safety_deprecation_switch {
			return Err(ProxyError::BrokenPrecondition(format!("This token network is deprecated",)))
		}

		Ok(())
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ProxyError> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;

		let settle_timeout: U256 = params.settle_timeout.into();
		self.token_network
			.contract
			.estimate_gas(
				"openChannel",
				(self.account.address(), params.partner, settle_timeout),
				self.account.address(),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
			)
			.await
			.map(|estimate| (estimate, gas_price))
			.map_err(ProxyError::ChainError)
	}

	async fn submit(
		&self,
		params: Self::Params,
		_data: Self::Data,
		gas_estimate: GasLimit,
		gas_price: GasPrice,
	) -> Result<Self::Output, ProxyError> {
		let nonce = self.account.peek_next_nonce().await;
		self.account.next_nonce().await;

		let settle_timeout: U256 = params.settle_timeout.into();
		let receipt = self
			.token_network
			.contract
			.signed_call_with_confirmations(
				"openChannel",
				(self.account.address(), params.partner, settle_timeout),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.gas = Some(gas_estimate);
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
				1,
				self.account.private_key(),
			)
			.await?;

		Ok(self
			.token_network
			.get_channel_identifier(
				self.account.address(),
				params.partner,
				receipt.block_hash.unwrap(),
			)
			.await?
			.unwrap())
	}

	async fn validate_postconditions(
		&self,
		params: Self::Params,
		_block: BlockHash,
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

		if let Some(channel_identifier) = data.channel_identifier {
			return Err(ProxyError::Recoverable(format!(
				"A channel with identifier: {} already exists with partner {}",
				channel_identifier, params.partner
			)))
		}

		if data.token_network_balance >= data.token_network_deposit_limit {
			return Err(ProxyError::Recoverable(format!(
				"Cannot open another channe, token network deposit limit reached",
			)))
		}

		if data.safety_deprecation_switch {
			return Err(ProxyError::Recoverable(format!("This token network is deprecated",)))
		}

		Err(ProxyError::Recoverable(format!(
			"Creating a new channel failed. Gas estimation failed for
            unknown reason. Reference block {} - {}",
			failed_at_blockhash, failed_at_blocknumber,
		)))
	}
}
