use std::sync::Arc;

use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockId,
	GasLimit,
	GasPrice,
	SecretRegistryAddress,
	SettleTimeout,
	TokenAddress,
	TokenAmount,
	TokenNetworkAddress,
	TransactionHash,
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
		TokenNetworkRegistryProxy,
		TokenProxy,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct RegisterTokenTransactionData {
	pub(crate) controller: Address,
	pub(crate) already_registered: bool,
	pub(crate) settlement_timeout_min: SettleTimeout,
	pub(crate) settlement_timeout_max: SettleTimeout,
	pub(crate) secret_registry_address: SecretRegistryAddress,
	pub(crate) max_token_networks: U256,
	pub(crate) token_networks_created: U256,
}

#[derive(Clone)]
pub struct RegisterTokenTransactionParams {
	pub(crate) token_address: Address,
	pub(crate) channel_participant_deposit_limit: TokenAmount,
	pub(crate) token_network_deposit_limit: TokenAmount,
}

pub struct RegisterTokenTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network_registry: TokenNetworkRegistryProxy<T>,
	pub(crate) token: TokenProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for RegisterTokenTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = (TransactionHash, TokenNetworkAddress);
	type Params = RegisterTokenTransactionParams;
	type Data = RegisterTokenTransactionData;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let controller = self.token_network_registry.get_controller(at_block_hash).await?;
		let settlement_timeout_min =
			self.token_network_registry.settlement_timeout_min(at_block_hash).await?;
		let settlement_timeout_max =
			self.token_network_registry.settlement_timeout_max(at_block_hash).await?;
		let secret_registry_address =
			self.token_network_registry.get_secret_registry_address(at_block_hash).await?;
		let max_token_networks =
			self.token_network_registry.get_max_token_networks(at_block_hash).await?;
		let token_networks_created =
			self.token_network_registry.get_token_networks_created(at_block_hash).await?;
		let already_registered = self
			.token_network_registry
			.get_token_network(params.token_address, at_block_hash)
			.await? != TokenNetworkAddress::zero();

		// If the token contract does not support this, then the token
		// is an invalid contract.
		let _ = self.token.total_supply(Some(at_block_hash)).await?;

		Ok(RegisterTokenTransactionData {
			controller,
			settlement_timeout_min,
			settlement_timeout_max,
			secret_registry_address,
			max_token_networks,
			token_networks_created,
			already_registered,
		})
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_at_block_hash: BlockHash,
	) -> Result<(), ProxyError> {
		if params.token_address == TokenAddress::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Calling to register a token with a zero address will fail",
			)))
		}
		if params.channel_participant_deposit_limit == TokenAmount::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Participant deposit limit must be larger than zero",
			)))
		}

		if params.token_network_deposit_limit == TokenAmount::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Token network deposit limit must be larger than zero",
			)))
		}

		if params.channel_participant_deposit_limit > params.token_network_deposit_limit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Participamt deposit limit must be smaller than the network deposit limit",
			)))
		}

		if data.already_registered {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Token network with provided token address is already registered",
			)))
		}

		if data.controller == Address::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The controller property for the token network registry is invalid",
			)))
		}

		if data.secret_registry_address == Address::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The secret registry address for the token network is invalid",
			)))
		}

		if data.settlement_timeout_min == SettleTimeout::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The minimum settlement timeout for the token network should be larger than zero",
			)))
		}

		if data.settlement_timeout_max == SettleTimeout::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The maximim settlement timeout for the token network should be larger than zero",
			)))
		}

		if data.settlement_timeout_min >= data.settlement_timeout_max {
			return Err(ProxyError::BrokenPrecondition(format!(
				"The maximum settlement timeout for the token network should be larger than the minimum",
			)))
		}

		Ok(())
	}

	async fn submit(
		&self,
		params: Self::Params,
		_data: Self::Data,
		gas_estimate: GasLimit,
		gas_price: GasPrice,
	) -> Result<Self::Output, ProxyError> {
		let nonce = self.account.next_nonce().await;
		let receipt = self
			.token
			.contract
			.signed_call_with_confirmations(
				"createERC20TokenNetwork",
				(
					params.token_address,
					params.channel_participant_deposit_limit,
					params.token_network_deposit_limit,
				),
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
		let succeeded_at_blockhash = receipt.block_hash.expect("Receipt should include blockhash");
		if let Ok(token_network_address) = self
			.token_network_registry
			.get_token_network(params.token_address, succeeded_at_blockhash)
			.await
		{
			Ok((receipt.transaction_hash, token_network_address))
		} else {
			return Err(ProxyError::Unrecoverable(format!(
				"createERC20TokenNetwork succeeded but token network address is null"
			)))
		}
	}

	async fn validate_postconditions(
		&self,
		params: Self::Params,
		_at_block_hash: BlockHash,
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
				self.gas_metadata.get("TokenNetworkRegistry.createERC20TokenNetwork").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		if params.token_address == TokenAddress::zero() {
			return Err(ProxyError::Recoverable(format!(
				"Calling to register a token with a zero address will fail",
			)))
		}
		if params.channel_participant_deposit_limit == TokenAmount::zero() {
			return Err(ProxyError::Recoverable(format!(
				"Participant deposit limit must be larger than zero",
			)))
		}

		if params.token_network_deposit_limit == TokenAmount::zero() {
			return Err(ProxyError::Recoverable(format!(
				"Token network deposit limit must be larger than zero",
			)))
		}

		if params.channel_participant_deposit_limit > params.token_network_deposit_limit {
			return Err(ProxyError::Recoverable(format!(
				"Participamt deposit limit must be smaller than the network deposit limit",
			)))
		}

		if data.already_registered {
			return Err(ProxyError::Recoverable(format!(
				"Token network with provided token address is already registered",
			)))
		}

		if data.controller == Address::zero() {
			return Err(ProxyError::Recoverable(format!(
				"The controller property for the token network registry is invalid",
			)))
		}

		if data.secret_registry_address == Address::zero() {
			return Err(ProxyError::Recoverable(format!(
				"The secret registry address for the token network is invalid",
			)))
		}

		if data.settlement_timeout_min == SettleTimeout::zero() {
			return Err(ProxyError::Recoverable(format!(
				"The minimum settlement timeout for the token network should be larger than zero",
			)))
		}

		if data.settlement_timeout_max == SettleTimeout::zero() {
			return Err(ProxyError::Recoverable(format!(
				"The maximim settlement timeout for the token network should be larger than zero",
			)))
		}

		if data.settlement_timeout_min >= data.settlement_timeout_max {
			return Err(ProxyError::Recoverable(format!(
				"The maximum settlement timeout for the token network should be larger than the minimum",
			)))
		}

		if data.token_networks_created >= data.max_token_networks {
			return Err(ProxyError::Recoverable(format!(
				"The number of existing token networks reached the maximum allowed",
			)))
		}

		return Err(ProxyError::Recoverable(format!("deposit failed for an unknown reason")))
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ()> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(|_| ())?;

		self.token
			.contract
			.estimate_gas(
				"createERC20TokenNetwork",
				(
					params.token_address,
					params.channel_participant_deposit_limit,
					params.token_network_deposit_limit,
				),
				self.account.address(),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
			)
			.await
			.map(|estimate| (estimate, gas_price))
			.map_err(|_| ())
	}
}
