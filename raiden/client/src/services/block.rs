use std::sync::Arc;

use futures::StreamExt;
use raiden_api::raiden::Raiden;
use raiden_state_machine::types::Block;
use raiden_transition::Transitioner;
use tracing::error;
use web3::{
	transports::WebSocket,
	Web3,
};

use super::SyncService;

/// Sync with an Ethereum node on latest blocks and dispatch block state changes.
pub struct BlockMonitorService {
	raiden: Arc<Raiden>,
	web3: Web3<WebSocket>,
	transition_service: Arc<Transitioner>,
	sync_service: SyncService,
}

impl BlockMonitorService {
	/// Create an instance of `BlockMonitoringService'.
	pub fn new(
		raiden: Arc<Raiden>,
		socket: WebSocket,
		transition_service: Arc<Transitioner>,
		sync_service: SyncService,
	) -> Self {
		let web3 = web3::Web3::new(socket);

		Self { raiden, web3, transition_service, sync_service }
	}

	/// Start the service.
	pub async fn start(mut self) {
		let mut block_stream = match self.web3.eth_subscribe().subscribe_new_heads().await {
			Ok(stream) => stream,
			Err(_) => {
				error!("Failed to get stream");
				return
			},
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
				let current_block_number =
					self.raiden.state_manager.read().current_state.block_number;
				let block_state_change = Block {
					block_number: block_number.into(),
					block_hash,
					gas_limit: header.gas_limit,
				};
				if let Err(e) =
					self.transition_service.transition(vec![block_state_change.into()]).await
				{
					error!("{}", e);
				}
				self.sync_service.sync(current_block_number, block_number.into()).await;
			}
		}
	}
}
