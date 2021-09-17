use std::sync::Arc;

use crate::{
    blockchain::proxies::Account,
    state_machine::types::Event,
    state_manager::StateManager,
    transport::messages::{
            Message,
            SignedMessage,
            TransportServiceMessage,
            WithdrawExpired,
        },
};
use parking_lot::RwLock;
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
        Self {
            account,
            _state_manager: state_manager,
            transport,
        }
    }

    pub async fn handle_event(&self, event: Event) {
        match event {
            Event::SendWithdrawExpired(inner) => {
                let queue_identifier = inner.queue_identifier();
                let mut message: WithdrawExpired = inner.into();
                let _ = message.sign(self.account.private_key());
                let _ = self
                    .transport
                    .send(TransportServiceMessage::Enqueue((queue_identifier, Message::WithdrawExpired(message))));
            }
            Event::SendWithdrawRequest(_) => todo!(),
            Event::ContractSendChannelSettle(_) => todo!(),
            Event::ContractSendChannelUpdateTransfer(_) => todo!(),
            Event::ContractSendChannelBatchUnlock(_) => todo!(),
            Event::InvalidActionWithdraw(_) => todo!(),
            Event::InvalidActionSetRevealTimeout(_) => todo!(),
        }
    }
}
