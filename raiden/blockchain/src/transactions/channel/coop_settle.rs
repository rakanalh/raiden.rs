use std::sync::Arc;

use ethabi::Token;
use raiden_primitives::types::{
	Address,
	BlockExpiration,
	BlockHash,
	BlockId,
	ChannelIdentifier,
	GasLimit,
	GasPrice,
	Signature,
	TokenAmount,
	TransactionHash,
};
use web3::{
	contract::{
		tokens::Tokenizable,
		Options,
	},
	types::{
		BlockNumber,
		U256,
	},
	Transport,
	Web3,
};

use crate::{
	contracts::GasMetadata,
	proxies::{
		Account,
		ProxyError,
		TokenNetworkProxy,
	},
	transactions::Transaction,
};

/// The container of data needed to be signed for withdraw
#[derive(Clone)]
pub struct WithdrawInput {
	pub initiator: Address,
	pub total_withdraw: TokenAmount,
	pub expiration_block: BlockExpiration,
	pub initiator_signature: Signature,
	pub partner_signature: Signature,
}

impl WithdrawInput {
	fn into_token(self) -> ethabi::Token {
		let expiration: U256 = self.expiration_block.into();

		Token::Tuple(vec![
			self.initiator.into_token(),
			self.total_withdraw.into_token(),
			expiration.into_token(),
			self.initiator_signature.into_token(),
			self.partner_signature.into_token(),
		])
	}
}

/// Cooperative settle parameters.
#[derive(Clone)]
pub struct ChannelCoopSettleTransactionParams {
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) withdraw_partner: WithdrawInput,
	pub(crate) withdraw_initiator: WithdrawInput,
}

/// Cooperative settle transaction type.
pub struct ChannelCoopSettleTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelCoopSettleTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = ChannelCoopSettleTransactionParams;
	type Data = ();

	async fn onchain_data(
		&self,
		_params: Self::Params,
		_at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		Ok(())
	}

	async fn validate_preconditions(
		&self,
		_params: Self::Params,
		_data: Self::Data,
		_block: BlockHash,
	) -> Result<(), ProxyError> {
		Ok(())
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

		let withdraw_initiator = params.withdraw_initiator;
		let withdraw_partner = params.withdraw_partner;

		let receipt = self
			.token_network
			.contract
			.signed_call_with_confirmations(
				"cooperativeSettle",
				(
					params.channel_identifier,
					withdraw_initiator.into_token(),
					withdraw_partner.into_token(),
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

		Ok(receipt.transaction_hash)
	}

	async fn validate_postconditions(
		&self,
		_params: Self::Params,
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
				self.gas_metadata.get("TokenNetwork.cooperativeSettle").into(),
				failed_at_blocknumber,
			)
			.await?;

		Err(ProxyError::Recoverable(format!(
			"Coop settle channel failed. Gas estimation failed for
            unknown reason. Reference block {} - {}",
			failed_at_blockhash, failed_at_blocknumber,
		)))
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ProxyError> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;

		let withdraw_initiator = params.withdraw_initiator;
		let withdraw_partner = params.withdraw_partner;

		self.token_network
			.contract
			.estimate_gas(
				"cooperativeSettle",
				(
					params.channel_identifier,
					withdraw_initiator.into_token(),
					withdraw_partner.into_token(),
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
			.map_err(ProxyError::ChainError)
	}
}
