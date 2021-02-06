use crate::{
	blockchain::{
		contracts::{
			self,
			Contract,
			ContractIdentifier
		},
		events::Event
	},
	cli,
	enums::{ChainID, StateChange},
	event_handler::EventHandler,
	state::{Result, StateManager},
	transfer::{
		self,
		views,
	},
};
use futures::{
	FutureExt,
	SinkExt,
	StreamExt,
	channel::mpsc,
};
use std::{process, sync::{Arc, Mutex}};
use ethsign::SecretKey;
use rusqlite::Connection;
use slog::Logger;
use tokio::{select, sync::RwLock};
use web3::{transports::WebSocket, types::{Address, U64}};

pub struct RaidenService {
    pub chain_id: ChainID,
    pub our_address: Address,
    pub secret_key: SecretKey,
    pub web3: web3::Web3<web3::transports::Http>,
    pub contracts_registry: contracts::ContractRegistry,
	transition_tx: mpsc::UnboundedSender<StateChange>,
	transition_rx: mpsc::UnboundedReceiver<StateChange>,
    state_manager: Arc<RwLock<StateManager>>,
    log: Logger,
}

impl RaidenService {
    pub fn new(
        web3: web3::Web3<web3::transports::Http>,
        chain_id: ChainID,
        our_address: Address,
        secret_key: SecretKey,
        log: Logger,
    ) -> RaidenService {
        let conn = match Connection::open("raiden.db") {
            Ok(conn) => Arc::new(Mutex::new(conn)),
            Err(e) => {
                crit!(log, "Could not connect to database: {}", e);
                process::exit(1)
            }
        };

        let state_manager = StateManager::new(Arc::clone(&conn));

        if let Err(e) = state_manager.setup() {
            crit!(log, "Could not setup database: {}", e);
            process::exit(1)
        }
        let contracts_registry = contracts::ContractRegistry::new(chain_id.clone()).unwrap();

		let (transition_tx, transition_rx) = mpsc::unbounded::<StateChange>();

        RaidenService {
            web3,
            chain_id,
            our_address,
            secret_key,
            contracts_registry,
            state_manager: Arc::new(RwLock::new(state_manager)),
			transition_tx,
			transition_rx,
            log,
        }
    }

	pub async fn initialize(&self) {
        let mut state_manager = self.state_manager.write().await;
		let token_network_registry = self.contracts_registry.token_network_registry();

        match state_manager.restore_or_init_state(
			self.chain_id.clone(),
			self.our_address.clone(),
			token_network_registry.address,
			token_network_registry.deploy_block_number,
		) {
			Ok(_) => {
				debug!(self.log, "Restored state");
			},
			Err(_) => {
				debug!(
					self.log,
					"Initialized node",
				);
			}
		};
    }

	pub async fn sync_with_blockchain(&mut self) {
		debug!(self.log, "Sync started");
		debug!(self.log, "Sync TokenNetworkRegistry");

		let block_number = views::block_number(&self.state_manager.read().await.current_state.as_ref().unwrap());

		let token_network_registry = self.contracts_registry.token_network_registry();
		self.poll_contract_filters(
			&token_network_registry,
			token_network_registry.deploy_block_number,
			block_number,
		).await;

		let token_networks = self.contracts_registry.contracts.get(&ContractIdentifier::TokenNetwork).unwrap_or(&vec![]).clone();
		debug!(self.log, "Token network filters {}", token_networks.len());
		for token_network in token_networks {
			debug!(self.log, "Sync TokenNetwork {}", token_network.address);
			self.poll_contract_filters(
				&token_network,
				token_network.deploy_block_number,
				block_number,
			).await;
		}
		debug!(self.log, "Sync finished");
	}

	pub async fn poll_contract_filters(
		&mut self,
		contract: &Contract,
		from_block: U64,
		to_block: U64,
	) {
		debug!(self.log, "Contract filters {}", contract.name);
        let filter = contract.filters(from_block, to_block);
		match self.web3.eth().logs((filter).clone()).await {
			Ok(logs) => {
				for log in logs {
					// let _ = self.filters_tx.clone().send(log).await;
					let current_state = self.state_manager.read().await.current_state.clone();
					let contracts_registry = &self.contracts_registry;
					// TODO: Event::to_state_change doesn't make sense
					// TODO: Make trait ToStateChange and implement on Log
					let state_change = Event::to_state_change(&current_state, contracts_registry, &log);
					if let Some(state_change) = state_change {
						if let Err(e) = self.transition(state_change).await {
							error!(self.log, "State transition failed: {}", e);
						}
						// let _ = self.transition_tx.clone().send(state_change).await;
					}
				}
			},
			Err(e) => error!(self.log, "Error fetching logs {}", e),
		}
	}
	
    pub async fn start(mut self, config: cli::Config<'_>) {
        debug!(
            self.log,
            "Chain State {:?}",
            self.state_manager.read().await.current_state
        );

		self.sync_with_blockchain().await;

		// TODO: I don't like this.
		// There should be a blocking version to poll / process all logs
		// before the actual tasks are started.
		// self.poll_filters().await;

		let mut blocks_receiver = self.create_blocks_monitor(config.eth_socket_rpc_endpoint).await.unwrap();
		loop {
			select! {
				// log = self.filters_rx.next().fuse() => {
				// 	let log = match log {
				// 		Some(sc) => sc,
				// 		None => continue,
				// 	};
				// 	let current_state = self.state_manager.read().await.current_state.clone();
				// 	let contracts_registry = &self.contracts_registry;
				// 	if let Some(state_change) = Event::to_state_change(&current_state, contracts_registry, &log) {
				// 		let _ = self.transition_tx.clone().send(state_change).await;
				// 	}
				// }
				state_change = self.transition_rx.next().fuse() => {
					let state_change = match state_change {
						Some(sc) => sc,
						None => continue,
					};
					debug!(self.log, "State transition {:#?}", state_change);
					if let Err(e) = self.transition(state_change).await {
						error!(self.log, "State transition failed: {}", e);
					}
				},
				block = blocks_receiver.next().fuse() => {
					let block = match block {
						Some(block) => block,
						None => continue,
					};

					debug!(self.log, "Received block"; "number" => block.to_string());

					let block_state_change =
						transfer::state_change::Block::new(self.chain_id.clone(), block.into());

					let _ = self.transition_tx.clone().send(StateChange::Block(block_state_change)).await;
				},
			}
		}
    }

	pub async fn create_blocks_monitor(&self, eth_socket_rpc_endpoint: String) -> Option<mpsc::UnboundedReceiver<U64>> {
		let (mut blocks_tx, blocks_rx) = mpsc::unbounded::<U64>();
        let ws = match WebSocket::new(&eth_socket_rpc_endpoint).await {
			Ok(ws) => ws,
			Err(_) => return None,
		};

        let web3 = web3::Web3::new(ws);

        let block_stream = web3.eth_subscribe().subscribe_new_heads().await;
		tokio::spawn(async move {
			if let Ok(mut stream) = block_stream {
				while let Some(subscription) = stream.next().await {
					if let Ok(subscription) = subscription {
						if let Some(block_number) = subscription.number {
							let _ = blocks_tx.send(block_number).await;
						}
					}
				}
			}
		});
		Some(blocks_rx)
    }

    pub async fn transition(&mut self, state_change: StateChange) -> Result<bool> {
		let transition_result = self.state_manager.write().await.transition(state_change);
		match transition_result {
			Ok(events) => {
				for event in events {
					if EventHandler::handle_event(self, event).await {
						// self.poll_filters().await;
					}
				}
				Ok(true)
			}
			Err(e) => Err(e),
		}
    }
}
