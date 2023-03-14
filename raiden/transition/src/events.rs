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
		Processed,
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
	packing::pack_balance_proof_message,
	types::{
		BalanceHash,
		Bytes,
		MessageHash,
		MessageTypeId,
		Nonce,
	},
};
use raiden_state_machine::{
	types::Event,
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
}

impl EventHandler {
	pub fn new(
		account: Account<Http>,
		state_manager: Arc<RwLock<StateManager>>,
		proxy_manager: Arc<ProxyManager>,
		transport: UnboundedSender<TransportServiceMessage>,
	) -> Self {
		Self { account, state_manager, proxy_manager, transport }
	}

	pub async fn handle_event(&self, event: Event) {
		let private_key = self.account.private_key();
		match event {
			Event::ContractSendChannelClose(inner) => {
				let (nonce, balance_hash, signature_in_proof, message_hash, canonical_identifier) =
					match inner.balance_proof {
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
					canonical_identifier,
					MessageTypeId::BalanceProof,
					signature_in_proof,
				);

				let our_signature = match self.account.private_key().sign_message(&closing_data.0) {
					Ok(sig) => sig,
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
				let confirmed_block = chain_state.block_hash;
				let channel_state = match views::get_channel_by_canonical_identifier(
					&chain_state,
					inner.canonical_identifier,
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

				channel_proxy
					.close(
						nonce,
						balance_hash,
						message_hash,
						signature_in_proof,
						our_signature,
						inner.triggered_by_blockhash,
					)
					.await;
			},
			Event::ContractSendChannelWithdraw(_) => todo!(),
			Event::ContractSendChannelSettle(_) => todo!(),
			Event::ContractSendChannelCoopSettle(_) => todo!(),
			Event::ContractSendChannelUpdateTransfer(_) => todo!(),
			Event::ContractSendChannelBatchUnlock(_) => todo!(),
			Event::ContractSendSecretReveal(_) => todo!(),
			Event::PaymentSentSuccess(_) => todo!(),
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
			Event::UnlockClaimSuccess(_) => todo!(),
			Event::UnlockSuccess(_) => todo!(),
			Event::UpdatedServicesAddresses(_) => todo!(),
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
