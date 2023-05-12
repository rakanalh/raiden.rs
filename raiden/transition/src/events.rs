use std::sync::Arc;

use parking_lot::RwLock as SyncRwLock;
use raiden_blockchain::{
	proxies::{
		Account,
		ProxyManager,
	},
	transactions::WithdrawInput,
};
use raiden_network_messages::{
	messages::{
		LockExpired,
		LockedTransfer,
		MessageInner,
		OutgoingMessage,
		PFSCapacityUpdate,
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
	payments::{
		PaymentStatus,
		PaymentsRegistry,
	},
	traits::{
		Checksum,
		ToBytes,
	},
	types::{
		Address,
		AddressMetadata,
		BalanceHash,
		BlockId,
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
		ChainState,
		ChannelEndState,
		Event,
		StateChange,
	},
	views,
};
use tokio::sync::{
	mpsc::UnboundedSender,
	RwLock,
};
use tracing::{
	error,
	info,
	warn,
};
use web3::{
	signing::Key,
	transports::Http,
	types::BlockNumber,
	Web3,
};

use crate::{
	manager::StateManager,
	utils::channel_state_until_state_change,
};

#[derive(Clone)]
pub struct EventHandler {
	web3: Web3<Http>,
	account: Account<Http>,
	state_manager: Arc<SyncRwLock<StateManager>>,
	proxy_manager: Arc<ProxyManager>,
	transport: UnboundedSender<TransportServiceMessage>,
	default_addresses: DefaultAddresses,
	payment_registry: Arc<RwLock<PaymentsRegistry>>,
}

impl EventHandler {
	pub fn new(
		web3: Web3<Http>,
		account: Account<Http>,
		state_manager: Arc<SyncRwLock<StateManager>>,
		proxy_manager: Arc<ProxyManager>,
		transport: UnboundedSender<TransportServiceMessage>,
		default_addresses: DefaultAddresses,
		payment_registry: Arc<RwLock<PaymentsRegistry>>,
	) -> Self {
		Self {
			web3,
			account,
			state_manager,
			proxy_manager,
			transport,
			default_addresses,
			payment_registry,
		}
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
							error!(
								message = "Close channel, signing failed",
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
					error!(
						message = "Channel close transaction failed",
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
							error!(
								message = "Channel withdraw, signing failed",
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
					error!(
						message = "Channel setTotalWithdraw transaction failed",
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
					error!(
						message = "Channel setTotalWithdraw transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::ContractSendChannelCoopSettle(inner) => {
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

				let participant_withdraw_data = pack_withdraw(
					inner.canonical_identifier.clone(),
					channel_state.our_state.address,
					inner.our_total_withdraw,
					inner.expiration,
				);
				let our_initiator_signature =
					match self.account.private_key().sign_message(&participant_withdraw_data.0) {
						Ok(signature) => signature,
						Err(e) => {
							error!("Could not sign our withdraw data: {:?}", e);
							return
						},
					};

				let withdraw_initiator = WithdrawInput {
					initiator: channel_state.our_state.address,
					total_withdraw: inner.our_total_withdraw,
					expiration_block: inner.expiration,
					initiator_signature: Bytes(our_initiator_signature.to_bytes()),
					partner_signature: inner.signature_our_withdraw.clone(),
				};

				let partner_withdraw_data = pack_withdraw(
					inner.canonical_identifier.clone(),
					channel_state.partner_state.address,
					inner.partner_total_withdraw,
					inner.expiration,
				);
				let our_partner_signature =
					match self.account.private_key().sign_message(&partner_withdraw_data.0) {
						Ok(signature) => signature,
						Err(e) => {
							error!("Could not sign partner withdraw data: {:?}", e);
							return
						},
					};

				let withdraw_partner = WithdrawInput {
					initiator: channel_state.partner_state.address,
					total_withdraw: inner.partner_total_withdraw,
					expiration_block: inner.expiration,
					initiator_signature: inner.signature_partner_withdraw.clone(),
					partner_signature: Bytes(our_partner_signature.to_bytes()),
				};

				if let Err(e) = channel_proxy
					.coop_settle(
						self.account.clone(),
						channel_state.canonical_identifier.channel_identifier,
						withdraw_partner,
						withdraw_initiator,
						inner.triggered_by_blockhash,
					)
					.await
				{
					error!(
						message = "Channel cooperative settle transaction failed",
						error = format!("{:?}", e),
					);
				}
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
					error!(
						message = "Channel update transfer transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::ContractSendChannelBatchUnlock(inner) => {
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

				let our_address = channel_state.our_state.address;
				let our_locksroot = channel_state.our_state.onchain_locksroot;

				let partner_address = channel_state.partner_state.address;
				let partner_locksroot = channel_state.partner_state.onchain_locksroot;

				let search_events = our_locksroot != *LOCKSROOT_OF_NO_LOCKS;
				let search_state_changes = partner_locksroot != *LOCKSROOT_OF_NO_LOCKS;

				if !search_events && !search_state_changes {
					warn! {
						message = "Onchain unlock already mined",
						channel_identifier = channel_state.canonical_identifier.channel_identifier.to_string(),
						participant = inner.sender.checksum(),
					};
				}

				// Update old channel state with lock information from current state
				// Call this after settlement, when you need to work on an older locksroot
				// because the channel has been settled with an outdated balance proof.
				fn update_lock_info(
					old_state: &mut ChannelEndState,
					new_state: &ChannelEndState,
					chain_state: &ChainState,
				) {
					// After settlement, no unlocks can be processed. So all locks that are
					// still present in the balance proof are locked, unless their secret is
					// registered on-chain.
					for (secret, unlock) in &old_state.secrethashes_to_onchain_unlockedlocks {
						old_state
							.secrethashes_to_lockedlocks
							.insert(secret.clone(), unlock.lock.clone());
					}

					old_state.secrethashes_to_unlockedlocks.clear();

					// In the time between the states, some locks might have been unlocked
					// on-chain. Update their state to "on-chain unlocked".
					for (secret, updated_unlock) in &new_state.secrethashes_to_onchain_unlockedlocks
					{
						old_state.secrethashes_to_lockedlocks.remove_entry(&secret);
						old_state
							.secrethashes_to_onchain_unlockedlocks
							.insert(secret.clone(), updated_unlock.clone());
					}

					// If we don't have a task for the secret, then that lock can't be
					// relevant to us, anymore. Otherwise, we would not have deleted the
					// payment task.
					// One case where this is necessary: We are a mediator and didn't unlock
					// the payee's BP, but the secret has been registered on-chain. We
					// will receive an Unlock from the payer and delete our MediatorTask,
					// since we got our tokens. After deleting the task, we won't listen for
					// on-chain unlocks, so we wrongly consider the tokens in the outgoing
					// channel to be ours and send an on-chain unlock although we won't
					// unlock any tokens to our benefit.

					old_state.secrethashes_to_lockedlocks = old_state
						.secrethashes_to_lockedlocks
						.clone()
						.into_iter()
						.filter(|(secret, _)| {
							chain_state.payment_mapping.secrethashes_to_task.contains_key(secret)
						})
						.collect();
				}

				if search_state_changes {
					let state_change_record = match self
						.state_manager
						.read()
						.storage
						.get_state_change_with_balance_proof_by_locksroot(
							channel_state.canonical_identifier.clone(),
							partner_locksroot,
							partner_address,
						) {
						Ok(Some(record)) => record,
						Ok(None) => {
							error!("Channel batch unlock: Failed to find state that matches the current channel locksroot");
							return
						},
						Err(e) => {
							error!("Channel batch unlock: storage error {}", e);
							return
						},
					};

					let state_change_identifier = state_change_record.identifier;
					let mut restored_channel_state = match channel_state_until_state_change(
						self.state_manager.read().storage.clone(),
						channel_state.canonical_identifier.clone(),
						state_change_identifier,
					) {
						Some(channel_state) => channel_state,
						None => {
							error!(
								message = "Channel was not found before state change",
								state_change = format!("{}", state_change_identifier),
							);
							return
						},
					};
					update_lock_info(
						&mut restored_channel_state.partner_state,
						&channel_state.partner_state,
						&chain_state,
					);

					let gain: TokenAmount = restored_channel_state
						.partner_state
						.secrethashes_to_onchain_unlockedlocks
						.values()
						.map(|unlock| unlock.lock.amount)
						.fold(TokenAmount::zero(), |current, next| current.saturating_add(next));

					if gain > TokenAmount::zero() {
						if let Err(e) = channel_proxy
							.unlock(
								self.account.clone(),
								channel_state.canonical_identifier.channel_identifier,
								partner_address,
								our_address,
								restored_channel_state.partner_state.pending_locks,
								inner.triggered_by_blockhash,
							)
							.await
						{
							error!(
								message = "Channel unlock transaction failed",
								error = format!("{:?}", e),
							);
						}
					}
				}

				if search_events {
					let event_record = match self
						.state_manager
						.read()
						.storage
						.get_event_with_balance_proof_by_locksroot(
							channel_state.canonical_identifier.clone(),
							our_locksroot,
							partner_address,
						) {
						Ok(Some(record)) => record,
						Ok(None) => {
							error!("Channel batch unlock: Failed to find state that matches the current channel locksroot");
							return
						},
						Err(e) => {
							error!("Channel batch unlock: storage error {}", e);
							return
						},
					};

					let state_change_identifier = event_record.state_change_identifier;
					let mut restored_channel_state = match channel_state_until_state_change(
						self.state_manager.read().storage.clone(),
						channel_state.canonical_identifier.clone(),
						state_change_identifier,
					) {
						Some(channel_state) => channel_state,
						None => {
							error!(
								message = "Channel was not found before state change",
								state_change = format!("{}", state_change_identifier),
							);
							return
						},
					};
					update_lock_info(
						&mut restored_channel_state.our_state,
						&channel_state.our_state,
						&chain_state,
					);

					let gain: TokenAmount = restored_channel_state
						.our_state
						.secrethashes_to_lockedlocks
						.values()
						.map(|lock| lock.amount)
						.fold(TokenAmount::zero(), |current, next| current.saturating_add(next));

					if gain > TokenAmount::zero() {
						if let Err(e) = channel_proxy
							.unlock(
								self.account.clone(),
								channel_state.canonical_identifier.channel_identifier,
								our_address,
								partner_address,
								restored_channel_state.our_state.pending_locks,
								inner.triggered_by_blockhash,
							)
							.await
						{
							error!(
								message = "Channel unlock transaction failed",
								error = format!("{:?}", e),
							);
						}
					}
				}
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
							message = "Could not instantiate secret registry with address.",
							secret_registry_address =
								self.default_addresses.secret_registry.checksum()
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
					error!(
						message = "RegisterSecret transaction failed",
						error = format!("{:?}", e),
					);
				}
			},
			Event::PaymentSentSuccess(inner) => {
				info!(
					message = "Payment Sent",
					to = format!("{:?}", inner.target),
					amount = format!("{}", inner.amount),
				);
				self.payment_registry
					.write()
					.await
					.complete(PaymentStatus::Success(inner.target, inner.identifier));
			},
			Event::UpdatedServicesAddresses(inner) => {
				let _ = self.transport.send(TransportServiceMessage::UpdateServiceAddresses(
					inner.service_address,
					inner.validity,
				));
			},
			Event::ExpireServicesAddresses(_inner) => {
				if let Ok(Some(block)) =
					self.web3.eth().block(BlockId::Number(BlockNumber::Latest)).await
				{
					let _ = self.transport.send(TransportServiceMessage::ExpireServiceAddresses(
						block.timestamp,
						block.number.expect("Block number should be set").into(),
					));
				}
			},
			Event::PaymentReceivedSuccess(inner) => {
				info!(
					message = "Payment Received",
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
			Event::SendPFSUpdate(pfs_update) => {
				let chain_state = &self.state_manager.read().current_state;
				let channel_state = match views::get_channel_by_canonical_identifier(
					chain_state,
					pfs_update.canonical_identifier,
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

				if !pfs_update.update_fee_schedule {
					return
				}

				// let mut fee_message: PFSFeeUpdate = channel_state.clone().into();
				// let _ = fee_message.sign(private_key);
				// let message = OutgoingMessage {
				// 	message_identifier: 0,
				// 	recipient: Address::zero(),
				// 	recipient_metadata: AddressMetadata::default(),
				// 	inner: MessageInner::PFSFeeUpdate(fee_message),
				// };
				// let _ = self.transport.send(TransportServiceMessage::Broadcast(message));
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
			Event::ClearMessages(queue_identifier) => {
				let _ = self.transport.send(TransportServiceMessage::Clear(queue_identifier));
			},
			Event::ErrorInvalidActionCoopSettle(e) => {
				error!(message = "Invalid action CoopSettle", reason = e.reason);
			},
			Event::ErrorInvalidActionWithdraw(e) => {
				error!(message = "Invalid action Withdraw", reason = e.reason);
			},
			Event::ErrorInvalidActionSetRevealTimeout(e) => {
				error!(message = "Invalid action SetRevealTimeout", reason = e.reason);
			},
			Event::ErrorInvalidReceivedUnlock(e) => {
				error!(message = "Invalid received Unlock", reason = e.reason);
			},
			Event::ErrorPaymentSentFailed(e) => {
				error!(message = "Payment failed", reason = e.reason);
				self.payment_registry.write().await.complete(PaymentStatus::Error(
					e.target,
					e.identifier,
					e.reason,
				));
			},
			Event::ErrorRouteFailed(e) => {
				error!(
					message = "Route Failed",
					routes = format!("{:?}", e.route),
					token_network_address = format!("{}", e.token_network_address),
				);
			},
			Event::ErrorUnlockFailed(e) => {
				error!(message = "Unlock failed", reason = e.reason);
			},
			Event::ErrorInvalidSecretRequest(e) => {
				error!(
					message = "Invalid secret request",
					payment_identifier = format!("{:?}", e.payment_identifier),
					intended_amount = format!("{}", e.intended_amount),
					actual_amount = format!("{}", e.actual_amount),
				);
			},
			Event::ErrorInvalidReceivedLockedTransfer(e) => {
				error!(message = "Invalid received locked transfer", reason = e.reason);
			},
			Event::ErrorInvalidReceivedLockExpired(e) => {
				error!(message = "Invalid received LockExpired", reason = e.reason);
			},
			Event::ErrorInvalidReceivedTransferRefund(e) => {
				error!(message = "Invalid received TransferRefund", reason = e.reason);
			},
			Event::ErrorInvalidReceivedWithdrawRequest(e) => {
				error!(message = "Invalid received WithdrawRequest", reason = e.reason);
			},
			Event::ErrorInvalidReceivedWithdrawConfirmation(e) => {
				error!(message = "Invalid received WithdrawConfirmation", reason = e.reason);
			},
			Event::ErrorInvalidReceivedWithdrawExpired(e) => {
				error!(message = "Invalid received WithdrawExpired", reason = e.reason);
			},
			Event::ErrorUnexpectedReveal(e) => {
				error!(message = "Unexpected reveal", reason = e.reason);
			},
			Event::ErrorUnlockClaimFailed(e) => {
				error!(message = "Error unlocking transfer", reason = e.reason);
			},
			Event::UnlockClaimSuccess(_) => {
				// Do Nothing
			},
			Event::UnlockSuccess(_) => {
				// Do Nothing
			},
		}
	}
}
