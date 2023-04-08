use std::{
	collections::HashMap,
	sync::Arc,
};

use raiden_primitives::types::{
	Address,
	BalanceHash,
	BlockExpiration,
	BlockHash,
	BlockId,
	ChainID,
	ChannelIdentifier,
	LockedAmount,
	Locksroot,
	Nonce,
	SettleTimeout,
	Signature,
	TokenAddress,
	TokenAmount,
	TransactionHash,
	H256,
	U256,
};
use raiden_state_machine::types::{
	ChannelStatus,
	PendingLocksState,
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
	Transport,
	Web3,
};

use super::{
	common::{
		Account,
		Result,
	},
	ProxyError,
	TokenProxy,
};
use crate::{
	contracts::GasMetadata,
	transactions::{
		ChannelCloseTransaction,
		ChannelCloseTransactionParams,
		ChannelCoopSettleTransaction,
		ChannelCoopSettleTransactionParams,
		ChannelOpenTransaction,
		ChannelOpenTransactionParams,
		ChannelSetTotalDepositTransaction,
		ChannelSetTotalDepositTransactionParams,
		ChannelSetTotalWithdrawTransaction,
		ChannelSetTotalWithdrawTransactionParams,
		ChannelSettleTransaction,
		ChannelSettleTransactionParams,
		ChannelUnlockTransaction,
		ChannelUnlockTransactionParams,
		ChannelUpdateTransferTransaction,
		ChannelUpdateTransferTransactionParams,
		Transaction,
		WithdrawInput,
	},
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
	pub locked_amount: LockedAmount,
}

#[derive(Clone)]
pub struct ParticipantsDetails {
	pub our_details: ParticipantDetails,
	pub partner_details: ParticipantDetails,
}

#[derive(Clone)]
pub struct ChannelData {
	pub channel_identifier: ChannelIdentifier,
	pub settle_block_number: U256,
	pub status: ChannelStatus,
}

#[derive(Clone)]
pub struct TokenNetworkProxy<T: Transport> {
	web3: Web3<T>,
	gas_metadata: Arc<GasMetadata>,
	token_proxy: TokenProxy<T>,
	pub(crate) contract: Contract<T>,
	pub(super) opening_channels_count: u32,
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
			contract,
			opening_channels_count: 0,
			channel_operations_lock: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	pub async fn new_channel(
		&mut self,
		account: Account<T>,
		partner: Address,
		settle_timeout: SettleTimeout,
		block: BlockHash,
	) -> Result<ChannelIdentifier> {
		let mut channel_operations_lock = self.channel_operations_lock.write().await;
		let _partner_lock_guard = match channel_operations_lock.get(&partner) {
			Some(mutex) => mutex.lock().await,
			None => {
				channel_operations_lock.insert(partner, Mutex::new(true));
				channel_operations_lock.get(&partner).unwrap().lock().await
			},
		};

		let open_channel_transaction = ChannelOpenTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			token_network: self.clone(),
			token_proxy: self.token_proxy.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		self.opening_channels_count += 1;
		let channel_id = open_channel_transaction
			.execute(ChannelOpenTransactionParams { partner, settle_timeout }, block)
			.await?;
		self.opening_channels_count -= 1;

		Ok(channel_id)
	}

	pub async fn close(
		&self,
		account: Account<T>,
		partner: Address,
		channel_identifier: ChannelIdentifier,
		nonce: Nonce,
		balance_hash: BalanceHash,
		additional_hash: H256,
		non_closing_signature: Signature,
		closing_signature: Signature,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		let close_channel_transaction = ChannelCloseTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			token_network: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		close_channel_transaction
			.execute(
				ChannelCloseTransactionParams {
					channel_identifier,
					nonce,
					partner,
					balance_hash,
					additional_hash,
					non_closing_signature,
					closing_signature,
				},
				block_hash,
			)
			.await
	}

	pub async fn approve_and_set_total_deposit(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		partner: Address,
		total_deposit: TokenAmount,
		block_hash: BlockHash,
	) -> Result<()> {
		let set_total_deposit_transaction = ChannelSetTotalDepositTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			token_network: self.clone(),
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

	pub async fn set_total_withdraw(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		total_withdraw: TokenAmount,
		participant: Address,
		partner: Address,
		participant_signature: Signature,
		partner_signature: Signature,
		expiration_block: BlockExpiration,
		block_hash: BlockHash,
	) -> Result<()> {
		let set_total_withdraw_transaction = ChannelSetTotalWithdrawTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			token_network: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		let params = ChannelSetTotalWithdrawTransactionParams {
			channel_identifier,
			participant,
			participant2: partner,
			participant_signature,
			participant2_signature: partner_signature,
			total_withdraw,
			expiration_block,
		};
		Ok(set_total_withdraw_transaction.execute(params, block_hash).await?)
	}

	pub async fn update_transfer(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		nonce: Nonce,
		partner: Address,
		balance_hash: BalanceHash,
		additional_hash: H256,
		closing_signature: Signature,
		non_closing_signature: Signature,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		let transaction = ChannelUpdateTransferTransaction {
			web3: self.web3.clone(),
			account,
			token_network: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		Ok(transaction
			.execute(
				ChannelUpdateTransferTransactionParams {
					channel_identifier,
					nonce,
					partner,
					balance_hash,
					additional_hash,
					closing_signature,
					non_closing_signature,
				},
				block_hash,
			)
			.await?)
	}

	pub async fn settle(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		our_transferred_amount: TokenAmount,
		our_locked_amount: LockedAmount,
		our_locksroot: Locksroot,
		partner_address: Address,
		partner_transferred_amount: TokenAmount,
		partner_locked_amount: LockedAmount,
		partner_locksroot: Locksroot,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		let settle_transaction = ChannelSettleTransaction {
			web3: self.web3.clone(),
			account,
			token_network: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		Ok(settle_transaction
			.execute(
				ChannelSettleTransactionParams {
					channel_identifier,
					our_transferred_amount,
					our_locked_amount,
					our_locksroot,
					partner_address,
					partner_transferred_amount,
					partner_locked_amount,
					partner_locksroot,
				},
				block_hash,
			)
			.await?)
	}

	pub async fn unlock(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		sender: Address,
		receiver: Address,
		pending_locks: PendingLocksState,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		let unlock_transaction = ChannelUnlockTransaction {
			web3: self.web3.clone(),
			account,
			token_network: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		Ok(unlock_transaction
			.execute(
				ChannelUnlockTransactionParams {
					channel_identifier,
					sender,
					receiver,
					pending_locks,
				},
				block_hash,
			)
			.await?)
	}

	pub async fn coop_settle(
		&self,
		account: Account<T>,
		channel_identifier: ChannelIdentifier,
		withdraw_partner: WithdrawInput,
		withdraw_initiator: WithdrawInput,
		block_hash: BlockHash,
	) -> Result<TransactionHash> {
		let coop_settle_transaction = ChannelCoopSettleTransaction {
			web3: self.web3.clone(),
			account,
			token_network: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		Ok(coop_settle_transaction
			.execute(
				ChannelCoopSettleTransactionParams {
					channel_identifier,
					withdraw_partner,
					withdraw_initiator,
				},
				block_hash,
			)
			.await?)
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
			return Ok(None)
		}

		Ok(Some(channel_identifier))
	}
	pub async fn address_by_token_address(
		&self,
		token_address: TokenAddress,
		block: H256,
	) -> Result<Address> {
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

	pub async fn channel_participant_deposit_limit(&self, block: H256) -> Result<U256> {
		self.contract
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
		block: Option<H256>,
	) -> Result<ParticipantsDetails> {
		let our_details =
			self.participant_details(channel_identifier, address, partner, block).await?;
		let partner_details =
			self.participant_details(channel_identifier, partner, address, block).await?;
		Ok(ParticipantsDetails { our_details, partner_details })
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
			.contract
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

	pub async fn chain_id(&self, block: H256) -> Result<ChainID> {
		self.contract
			.query("chain_id", (), None, Options::default(), Some(BlockId::Hash(block)))
			.await
			.map(|b: U256| b.into())
			.map_err(Into::into)
	}

	pub async fn settlement_timeout_min(&self, block: H256) -> Result<SettleTimeout> {
		self.contract
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
		self.contract
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

	pub async fn participant_details(
		&self,
		channel_identifier: U256,
		address: Address,
		partner: Address,
		block: Option<H256>,
	) -> Result<ParticipantDetails> {
		let block = block.map(|b| BlockId::Hash(b));
		let data: (TokenAmount, TokenAmount, bool, BalanceHash, Nonce, Locksroot, TokenAmount) =
			self.contract
				.query(
					"getChannelParticipantInfo",
					(channel_identifier, address, partner),
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
