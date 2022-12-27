use std::sync::Arc;

use futures::StreamExt;
use raiden::state_machine::types::Block;
use raiden::{raiden::Raiden, services::Transitioner};
use web3::{transports::WebSocket, Web3};

use super::SyncService;

pub struct BlockMonitorService {
    raiden: Arc<Raiden>,
    web3: Web3<WebSocket>,
    transition_service: Arc<dyn Transitioner>,
    sync_service: SyncService,
}

impl BlockMonitorService {
    pub fn new(
        raiden: Arc<Raiden>,
        socket: WebSocket,
        transition_service: Arc<dyn Transitioner>,
        sync_service: SyncService,
    ) -> Result<Self, ()> {
        let web3 = web3::Web3::new(socket);

        Ok(Self {
            raiden,
            web3,
            transition_service,
            sync_service,
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
                debug!(self.raiden.logger, "New Block {}", block_number);
                let current_block_number = self.raiden.state_manager.read().current_state.block_number;
                let block_state_change = Block {
                    block_number: block_number.into(),
                    block_hash,
                    gas_limit: header.gas_limit,
                };
                self.transition_service.transition(block_state_change.into()).await;
                self.sync_service.sync(current_block_number, block_number.into()).await;
            }
        }
    }
}
