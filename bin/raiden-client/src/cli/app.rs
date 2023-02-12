use crate::services::{BlockMonitorService, SyncService};
use parking_lot::RwLock as SyncRwLock;
use raiden::{
	api::Api,
	blockchain::{
		contracts::{self, ContractsManager},
		proxies::ProxyManager,
	},
	event_handler::EventHandler,
	payments::PaymentsRegistry,
	primitives::{RaidenConfig, U64},
	raiden::{DefaultAddresses, Raiden},
	services::TransitionService,
	state_manager::StateManager,
	storage::Storage,
	transport::matrix::{MatrixClient, MatrixService},
};
use rusqlite::Connection;
use slog::Logger;
use std::sync::Arc;
use tokio::sync::RwLock;
use web3::{
	transports::{Http, WebSocket},
	Web3,
};

type Result<T> = std::result::Result<T, String>;

pub struct RaidenApp {
	raiden: Arc<Raiden>,
	sync_start_block_number: U64,
}

impl RaidenApp {
	pub fn new(
		config: RaidenConfig,
		web3: Web3<Http>,
		matrix_client: MatrixClient,
		contracts_manager: Arc<ContractsManager>,
		proxy_manager: Arc<ProxyManager>,
		logger: Logger,
	) -> Result<Self> {
		let conn = match Connection::open(config.datadir.join("raiden.db")) {
			Ok(conn) => conn,
			Err(e) => return Err(format!("Could not connect to database: {}", e)),
		};
		let storage = Arc::new(Storage::new(conn));
		storage.setup_database().map_err(|e| format!("Failed to setup storage {}", e))?;

		let token_network_registry_deployed_contract = match contracts_manager
			.get_deployed(contracts::ContractIdentifier::TokenNetworkRegistry)
		{
			Ok(contract) => contract,
			Err(e) =>
				return Err(format!(
					"Could not find token network registry deployment info: {:?}",
					e
				)),
		};

		let one_to_n_deployed_contract =
			match contracts_manager.get_deployed(contracts::ContractIdentifier::OneToN) {
				Ok(contract) => contract,
				Err(e) => return Err(format!("Could not find OneToN deployment info {:?}", e)),
			};

		debug!(logger, "Restore state");
		let (state_manager, sync_start_block_number) = match StateManager::restore_or_init_state(
			storage,
			config.chain_id.clone(),
			config.account.address(),
			token_network_registry_deployed_contract.address,
			token_network_registry_deployed_contract.block,
		) {
			Ok((state_manager, block_number)) =>
				(Arc::new(SyncRwLock::new(state_manager)), block_number),
			Err(e) => return Err(format!("Failed to initialize state {}", e)),
		};

		let transport = Arc::new(matrix_client);

		let raiden = Arc::new(Raiden {
			web3,
			config,
			contracts_manager,
			proxy_manager,
			state_manager,
			transport,
			logger,
			addresses: DefaultAddresses {
				token_network_registry: token_network_registry_deployed_contract.address,
				one_to_n: one_to_n_deployed_contract.address,
			},
		});

		Ok(Self { raiden, sync_start_block_number })
	}

	pub async fn run(&self) {
		let latest_block_number = self.raiden.web3.eth().block_number().await.unwrap();

		let ws = match WebSocket::new(&self.raiden.config.eth_socket_rpc_endpoint).await {
			Ok(ws) => ws,
			Err(e) => {
				debug!(self.raiden.logger, "Error connecting to websocket: {:?}", e);
				return
			},
		};

		let sm = self.raiden.state_manager.clone();
		let account = self.raiden.config.account.clone();

		let (transport_service, sender) = MatrixService::new(
			self.raiden.config.transport_config.clone(),
			self.raiden.transport.clone(),
		);

		let transition_service =
			Arc::new(TransitionService::new(self.raiden.state_manager.clone(), move |event| {
				let event_handler = EventHandler::new(account.clone(), sm.clone(), sender.clone());
				async move { event_handler.handle_event(event).await }
			}));

		let mut sync_service = SyncService::new(self.raiden.clone(), transition_service.clone());

		let payments_registry = Arc::new(RwLock::new(PaymentsRegistry::new()));
		let api =
			Api::new(self.raiden.clone(), transition_service.clone(), payments_registry.clone());

		sync_service
			.sync(self.sync_start_block_number, latest_block_number.into())
			.await;

		let block_monitor = match BlockMonitorService::new(
			self.raiden.clone(),
			ws,
			transition_service.clone(),
			sync_service,
		) {
			Ok(bm) => bm,
			Err(_) => return,
		};

		let http_service = crate::http::HttpServer::new(self.raiden.clone(), Arc::new(api));

		futures::join!(block_monitor.start(), transport_service.run(), http_service.start());
	}
}
