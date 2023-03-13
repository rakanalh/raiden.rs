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
use tokio::sync::mpsc::UnboundedSender;
use tracing::{
	error,
	event,
	Level,
};
use web3::transports::Http;

pub struct EventHandler {
	account: Account<Http>,
	transport: UnboundedSender<TransportServiceMessage>,
}

impl EventHandler {
	pub fn new(
		account: Account<Http>,
		transport: UnboundedSender<TransportServiceMessage>,
	) -> Self {
		Self { account, transport }
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
