use std::sync::Arc;

use raiden_primitives::{
	constants::EMPTY_SIGNATURE,
	packing::{
		pack_balance_proof,
		pack_balance_proof_message,
	},
	signing::recover,
	types::{
		Address,
		BalanceHash,
		BlockHash,
		BlockId,
		CanonicalIdentifier,
		ChainID,
		ChannelIdentifier,
		GasLimit,
		GasPrice,
		MessageTypeId,
		Nonce,
		Signature,
		TransactionHash,
		H256,
	},
};
use raiden_state_machine::types::ChannelStatus;
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
		ChannelData,
		ProxyError,
		TokenNetworkProxy,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct ChannelCloseTransactionData {
	chain_id: ChainID,
	channel_onchain_details: ChannelData,
}

#[derive(Clone)]
pub struct ChannelCloseTransactionParams {
	pub(crate) channel_identifier: ChannelIdentifier,
	pub(crate) nonce: Nonce,
	pub(crate) partner: Address,
	pub(crate) balance_hash: BalanceHash,
	pub(crate) additional_hash: H256,
	pub(crate) non_closing_signature: Signature,
	pub(crate) closing_signature: Signature,
}

pub struct ChannelCloseTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) token_network: TokenNetworkProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for ChannelCloseTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = ChannelCloseTransactionParams;
	type Data = ChannelCloseTransactionData;

	async fn onchain_data(
		&self,
		params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let channel_onchain_details = self
			.token_network
			.channel_details(
				Some(params.channel_identifier),
				self.account.address(),
				params.partner,
				at_block_hash,
			)
			.await?;

		let chain_id = self.token_network.chain_id(at_block_hash).await?;

		Ok(ChannelCloseTransactionData { channel_onchain_details, chain_id })
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_block: BlockHash,
	) -> Result<(), ProxyError> {
		let canonical_identifier = CanonicalIdentifier {
			chain_identifier: data.chain_id,
			token_network_address: self.token_network.contract.address(),
			channel_identifier: params.channel_identifier,
		};

		let our_signed_data = pack_balance_proof_message(
			params.nonce,
			params.balance_hash,
			params.additional_hash,
			canonical_identifier.clone(),
			MessageTypeId::BalanceProof,
			params.non_closing_signature.clone(),
		);

		let our_recovered_address = recover(&our_signed_data.0, &params.closing_signature.0)
			.map_err(|_| {
				ProxyError::Unrecoverable("Could not verify the closing signature".to_owned())
			})?;

		if our_recovered_address != self.account.address() {
			return Err(ProxyError::Unrecoverable("Invalid closing signature".to_owned()))
		}

		if params.non_closing_signature != *EMPTY_SIGNATURE {
			let partner_signed_data = pack_balance_proof(
				params.nonce,
				params.balance_hash,
				params.additional_hash,
				canonical_identifier,
				MessageTypeId::BalanceProof,
			);

			let partner_recovered_address =
				recover(&partner_signed_data.0, &params.non_closing_signature.0).map_err(|_| {
					ProxyError::Unrecoverable("Could not verify non-closing-signature".to_owned())
				})?;

			if partner_recovered_address != params.partner {
				return Err(ProxyError::Unrecoverable("Invalid non-closing signature".to_owned()))
			}
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
		let nonce = self.account.peek_next_nonce().await;
		self.account.next_nonce().await;

		let receipt = self
			.token_network
			.contract
			.signed_call_with_confirmations(
				"closeChannel",
				(
					params.channel_identifier,
					params.partner,
					self.account.address(),
					params.balance_hash,
					params.nonce,
					params.additional_hash,
					params.non_closing_signature,
					params.closing_signature,
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
				self.gas_metadata.get("TokenNetwork.closeChannel").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		if data.channel_onchain_details.status == ChannelStatus::Closed {
			return Err(ProxyError::Recoverable(
				"Cannot call close on a channel that has been closed already".to_owned(),
			))
		}

		Err(ProxyError::Recoverable(format!(
			"Close channel failed. Gas estimation failed for
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

		self.token_network
			.contract
			.estimate_gas(
				"closeChannel",
				(
					params.channel_identifier,
					self.account.address(),
					params.partner,
					params.balance_hash,
					params.nonce,
					params.additional_hash,
					params.non_closing_signature,
					params.closing_signature,
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
