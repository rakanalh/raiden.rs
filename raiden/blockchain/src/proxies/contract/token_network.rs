use derive_more::Deref;
use raiden_primitives::types::{
	Address,
	BalanceHash,
	BlockId,
	ChannelIdentifier,
	Locksroot,
	Nonce,
	SettleTimeout,
	TokenAddress,
	TokenAmount,
	H256,
	U256,
};
use raiden_state_machine::types::ChannelStatus;
use web3::{
	contract::{
		Contract,
		Options,
	},
	Transport,
};

use crate::proxies::{
	common::Result,
	ProxyError,
};

#[derive(Clone)]
pub struct ParticipantDetails {
	pub address: Address,
	pub deposit: TokenAmount,
	pub withdrawn: TokenAmount,
	pub is_closer: bool,
	pub balance_hash: BalanceHash,
	pub nonce: Nonce,
	pub locksroot: Locksroot,
	pub locked_amount: TokenAmount,
}

#[derive(Clone)]
pub struct ChannelData {
	pub channel_identifier: ChannelIdentifier,
	pub settle_block_number: U256,
	pub status: ChannelStatus,
}

#[derive(Clone, Deref)]
pub struct TokenNetworkContract<T: Transport> {
	pub(crate) inner: Contract<T>,
}

impl<T: Transport> TokenNetworkContract<T> {
	pub fn address(&self) -> Address {
		self.inner.address()
	}

	pub async fn get_channel_identifier(
		&self,
		participant1: Address,
		participant2: Address,
		block: H256,
	) -> Result<Option<U256>> {
		let channel_identifier: U256 = self
			.inner
			.query(
				"getChannelIdentifier",
				(participant1, participant2),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await?;

		if channel_identifier.is_zero() {
			return Ok(None)
		}

		Ok(Some(channel_identifier))
	}
	pub async fn address_by_token_address(
		&self,
		token_address: TokenAddress,
		block: H256,
	) -> Result<Address> {
		self.inner
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
		self.inner
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

	pub async fn channel_participant_deposit_limit(&self, block: H256) -> Result<U256> {
		self.inner
			.query(
				"channel_participant_deposit_limit",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map_err(Into::into)
	}

	pub async fn participants_details(
		&self,
		channel_identifier: U256,
		address: Address,
		partner: Address,
		block: H256,
	) -> Result<(ParticipantDetails, ParticipantDetails)> {
		let our_data = self
			.participant_details(channel_identifier, address, partner, Some(block))
			.await?;
		let partner_data = self
			.participant_details(channel_identifier, partner, address, Some(block))
			.await?;
		Ok((our_data, partner_data))
	}

	pub async fn channel_details(
		&self,
		channel_identifier: Option<U256>,
		address: Address,
		partner: Address,
		block: H256,
	) -> Result<ChannelData> {
		let channel_identifier = channel_identifier.unwrap_or(
			self.get_channel_identifier(address, partner, block)
				.await?
				.ok_or(ProxyError::BrokenPrecondition("Channel does not exist".to_string()))?,
		);

		let (settle_block_number, status) = self
			.inner
			.query(
				"getChannelInfo",
				(channel_identifier, address, partner),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await?;

		Ok(ChannelData {
			channel_identifier,
			settle_block_number,
			status: match status {
				1 => ChannelStatus::Opened,
				2 => ChannelStatus::Closed,
				3 => ChannelStatus::Settled,
				4 => ChannelStatus::Removed,
				_ => ChannelStatus::Unusable,
			},
		})
	}

	pub async fn settlement_timeout_min(&self, block: H256) -> Result<SettleTimeout> {
		self.inner
			.query(
				"settlement_timeout_min",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map(|b: U256| b.as_u64().into())
			.map_err(Into::into)
	}

	pub async fn settlement_timeout_max(&self, block: H256) -> Result<SettleTimeout> {
		self.inner
			.query(
				"settlement_timeout_max",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map(|b: U256| b.as_u64().into())
			.map_err(Into::into)
	}

	pub async fn token_network_deposit_limit(&self, block: H256) -> Result<U256> {
		self.inner
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

	pub async fn participant_details(
		&self,
		channel_identifier: U256,
		address: Address,
		partner: Address,
		block: Option<H256>,
	) -> Result<ParticipantDetails> {
		let block = block.map(|b| BlockId::Hash(b));
		let data: (TokenAmount, TokenAmount, bool, BalanceHash, Nonce, Locksroot, TokenAmount) =
			self.inner
				.query(
					"getChannelParticipantInfo",
					(channel_identifier, partner, partner),
					None,
					Options::default(),
					block,
				)
				.await?;

		Ok(ParticipantDetails {
			address,
			deposit: data.0,
			withdrawn: data.1,
			is_closer: data.2,
			balance_hash: data.3,
			nonce: data.4,
			locksroot: data.5,
			locked_amount: data.6,
		})
	}
}
