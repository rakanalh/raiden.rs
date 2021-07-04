use std::sync::Arc;

use futures::StreamExt;
use parking_lot::RwLock;
use raiden::{
    state_machine::types::{
        Block,
        ChainID,
        StateChange,
    },
    state_manager::StateManager,
};
use slog::Logger;
use web3::{
    transports::WebSocket,
    Web3,
};

use super::{
    SyncService,
    TransitionService,
};

pub struct BlockMonitorService {
    chain_id: ChainID,
    web3: Web3<WebSocket>,
    state_manager: Arc<RwLock<StateManager>>,
    transition_service: Arc<TransitionService>,
    sync_service: SyncService,
    logger: Logger,
}

impl BlockMonitorService {
    pub fn new(
        socket: WebSocket,
        chain_id: ChainID,
        state_manager: Arc<RwLock<StateManager>>,
        transition_service: Arc<TransitionService>,
        sync_service: SyncService,
        logger: Logger,
    ) -> Result<Self, ()> {
        let web3 = web3::Web3::new(socket);

        Ok(Self {
            chain_id,
            web3,
            state_manager,
            transition_service,
            sync_service,
            logger,
        })
    }

    pub async fn start(mut self) {
        let mut block_stream = match self.web3.eth_subscribe().subscribe_new_heads().await {
            Ok(stream) => stream,
            Err(_) => {
                println!("Failed to get stream");
                return;
            }
        };
        while let Some(subscription) = block_stream.next().await {
            if let Ok(header) = subscription {
                let block_number = match header.number {
                    Some(block_number) => block_number,
                    None => continue,
                };
                let block_hash = match header.hash {
                    Some(hash) => hash,
                    None => continue,
                };
                debug!(self.logger, "Block {}", block_number);
                let current_block_number = self.state_manager.read().current_state.block_number;
                let block_state_change = Block {
                    block_number: block_number.into(),
                    block_hash,
                    gas_limit: header.gas_limit,
                };
                self.transition_service
                    .transition(StateChange::Block(block_state_change))
                    .await;
                self.sync_service.sync(current_block_number, block_number).await;
            }
        }
    }
}
