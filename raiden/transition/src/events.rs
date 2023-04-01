use std::sync::Arc;

use parking_lot::RwLock;
use raiden_blockchain::proxies::{
	Account,
	ProxyManager,
};
use raiden_network_messages::{
	messages::{
		LockExpired,
		LockedTransfer,
		MessageInner,
		OutgoingMessage,
		PFSCapacityUpdate,
		PFSFeeUpdate,
		Processed,
		RequestMonitoring,
		SecretRequest,
		SecretReveal,
		SignedMessage,
		TransportServiceMessage,
		Unlock,
		WithdrawConfirmation,
		WithdrawExpired,
		WithdrawRequest,
	},
	to_message,
};
use raiden_primitives::{
	constants::{
		LOCKSROOT_OF_NO_LOCKS,
		MONITORING_REWARD,
	},
	packing::{
		pack_balance_proof_message,
		pack_withdraw,
	},
	traits::ToBytes,
	types::{
		Address,
		AddressMetadata,
		BalanceHash,
		Bytes,
		DefaultAddresses,
		MessageHash,
		MessageTypeId,
		Nonce,
		TokenAmount,
	},
};
use raiden_state_machine::{
	types::{
		Event,
		StateChange,
	},
	views,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{
	error,
	event,
	warn,
	Level,
};
use web3::{
	signing::Key,
	transports::Http,
};

use crate::manager::StateManager;

pub struct EventHandler {
	account: Account<Http>,
	state_manager: Arc<RwLock<StateManager>>,
	proxy_manager: Arc<ProxyManager>,
	transport: UnboundedSender<TransportServiceMessage>,
	default_addresses: DefaultAddresses,
}

impl EventHandler {
	pub fn new(
		account: Account<Http>,
		state_manager: Arc<RwLock<StateManager>>,
		proxy_manager: Arc<ProxyManager>,
		transport: UnboundedSender<TransportServiceMessage>,
		default_addresses: DefaultAddresses,
	) -> Self {
		Self { account, state_manager, proxy_manager, transport, default_addresses }
	}

	pub async fn handle_event(&self, event: Event) {
		let private_key = self.account.private_key();
		match event {
			Event::ContractSendChannelClose(inner) => {
				let (nonce, balance_hash, signature_in_proof, message_hash, canonical_identifier) =
					match inner.balance_proof.clone() {
						Some(bp) => {
							let signature = match bp.signature {
								Some(sig) => sig,
								None => {
									warn!("Closing channel but partner's balance proof is None");
									Bytes(vec![])
								},
							};

							let message_hash = match bp.message_hash {
								Some(m) => m,
								None => {
									warn!("Closing channel but message hash is None");
									MessageHash::zero()
								},
							};

							(
								bp.nonce,
								bp.balance_hash,
								signature,
								message_hash,
								bp.canonical_identifier,
							)
						},
						None => (
							Nonce::zero(),
							BalanceHash::zero(),
							Bytes(vec![0; 65]),
							MessageHash::zero(),
							inner.canonical_identifier.clone(),
						),
					};

				let closing_data = pack_balance_proof_message(
					nonce,
					balance_hash,
					message_hash,
					canonical_identifier.clone(),
					MessageTypeId::BalanceProof,
					signature_in_proof.clone(),
				);

				let our_signature: Bytes =
					match self.account.private_key().sign_message(&closing_data.0) {
						Ok(sig) => Bytes(sig.to_bytes()),
						Err(e) => {
							event!(
								Level::ERROR,
								reason = "Close channel, signing failed",
								error = format!("{:?}", e),
							);
							return
						},
					};

				let chain_state = self.state_manager.read().current_state.clone();
				let channel_state = match views::get_channel_by_canonical_identifier(
					&chain_state,
					inner.canonical_identifier.clone(),
				) {
					Some(channel_state) => channel_state,
					None => {
						error!("Closing channel for non-existent channel");
						return
					},
				};
				let channel_proxy = match self.proxy_manager.payment_channel(&channel_state).await {
					Ok(proxy) => proxy,
					Err(e) => {
						error!("Something went wrong constructing channel proxy {:?}", e);
						return
					},
				};

				if let Err(e) = channel_proxy
					.close(
						self.account.clone(),
						channel_state.partner_state.address,
						canonical_identifier.channel_identifier,
						nonce,
						balance_hash,
						message_hash,
						signature_in_proof,
						our_signature,
						inner.triggered_by_blockhash,
					)
					.await
				{
					event!(
						Level::ERROR,
						reason = "Channel close transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::ContractSendChannelWithdraw(inner) => {
				let withdraw_confirmation = pack_withdraw(
					inner.canonical_identifier.clone(),
					self.account.address(),
					inner.total_withdraw,
					inner.expiration,
				);

				let our_signature =
					match self.account.private_key().sign_message(&withdraw_confirmation.0) {
						Ok(sig) => Bytes(sig.to_bytes()),
						Err(e) => {
							event!(
								Level::ERROR,
								reason = "Channel withdraw, signing failed",
								error = format!("{:?}", e),
							);
							return
						},
					};

				let chain_state = self.state_manager.read().current_state.clone();
				let channel_state = match views::get_channel_by_canonical_identifier(
					&chain_state,
					inner.canonical_identifier.clone(),
				) {
					Some(channel_state) => channel_state,
					None => {
						error!("ContractSendChannelWithdraw for non-existent channel");
						return
					},
				};

				let channel_proxy = match self.proxy_manager.payment_channel(&channel_state).await {
					Ok(proxy) => proxy,
					Err(e) => {
						error!("Something went wrong constructing channel proxy {:?}", e);
						return
					},
				};

				if let Err(e) = channel_proxy
					.set_total_withdraw(
						self.account.clone(),
						inner.canonical_identifier.channel_identifier.clone(),
						inner.total_withdraw,
						channel_state.our_state.address,
						channel_state.partner_state.address,
						our_signature,
						inner.partner_signature.clone(),
						inner.expiration.clone(),
						inner.triggered_by_blockhash,
					)
					.await
				{
					event!(
						Level::ERROR,
						reason = "Channel setTotalWithdraw transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::ContractSendChannelSettle(inner) => {
				let chain_state = self.state_manager.read().current_state.clone();
				let channel_state = match views::get_channel_by_canonical_identifier(
					&chain_state,
					inner.canonical_identifier.clone(),
				) {
					Some(channel_state) => channel_state,
					None => {
						error!("ContractSendChannelWithdraw for non-existent channel");
						return
					},
				};

				let channel_proxy = match self.proxy_manager.payment_channel(&channel_state).await {
					Ok(proxy) => proxy,
					Err(e) => {
						error!("Something went wrong constructing channel proxy {:?}", e);
						return
					},
				};
				let token_network_proxy = channel_proxy.token_network;

				let participant_details = match token_network_proxy
					.participants_details(
						inner.canonical_identifier.channel_identifier,
						channel_state.our_state.address,
						channel_state.partner_state.address,
						Some(inner.triggered_by_blockhash),
					)
					.await
				{
					Ok(details) => details,
					Err(_) => match token_network_proxy
						.participants_details(
							inner.canonical_identifier.channel_identifier,
							channel_state.our_state.address,
							channel_state.partner_state.address,
							None,
						)
						.await
					{
						Ok(details) => details,
						Err(e) => {
							error!("Channel settle: Something went wrong fetching participant details {:?}", e);
							return
						},
					},
				};

				let (our_transferred_amount, our_locked_amount, our_locksroot) =
					if participant_details.our_details.balance_hash != BalanceHash::zero() {
						let event_record = match self
							.state_manager
							.read()
							.storage
							.get_event_with_balance_proof_by_balance_hash(
								inner.canonical_identifier.clone(),
								participant_details.our_details.balance_hash,
								participant_details.partner_details.address,
							) {
							Ok(Some(event)) => event.data,
							Ok(None) => {
								error!("Channel settle: Our balance proof could not be found in the database");
								return
							},
							Err(e) => {
								error!("Channel settle: storage error {}", e);
								return
							},
						};

						let our_balance_proof = match event_record {
							Event::SendLockedTransfer(inner) => inner.transfer.balance_proof,
							Event::SendLockExpired(inner) => inner.balance_proof,
							Event::SendUnlock(inner) => inner.balance_proof,
							Event::ContractSendChannelClose(inner) =>
								inner.balance_proof.expect("Balance proof should be set"),
							Event::ContractSendChannelUpdateTransfer(inner) => inner.balance_proof,
							_ => {
								error!("Channel settle: found participant event does not contain balance proof");
								return
							},
						};

						(
							our_balance_proof.transferred_amount,
							our_balance_proof.locked_amount,
							our_balance_proof.locksroot,
						)
					} else {
						(TokenAmount::zero(), TokenAmount::zero(), *LOCKSROOT_OF_NO_LOCKS)
					};

				let (partner_transferred_amount, partner_locked_amount, partner_locksroot) =
					if participant_details.partner_details.balance_hash != BalanceHash::zero() {
						let state_change_record = match self
							.state_manager
							.read()
							.storage
							.get_state_change_with_balance_proof_by_balance_hash(
								inner.canonical_identifier.clone(),
								participant_details.partner_details.balance_hash,
								participant_details.partner_details.address,
							) {
							Ok(Some(state_change)) => state_change.data,
							Ok(None) => {
								error!("Channel settle: Partner balance proof could not be found in the database");
								return
							},
							Err(e) => {
								error!("Channel settle: storage error {}", e);
								return
							},
						};

						let partner_balance_proof = match state_change_record {
							StateChange::ActionInitMediator(inner) => inner.balance_proof,
							StateChange::ActionInitTarget(inner) => inner.balance_proof,
							StateChange::ActionTransferReroute(inner) =>
								inner.transfer.balance_proof,
							StateChange::ReceiveTransferCancelRoute(inner) =>
								inner.transfer.balance_proof,
							StateChange::ReceiveLockExpired(inner) => inner.balance_proof,
							StateChange::ReceiveTransferRefund(inner) => inner.balance_proof,
							StateChange::ReceiveUnlock(inner) => inner.balance_proof,
							_ => {
								error!("Channel settle: found participant event does not contain balance proof");
								return
							},
						};

						(
							partner_balance_proof.transferred_amount,
							partner_balance_proof.locked_amount,
							partner_balance_proof.locksroot,
						)
					} else {
						(TokenAmount::zero(), TokenAmount::zero(), *LOCKSROOT_OF_NO_LOCKS)
					};

				if let Err(e) = token_network_proxy
					.settle(
						self.account.clone(),
						inner.canonical_identifier.channel_identifier,
						our_transferred_amount,
						our_locked_amount,
						our_locksroot,
						channel_state.partner_state.address,
						partner_transferred_amount,
						partner_locked_amount,
						partner_locksroot,
						inner.triggered_by_blockhash,
					)
					.await
				{
					event!(
						Level::ERROR,
						reason = "Channel setTotalWithdraw transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::ContractSendChannelCoopSettle(inner) => {
				// Call
			},
			Event::ContractSendChannelUpdateTransfer(inner) => {
				let partner_signature = match &inner.balance_proof.signature {
					Some(sig) => sig.clone(),
					None => {
						error!("Channel update transfer: Partner signature is not set");
						return
					},
				};
				let message_hash = match &inner.balance_proof.message_hash {
					Some(hash) => hash.clone(),
					None => {
						error!("Channel update transfer: Message hash is not set");
						return
					},
				};
				let chain_state = self.state_manager.read().current_state.clone();
				let channel_state = match views::get_channel_by_canonical_identifier(
					&chain_state,
					inner.balance_proof.canonical_identifier.clone(),
				) {
					Some(channel_state) => channel_state,
					None => {
						error!("ContractSendChannelWithdraw for non-existent channel");
						return
					},
				};

				let channel_proxy = match self.proxy_manager.payment_channel(&channel_state).await {
					Ok(proxy) => proxy,
					Err(e) => {
						error!("Something went wrong constructing channel proxy {:?}", e);
						return
					},
				};

				let balance_proof = inner.balance_proof.clone();
				let non_closing_data = pack_balance_proof_message(
					balance_proof.nonce,
					balance_proof.balance_hash,
					message_hash,
					balance_proof.canonical_identifier,
					MessageTypeId::BalanceProofUpdate,
					partner_signature.clone(),
				);
				let our_signature =
					match self.account.private_key().sign_message(&non_closing_data.0) {
						Ok(sig) => Bytes(sig.to_bytes()),
						Err(e) => {
							error!("Error signing non-closing-data {:?}", e);
							return
						},
					};

				if let Err(e) = channel_proxy
					.update_transfer(
						self.account.clone(),
						channel_state.canonical_identifier.channel_identifier,
						balance_proof.nonce,
						channel_state.partner_state.address,
						balance_proof.balance_hash,
						message_hash,
						partner_signature,
						our_signature,
						inner.triggered_by_blockhash,
					)
					.await
				{
					event!(
						Level::ERROR,
						reason = "Channel update transfer transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::ContractSendChannelBatchUnlock(inner) => {
				// Call
			},
			Event::ContractSendSecretReveal(inner) => {
				let secret_registry = match self
					.proxy_manager
					.secret_registry(self.default_addresses.secret_registry)
					.await
				{
					Ok(registry) => registry,
					Err(_) => {
						error!(
							"Could not instantiate secret registry with address {:?}",
							self.default_addresses.secret_registry
						);
						return
					},
				};
				if let Err(e) = secret_registry
					.register_secret(
						self.account.clone(),
						inner.secret.clone(),
						inner.triggered_by_blockhash,
					)
					.await
				{
					event!(
						Level::ERROR,
						reason = "RegisterSecret transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::PaymentSentSuccess(inner) => {
				event!(
					Level::INFO,
					reason = "Payment Sent",
					to = format!("{:?}", inner.target),
					amount = format!("{}", inner.amount),
				);
			},
			Event::UnlockClaimSuccess(inner) => {
				// TODO
				todo!()
			},
			Event::UnlockSuccess(_) => {
				// Do Nothing
			},
			Event::UpdatedServicesAddresses(inner) => {
				// TODO
				todo!()
			},
			Event::ExpireServicesAddresses(inner) => {
				// TODO
				todo!()
			},
			Event::PaymentReceivedSuccess(inner) => {
				event!(
					Level::INFO,
					reason = "Payment Received",
					from = format!("{:?}", inner.initiator),
					amount = format!("{}", inner.amount),
				);
			},
			Event::SendWithdrawRequest(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, WithdrawRequest);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendWithdrawConfirmation(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, WithdrawConfirmation);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendWithdrawExpired(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, WithdrawExpired);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendLockedTransfer(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, LockedTransfer);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendLockExpired(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, LockExpired);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendSecretReveal(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, SecretReveal);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendUnlock(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, Unlock);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendProcessed(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, Processed);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendSecretRequest(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, SecretRequest);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::UpdatedServicesAddresses(_) => todo!(),
			Event::SendPFSUpdate(canonical_identifier, update_fee_schedule) => {
				let chain_state = &self.state_manager.read().current_state;
				let channel_state = match views::get_channel_by_canonical_identifier(
					chain_state,
					canonical_identifier,
				) {
					Some(channel_state) => channel_state,
					None => return,
				};

				let mut capacity_message: PFSCapacityUpdate = channel_state.clone().into();
				let _ = capacity_message.sign(private_key.clone());
				let message = OutgoingMessage {
					message_identifier: 0,
					recipient: Address::zero(),
					recipient_metadata: AddressMetadata::default(),
					inner: MessageInner::PFSCapacityUpdate(capacity_message),
				};
				let _ = self.transport.send(TransportServiceMessage::Broadcast(message));

				if !update_fee_schedule {
					return
				}

				let mut fee_message: PFSFeeUpdate = channel_state.clone().into();
				let _ = fee_message.sign(private_key);
				let message = OutgoingMessage {
					message_identifier: 0,
					recipient: Address::zero(),
					recipient_metadata: AddressMetadata::default(),
					inner: MessageInner::PFSFeeUpdate(fee_message),
				};
				let _ = self.transport.send(TransportServiceMessage::Broadcast(message));
			},
			Event::SendMSUpdate(balance_proof) => {
				let mut monitoring_message = RequestMonitoring::from_balance_proof(
					balance_proof,
					self.account.address(),
					*MONITORING_REWARD,
					self.default_addresses.monitoring_service,
				);
				let _ = monitoring_message.sign(private_key);
				let message = OutgoingMessage {
					message_identifier: 0,
					recipient: Address::zero(),
					recipient_metadata: AddressMetadata {
						user_id: String::new(),
						displayname: String::new(),
						capabilities: String::new(),
					},
					inner: MessageInner::MSUpdate(monitoring_message),
				};
				let _ = self.transport.send(TransportServiceMessage::Broadcast(message));
			},
			Event::ErrorInvalidActionCoopSettle(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidActionWithdraw(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidActionSetRevealTimeout(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidReceivedUnlock(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorPaymentSentFailed(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorRouteFailed(e) => {
				event!(
					Level::ERROR,
					reason = "Route Failed",
					routes = format!("{:?}", e.route),
					token_network_address = format!("{}", e.token_network_address),
				);
			},
			Event::ErrorUnlockFailed(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidSecretRequest(e) => {
				event!(
					Level::ERROR,
					reason = "Invalid secret request",
					payment_identifier = format!("{:?}", e.payment_identifier),
					intended_amount = format!("{}", e.intended_amount),
					actual_amount = format!("{}", e.actual_amount),
				);
			},
			Event::ErrorInvalidReceivedLockedTransfer(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidReceivedLockExpired(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidReceivedTransferRefund(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidReceivedWithdrawRequest(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidReceivedWithdrawConfirmation(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorInvalidReceivedWithdrawExpired(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorUnexpectedReveal(e) => {
				error!("{}", e.reason);
			},
			Event::ErrorUnlockClaimFailed(e) => {
				error!("{}", e.reason);
			},
		}
	}
}
