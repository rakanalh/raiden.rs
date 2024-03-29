use std::sync::Arc;

use derive_more::Display;
use ethabi::Token;
use raiden_primitives::{
	traits::Checksum,
	types::{
		Address,
		BlockNumber,
		CanonicalIdentifier,
		Locksroot,
		RevealTimeout,
		SettleTimeout,
	},
};
use raiden_state_machine::{
	storage::StateStorage,
	types::{
		ChainState,
		ChannelState,
		ContractReceiveChannelBatchUnlock,
		ContractReceiveChannelClosed,
		ContractReceiveChannelDeposit,
		ContractReceiveChannelOpened,
		ContractReceiveChannelSettled,
		ContractReceiveChannelWithdraw,
		ContractReceiveTokenNetworkCreated,
		ContractReceiveUpdateTransfer,
		MediationFeeConfig,
		StateChange,
		TokenNetworkState,
		TransactionChannelDeposit,
		TransactionExecutionStatus,
		TransactionResult,
		UpdateServicesAddresses,
	},
	views,
};
use thiserror::Error;
use tracing::{
	debug,
	trace,
};

use super::events::Event;

/// Error for decoding log
#[derive(Error, Debug, Display)]
pub struct DecodeError(String);

/// Decoder result type.
pub type Result<T> = std::result::Result<T, DecodeError>;

/// Decodes an event info a state change.
pub struct EventDecoder {
	mediation_config: MediationFeeConfig,
	default_reveal_timeout: RevealTimeout,
}

impl EventDecoder {
	/// Returns a new instance of `EventDecoder`.
	pub fn new(
		mediation_config: MediationFeeConfig,
		default_reveal_timeout: RevealTimeout,
	) -> Self {
		Self { mediation_config, default_reveal_timeout }
	}

	/// Converts event into a state change.
	pub async fn as_state_change(
		&self,
		event: Event,
		chain_state: &ChainState,
		storage: Arc<StateStorage>,
	) -> Result<Option<StateChange>> {
		debug!(message = "Decoding blockchain event", name = event.name);
		match event.name.as_ref() {
			"TokenNetworkCreated" => self.token_network_created(event),
			"ChannelOpened" => self.channel_opened(chain_state, event),
			"ChannelNewDeposit" => self.channel_deposit(chain_state, event),
			"ChannelWithdraw" => self.channel_withdraw(chain_state, event),
			"ChannelClosed" => self.channel_closed(chain_state, event),
			"ChannelSettled" => self.channel_settled(chain_state, event).await,
			"ChannelUnlocked" => self.channel_unlocked(chain_state, event, storage).await,
			"NonClosingBalanceProofUpdated" =>
				self.channel_non_closing_balance_proof_updated(chain_state, event),
			"RegisteredService" => self.registered_service(chain_state, event),
			_ => Err(DecodeError(format!("Event {} unknown", event.name))),
		}
	}

	/// Converts event into `ContractReceiveTokenNetworkCreated` state change.
	fn token_network_created(&self, event: Event) -> Result<Option<StateChange>> {
		let token_address = match event.data.get("token_address") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid token address",
					event.name,
				))),
		};
		let token_network_address = match event.data.get("token_network_address") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid token network address",
					event.name,
				))),
		};
		let token_network = TokenNetworkState::new(token_network_address, token_address);
		let token_network_registry_address = event.address;
		Ok(Some(
			ContractReceiveTokenNetworkCreated {
				transaction_hash: Some(event.transaction_hash),
				block_number: event.block_number,
				block_hash: event.block_hash,
				token_network_registry_address,
				token_network,
			}
			.into(),
		))
	}

	/// Converts event into `UpdateServicesAddresses` state change.
	fn registered_service(
		&self,
		_chain_state: &ChainState,
		event: Event,
	) -> Result<Option<StateChange>> {
		let service_address = match event.data.get("service_address") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid service address",
					event.name,
				))),
		};
		let valid_till: u64 = match event.data.get("valid_till") {
			Some(Token::Uint(valid_till)) => valid_till.clone().as_u64(),
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid `valid_till` block",
					event.name,
				))),
		};
		Ok(Some(
			UpdateServicesAddresses { service: service_address, valid_till: valid_till.into() }
				.into(),
		))
	}

	/// Converts event into `ContractReceiveChannelOpened` state change.
	fn channel_opened(
		&self,
		chain_state: &ChainState,
		event: Event,
	) -> Result<Option<StateChange>> {
		let channel_identifier = match event.data.get("channel_identifier") {
			Some(Token::Uint(identifier)) => *identifier,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid channel identifier",
					event.name,
				))),
		};
		let participant1 = match event.data.get("participant1") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!("{} event has an invalid participant1", event.name))),
		};
		let participant2 = match event.data.get("participant2") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!("{} event has an invalid participant2", event.name))),
		};
		let settle_timeout = match event.data.get("settle_timeout") {
			Some(Token::Uint(timeout)) => SettleTimeout::from(timeout.clone().low_u64()),
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid settle timeout",
					event.name,
				))),
		};

		let our_address = chain_state.our_address;
		let partner_address: Address = if our_address == participant1 {
			participant2
		} else if our_address == participant2 {
			participant1
		} else {
			trace!(
				message = "Ignore channel opened",
				participant1 = participant1.checksum(),
				participant2 = participant2.checksum()
			);
			return Ok(None)
		};

		let token_network_address = event.address;
		let token_network_registry = views::get_token_network_registry_by_token_network_address(
			chain_state,
			token_network_address,
		)
		.ok_or_else(|| {
			DecodeError(format!("{} event has an unknown Token network address", event.name))
		})?;
		let token_network = views::get_token_network_by_address(chain_state, token_network_address)
			.ok_or_else(|| {
				DecodeError(format!("{} event has an unknown Token network address", event.name))
			})?;
		let token_address = token_network.token_address;
		let reveal_timeout = RevealTimeout::from(self.default_reveal_timeout);
		let open_transaction = TransactionExecutionStatus {
			started_block_number: Some(BlockNumber::from(0)),
			finished_block_number: Some(event.block_number),
			result: Some(TransactionResult::Success),
		};
		let channel_state = ChannelState::new(
			CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address,
				channel_identifier,
			},
			token_address,
			token_network_registry.address,
			our_address,
			partner_address,
			reveal_timeout,
			settle_timeout,
			open_transaction,
			self.mediation_config.clone(),
		)
		.map_err(|e| DecodeError(format!("{:?}", e)))?;

		Ok(Some(
			ContractReceiveChannelOpened {
				transaction_hash: Some(event.transaction_hash),
				block_number: event.block_number,
				block_hash: event.block_hash,
				channel_state,
			}
			.into(),
		))
	}

	/// Converts event into `ContractReceiveChannelDeposit` state change.
	fn channel_deposit(
		&self,
		chain_state: &ChainState,
		event: Event,
	) -> Result<Option<StateChange>> {
		let token_network_address = event.address;
		let channel_identifier = match event.data.get("channel_identifier") {
			Some(Token::Uint(identifier)) => *identifier,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid channel identifier",
					event.name,
				))),
		};
		let participant = match event.data.get("participant") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!("{} event has an invalid participant", event.name))),
		};

		// Check if we have a channel with participant
		if views::get_channel_by_token_network_and_partner(
			chain_state,
			token_network_address,
			participant,
		)
		.is_none()
		{
			// No channel with `participant`. Check if `participant is our address.
			if participant != chain_state.our_address {
				trace!(message = "Ignore channel deposit", participant = participant.checksum());
				return Ok(None)
			}
		}

		let total_deposit = match event.data.get("total_deposit") {
			Some(Token::Uint(total_deposit)) => *total_deposit,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid total deposit",
					event.name,
				))),
		};
		let channel_deposit = ContractReceiveChannelDeposit {
			canonical_identifier: CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address,
				channel_identifier,
			},
			deposit_transaction: TransactionChannelDeposit {
				participant_address: participant,
				contract_balance: total_deposit,
				deposit_block_number: event.block_number,
			},
			fee_config: self.mediation_config.clone(),
			transaction_hash: Some(event.transaction_hash),
			block_number: event.block_number,
			block_hash: event.block_hash,
		};
		Ok(Some(channel_deposit.into()))
	}

	/// Converts event into `ContractReceiveChannelWithdraw` state change.
	fn channel_withdraw(
		&self,
		chain_state: &ChainState,
		event: Event,
	) -> Result<Option<StateChange>> {
		let token_network_address = event.address;
		let channel_identifier = match event.data.get("channel_identifier") {
			Some(Token::Uint(identifier)) => *identifier,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid channel identifier",
					event.name,
				))),
		};
		let participant = match event.data.get("participant") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!("{} event has an invalid participant", event.name,))),
		};

		// Check if we have a channel with participant
		if views::get_channel_by_token_network_and_partner(
			chain_state,
			token_network_address,
			participant,
		)
		.is_none()
		{
			// No channel with `participant`. Check if `participant is our address.
			if participant != chain_state.our_address {
				trace!(message = "Ignore channel withdraw", participant = participant.checksum());
				return Ok(None)
			}
		}

		let total_withdraw = match event.data.get("total_withdraw") {
			Some(Token::Uint(total_withdraw)) => *total_withdraw,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid total withdraw",
					event.name,
				))),
		};
		let channel_withdraw = ContractReceiveChannelWithdraw {
			canonical_identifier: CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address,
				channel_identifier,
			},
			participant,
			total_withdraw,
			fee_config: self.mediation_config.clone(),
			transaction_hash: Some(event.transaction_hash),
			block_number: event.block_number,
			block_hash: event.block_hash,
		};
		Ok(Some(channel_withdraw.into()))
	}

	/// Converts event into `ContractReceiveChannelClosed` state change.
	fn channel_closed(
		&self,
		chain_state: &ChainState,
		event: Event,
	) -> Result<Option<StateChange>> {
		let token_network_address = event.address;
		let channel_identifier = match event.data.get("channel_identifier") {
			Some(Token::Uint(identifier)) => *identifier,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid channel identifier",
					event.name,
				))),
		};
		let transaction_from = match event.data.get("closing_participant") {
			Some(Token::Address(address)) => *address,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid closing participant",
					event.name,
				))),
		};

		// Check if we have a channel with participant
		if views::get_channel_by_token_network_and_partner(
			chain_state,
			token_network_address,
			transaction_from,
		)
		.is_none()
		{
			// No channel with `participant`. Check if `participant is our address.
			if transaction_from != chain_state.our_address {
				trace!(
					message = "Ignore channel closed with closing address",
					closing_address = transaction_from.checksum()
				);
				return Ok(None)
			}
		}

		let channel_closed = ContractReceiveChannelClosed {
			transaction_hash: Some(event.transaction_hash),
			block_number: event.block_number,
			block_hash: event.block_hash,
			transaction_from,
			canonical_identifier: CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address,
				channel_identifier,
			},
		};
		Ok(Some(channel_closed.into()))
	}

	/// Converts event into `ContractReceiveUpdateTransfer` state change.
	fn channel_non_closing_balance_proof_updated(
		&self,
		chain_state: &ChainState,
		event: Event,
	) -> Result<Option<StateChange>> {
		let token_network_address = event.address;
		let channel_identifier = match event.data.get("channel_identifier") {
			Some(Token::Uint(identifier)) => *identifier,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid channel_identifier",
					event.name,
				))),
		};
		let nonce = match event.data.get("nonce") {
			Some(Token::Uint(nonce)) => *nonce,
			_ => return Err(DecodeError(format!("{} event has an invalid nonce", event.name,))),
		};
		let update_transfer = ContractReceiveUpdateTransfer {
			canonical_identifier: CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address,
				channel_identifier,
			},
			nonce,
			transaction_hash: Some(event.transaction_hash),
			block_number: event.block_number,
			block_hash: event.block_hash,
		};
		Ok(Some(update_transfer.into()))
	}

	/// Converts event into `ContractReceiveChannelSettled` state change.
	async fn channel_settled(
		&self,
		chain_state: &ChainState,
		event: Event,
	) -> Result<Option<StateChange>> {
		let token_network_address = event.address;
		let channel_identifier = match event.data.get("channel_identifier") {
			Some(Token::Uint(identifier)) => *identifier,
			_ =>
				return Err(DecodeError(format!(
					"{} event arg `channel_identifier` invalid",
					event.name,
				))),
		};
		let participant1 = match event.data.get("participant1") {
			Some(Token::Address(participant)) => *participant,
			_ =>
				return Err(DecodeError(format!("{} event arg `participant1` invalid", event.name,))),
		};
		let participant2 = match event.data.get("participant2") {
			Some(Token::Address(participant)) => *participant,
			_ =>
				return Err(DecodeError(format!("{} event arg `participant2` invalid", event.name,))),
		};
		let amount_participant1 = match event.data.get("participant1_amount") {
			Some(Token::Uint(amount)) => *amount,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid participant1_amount",
					event.name,
				))),
		};
		let amount_participant2 = match event.data.get("participant2_amount") {
			Some(Token::Uint(amount)) => *amount,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid participant2_amount",
					event.name,
				))),
		};
		let locksroot_participant1 = match event.data.get("participant1_locksroot") {
			Some(Token::FixedBytes(locksroot)) => Locksroot::from_slice(locksroot),
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid participant1_locksroot",
					event.name,
				))),
		};
		let locksroot_participant2 = match event.data.get("participant2_locksroot") {
			Some(Token::FixedBytes(locksroot)) => Locksroot::from_slice(locksroot),
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid participant2_locksroot",
					event.name,
				))),
		};

		if views::get_channel_by_canonical_identifier(
			chain_state,
			CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address,
				channel_identifier,
			},
		)
		.is_none()
		{
			trace!(
				message = "Ignore channel settled",
				channel_identifier = channel_identifier.to_string()
			);
			return Ok(None)
		}

		let (
			our_onchain_locksroot,
			our_transferred_amount,
			partner_onchain_locksroot,
			partner_transferred_amount,
		) = if participant1 == chain_state.our_address {
			(
				locksroot_participant1,
				amount_participant1,
				locksroot_participant2,
				amount_participant2,
			)
		} else if participant2 == chain_state.our_address {
			(
				locksroot_participant2,
				amount_participant2,
				locksroot_participant1,
				amount_participant1,
			)
		} else {
			trace!(
				message = "Ingore channel settled",
				channel_identifier = channel_identifier.to_string(),
				participant1 = participant1.checksum(),
				participant2 = participant2.checksum()
			);
			return Ok(None)
		};

		let channel_settled = ContractReceiveChannelSettled {
			transaction_hash: Some(event.transaction_hash),
			block_number: event.block_number,
			block_hash: event.block_hash,
			canonical_identifier: CanonicalIdentifier {
				chain_identifier: chain_state.chain_id,
				token_network_address,
				channel_identifier,
			},
			our_onchain_locksroot,
			partner_onchain_locksroot,
			our_transferred_amount,
			partner_transferred_amount,
		};
		Ok(Some(channel_settled.into()))
	}

	/// Converts event into `ContractReceiveChannelBatchUnlock` state change.
	async fn channel_unlocked(
		&self,
		chain_state: &ChainState,
		event: Event,
		storage: Arc<StateStorage>,
	) -> Result<Option<StateChange>> {
		let token_network_address = event.address;
		let participant1 = match event.data.get("sender") {
			Some(Token::Address(address)) => *address,
			_ => return Err(DecodeError(format!("{} event has an invalid sender", event.name))),
		};
		let participant2 = match event.data.get("receiver") {
			Some(Token::Address(address)) => *address,
			_ => return Err(DecodeError(format!("{} event has an invalid receiver", event.name))),
		};
		let locksroot = match event.data.get("locksroot") {
			Some(Token::Bytes(bytes)) => Locksroot::from_slice(bytes),
			_ => return Err(DecodeError(format!("{} event has an invalid locksroot", event.name))),
		};
		let unlocked_amount = match event.data.get("unlocked_amount") {
			Some(Token::Uint(amount)) => *amount,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid unlocked amount",
					event.name
				))),
		};
		let returned_tokens = match event.data.get("returned_tokens") {
			Some(Token::Uint(amount)) => *amount,
			_ =>
				return Err(DecodeError(format!(
					"{} event has an invalid returned tokens",
					event.name
				))),
		};
		let token_network =
			match views::get_token_network_by_address(chain_state, token_network_address) {
				Some(token_network) => token_network,
				None => {
					trace!(
						message = "Ignore channel unlock",
						reason = "Token network not found",
						token_network_address = token_network_address.checksum(),
						participant1 = participant1.checksum(),
						participant2 = participant2.checksum()
					);
					return Ok(None)
				},
			};

		let partner = if participant1 == chain_state.our_address {
			participant2
		} else if participant2 == chain_state.our_address {
			participant1
		} else {
			trace!(
				message = "Ignore channel unlock",
				reason = "Neither of participants matches our node address",
				our_address = chain_state.our_address.checksum(),
				participant1 = participant1.checksum(),
				participant2 = participant2.checksum()
			);
			return Ok(None)
		};

		let channel_identifiers = token_network.channelidentifiers_to_channels.keys();
		let mut canonical_identifier = None;
		for channel_identifier in channel_identifiers {
			if partner == participant1 {
				let state_change_record = match storage
					.get_state_change_with_balance_proof_by_locksroot(
						CanonicalIdentifier {
							chain_identifier: chain_state.chain_id,
							token_network_address,
							channel_identifier: *channel_identifier,
						},
						locksroot,
						partner,
					) {
					Ok(Some(state_change_record)) => state_change_record,
					_ => continue,
				};

				canonical_identifier = match state_change_record.data {
					StateChange::ActionInitMediator(inner) =>
						Some(inner.balance_proof.canonical_identifier),
					StateChange::ActionInitTarget(inner) =>
						Some(inner.balance_proof.canonical_identifier),
					StateChange::ReceiveTransferCancelRoute(inner) =>
						Some(inner.transfer.balance_proof.canonical_identifier),
					StateChange::ReceiveTransferRefund(inner) =>
						Some(inner.balance_proof.canonical_identifier),
					StateChange::ReceiveLockExpired(inner) =>
						Some(inner.balance_proof.canonical_identifier),
					StateChange::ReceiveUnlock(inner) =>
						Some(inner.balance_proof.canonical_identifier),
					_ => None,
				};
			} else if partner == participant2 {
				let event_record = match storage.get_event_with_balance_proof_by_locksroot(
					CanonicalIdentifier {
						chain_identifier: chain_state.chain_id,
						token_network_address,
						channel_identifier: *channel_identifier,
					},
					locksroot,
					partner,
				) {
					Ok(Some(event_record)) => event_record,
					_ => continue,
				};

				canonical_identifier = match event_record.data {
					raiden_state_machine::types::Event::ContractSendChannelClose(inner) =>
						inner.balance_proof.map(|b| b.canonical_identifier),
					raiden_state_machine::types::Event::ContractSendChannelUpdateTransfer(
						inner,
					) => Some(inner.balance_proof.canonical_identifier),
					raiden_state_machine::types::Event::SendLockedTransfer(inner) =>
						Some(inner.transfer.balance_proof.canonical_identifier),
					raiden_state_machine::types::Event::SendLockExpired(inner) =>
						Some(inner.balance_proof.canonical_identifier),
					raiden_state_machine::types::Event::SendUnlock(inner) =>
						Some(inner.balance_proof.canonical_identifier),
					_ => None,
				}
			} else {
				trace!(message = "Ignore");
				return Ok(None)
			};

			if canonical_identifier.is_some() {
				break
			}
		}

		let canonical_identifier = match canonical_identifier {
			Some(id) => id,
			None => {
				trace!(message = "Ignore");
				return Ok(None)
			},
		};

		let channel_unlocked = ContractReceiveChannelBatchUnlock {
			canonical_identifier,
			receiver: participant2,
			sender: participant1,
			locksroot,
			unlocked_amount,
			returned_tokens,
			transaction_hash: Some(event.transaction_hash),
			block_number: event.block_number,
			block_hash: event.block_hash,
		};
		Ok(Some(StateChange::ContractReceiveChannelBatchUnlock(channel_unlocked)))
	}
}
