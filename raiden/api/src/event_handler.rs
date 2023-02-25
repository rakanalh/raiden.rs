use std::sync::Arc;

use parking_lot::RwLock;
use raiden_blockchain::proxies::Account;
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
use raiden_state_machine::types::Event;
use raiden_storage::state_manager::StateManager;
use tokio::sync::mpsc::UnboundedSender;
use web3::transports::Http;

pub struct EventHandler {
	account: Account<Http>,
	_state_manager: Arc<RwLock<StateManager>>,
	transport: UnboundedSender<TransportServiceMessage>,
}

impl EventHandler {
	pub fn new(
		account: Account<Http>,
		state_manager: Arc<RwLock<StateManager>>,
		transport: UnboundedSender<TransportServiceMessage>,
	) -> Self {
		Self { account, _state_manager: state_manager, transport }
	}

	pub async fn handle_event(&self, event: Event) {
		let private_key = self.account.private_key();
		match event {
			Event::ContractSendChannelClose(_) => todo!(),
			Event::ContractSendChannelWithdraw(_) => todo!(),
			Event::ContractSendChannelSettle(_) => todo!(),
			Event::ContractSendChannelCoopSettle(_) => todo!(),
			Event::ContractSendChannelUpdateTransfer(_) => todo!(),
			Event::ContractSendChannelBatchUnlock(_) => todo!(),
			Event::ContractSendSecretReveal(_) => todo!(),
			Event::PaymentSentSuccess(_) => todo!(),
			Event::PaymentReceivedSuccess(_) => todo!(),
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
			Event::ErrorInvalidActionCoopSettle(_) => todo!(),
			Event::ErrorInvalidActionWithdraw(_) => todo!(),
			Event::ErrorInvalidActionSetRevealTimeout(_) => todo!(),
			Event::ErrorInvalidReceivedUnlock(_) => todo!(),
			Event::ErrorPaymentSentFailed(_) => todo!(),
			Event::ErrorRouteFailed(_) => todo!(),
			Event::ErrorUnlockFailed(_) => todo!(),
			Event::ErrorInvalidSecretRequest(_) => todo!(),
			Event::ErrorInvalidReceivedLockedTransfer(_) => todo!(),
			Event::ErrorInvalidReceivedLockExpired(_) => todo!(),
			Event::ErrorInvalidReceivedTransferRefund(_) => todo!(),
			Event::ErrorInvalidReceivedWithdrawRequest(_) => todo!(),
			Event::ErrorInvalidReceivedWithdrawConfirmation(_) => todo!(),
			Event::ErrorInvalidReceivedWithdrawExpired(_) => todo!(),
			Event::ErrorUnexpectedReveal(_) => todo!(),
			Event::ErrorUnlockClaimFailed(_) => todo!(),
		}
	}
}
