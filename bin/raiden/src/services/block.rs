use std::sync::Arc;

use futures::StreamExt;
use raiden::state_machine::types::{
    Block,
    ChainID,
    StateChange,
};
use web3::{
    transports::WebSocket,
    Web3,
};

use super::TransitionService;

pub struct BlockMonitorService {
    chain_id: ChainID,
    web3: Web3<WebSocket>,
    transition_service: Arc<TransitionService>,
}

impl BlockMonitorService {
    pub fn new(socket: WebSocket, chain_id: ChainID, transition_service: Arc<TransitionService>) -> Result<Self, ()> {
        let web3 = web3::Web3::new(socket);

        Ok(Self {
            chain_id,
            web3,
            transition_service,
        })
    }

    pub async fn start(self) {
        let mut block_stream = match self.web3.eth_subscribe().subscribe_new_heads().await {
            Ok(stream) => stream,
            Err(_) => {
                println!("Failed to get stream");
                return;
            }
        };
        while let Some(subscription) = block_stream.next().await {
            if let Ok(subscription) = subscription {
                if let Some(block_number) = subscription.number {
                    let block_state_change = Block::new(self.chain_id.clone(), block_number.into());
                    self.transition_service
                        .transition(StateChange::Block(block_state_change))
                        .await;
                }
            }
        }
    }
}
