use std::sync::Arc;

use parking_lot::RwLock;
use crate::{
    state_machine::types::Event,
    state_manager::StateManager,
};

pub struct EventHandler {
    _state_manager: Arc<RwLock<StateManager>>,
    //transport: Arc<dyn Transport>,
}

impl EventHandler {
    pub fn new(state_manager: Arc<RwLock<StateManager>>) -> Self {
        Self {
            _state_manager: state_manager,
        }
    }

    pub async fn handle_event(&self, event: Event) {
        match event {
            Event::SendWithdrawExpired(_) => todo!(),
            Event::SendWithdrawRequest(_) => todo!(),
            Event::ContractSendChannelSettle(_) => todo!(),
            Event::ContractSendChannelUpdateTransfer(_) => todo!(),
            Event::ContractSendChannelBatchUnlock(_) => todo!(),
            Event::InvalidActionWithdraw(_) => todo!(),
            Event::InvalidActionSetRevealTimeout(_) => todo!(),
        }
    }
}
