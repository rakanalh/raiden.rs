use std::{
	path::PathBuf,
	sync::Arc,
};

use parking_lot::RwLock as SyncRwLock;
use raiden_blockchain::{
	contracts::{
		self,
		ContractsManager,
	},
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_network_messages::messages::TransportServiceMessage;
use raiden_network_transport::{
	config::{
		MatrixTransportConfig,
		TransportConfig,
	},
	matrix::{
		constants::MATRIX_AUTO_SELECT_SERVER,
		utils::{
			get_default_matrix_servers,
			select_best_server,
		},
		MatrixClient,
		MatrixService,
	},
	types::EnvironmentType,
};
use raiden_pathfinding::config::{
	PFSInfo,
	ServicesConfig,
};
use raiden_primitives::types::{
	AddressMetadata,
	BlockNumber,
	ChainID,
	DefaultAddresses,
};
use raiden_storage::{
	matrix::MatrixStorage,
	state::StateStorage,
};
use raiden_transition::manager::StateManager;
use rusqlite::Connection;
use tokio::sync::mpsc::UnboundedSender;
use web3::transports::Http;

pub fn init_storage(datadir: PathBuf) -> Result<Arc<StateStorage>, String> {
	let conn = Connection::open(datadir.join("raiden.db"))
		.map_err(|e| format!("Could not connect to database: {}", e))?;

	let storage = Arc::new(StateStorage::new(conn));
	storage
		.setup_database()
		.map_err(|e| format!("Failed to setup storage: {}", e))?;

	Ok(storage)
}

pub fn init_state_manager(
	contracts_manager: Arc<ContractsManager>,
	storage: Arc<StateStorage>,
	chain_id: ChainID,
	account: Account<Http>,
) -> Result<(Arc<SyncRwLock<StateManager>>, BlockNumber, DefaultAddresses), String> {
	let token_network_registry_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::TokenNetworkRegistry)
		.map_err(|e| format!("Could not find token network registry deployment info: {:?}", e))?;

	let secret_registry_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::SecretRegistry)
		.map_err(|e| format!("Could not find secret registry deployment info: {:?}", e))?;

	let service_registry_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::ServiceRegistry)
		.map_err(|e| format!("Could not find service registry deployment info: {:?}", e))?;

	let one_to_n_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::OneToN)
		.map_err(|e| format!("Could not find OneToN deployment info: {:?}", e))?;

	let default_addresses = DefaultAddresses {
		service_registry: service_registry_deployed_contract.address,
		secret_registry: secret_registry_deployed_contract.address,
		token_network_registry: token_network_registry_deployed_contract.address,
		one_to_n: one_to_n_deployed_contract.address,
	};

	let (state_manager, block_number) = StateManager::restore_or_init_state(
		storage,
		chain_id,
		account.address(),
		token_network_registry_deployed_contract.address,
		token_network_registry_deployed_contract.block,
	)
	.map_err(|e| format!("Failed to initialize state: {}", e))?;

	Ok((Arc::new(SyncRwLock::new(state_manager)), block_number, default_addresses))
}

pub async fn init_transport(
	environment_type: EnvironmentType,
	homeserver_url: String,
	retry_timeout: u8,
	retry_count: u32,
	retry_timeout_max: u8,
	account: Account<Http>,
	storage_path: PathBuf,
) -> Result<(MatrixService, UnboundedSender<TransportServiceMessage>, AddressMetadata), String> {
	let homeserver_url = if homeserver_url == MATRIX_AUTO_SELECT_SERVER {
		let servers = get_default_matrix_servers(environment_type)
			.await
			.map_err(|e| format!("Could not fetch default matrix servers: {:?}", e))?;
		select_best_server(servers)
	} else {
		homeserver_url
	};
	let transport_config = TransportConfig {
		retry_timeout,
		retry_timeout_max,
		retry_count,
		matrix: MatrixTransportConfig { homeserver_url: homeserver_url.clone() },
	};

	let conn = Connection::open(storage_path.join("raiden.db"))
		.map_err(|e| format!("Could not connect to database: {}", e))?;
	let storage = MatrixStorage::new(conn);
	storage
		.setup_database()
		.map_err(|e| format!("Failed to setup storage: {}", e))?;

	let matrix_client = MatrixClient::new(homeserver_url, account.private_key()).await;

	matrix_client
		.init()
		.await
		.map_err(|e| format!("Failed to initialize Matrix client: {}", e))?;

	let our_metadata = matrix_client.address_metadata();

	let (mut transport_service, sender) =
		MatrixService::new(transport_config, matrix_client, storage);

	transport_service.init_from_storage()?;

	Ok((transport_service, sender, our_metadata))
}

pub async fn init_pfs_info(
	contracts_manager: Arc<ContractsManager>,
	proxy_manager: Arc<ProxyManager>,
	services_config: ServicesConfig,
) -> Result<PFSInfo, String> {
	let service_registry_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::ServiceRegistry)
		.map_err(|e| format!("Could not find service registry deployment info {:?}", e))?;

	let service_registry = proxy_manager
		.service_registry(service_registry_deployed_contract.address)
		.await
		.map_err(|e| format!("Could not create service registry {:?}", e))?;

	Ok(raiden_pathfinding::configure_pfs(services_config, service_registry)
		.await
		.map_err(|e| format!("Failed to initialize PFS: {}", e))?)
}
