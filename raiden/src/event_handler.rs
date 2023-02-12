use crate::{
	blockchain::proxies::Account,
	state_machine::types::Event,
	state_manager::StateManager,
	to_message,
	transport::messages::{
		Message, MessageInner, SignedMessage, TransportServiceMessage, WithdrawExpired,
	},
};
use parking_lot::RwLock;
use std::sync::Arc;
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
			Event::SendWithdrawRequest(_) => todo!(),
			Event::SendWithdrawConfirmation(_) => todo!(),
			Event::SendWithdrawExpired(inner) => {
				let queue_identifier = inner.queue_identifier();
				let message = to_message!(inner, private_key, WithdrawExpired);
				let _ = self
					.transport
					.send(TransportServiceMessage::Enqueue((queue_identifier, message)));
			},
			Event::SendLockedTransfer(_) => todo!(),
			Event::SendLockExpired(_) => todo!(),
			Event::SendSecretReveal(_) => todo!(),
			Event::SendUnlock(_) => todo!(),
			Event::SendProcessed(_) => todo!(),
			Event::SendSecretRequest(_) => todo!(),
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
