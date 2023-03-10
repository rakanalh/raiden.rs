use std::{
	collections::HashMap,
	fs,
	path::PathBuf,
	sync::Arc,
};

use parking_lot::RwLock as SyncRwLock;
use raiden_api::raiden::DefaultAddresses;
use raiden_blockchain::{
	contracts::{
		self,
		ContractsManager,
	},
	keys::PrivateKey,
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_client::cli::list_keys;
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
	Address,
	BlockNumber,
	ChainID,
};
use raiden_state_machine::types::AddressMetadata;
use raiden_storage::Storage;
use raiden_transition::manager::StateManager;
use rusqlite::Connection;
use tokio::sync::mpsc::UnboundedSender;
use web3::transports::Http;

use crate::cli::prompt_key;

pub fn init_private_key(
	keystore_path: PathBuf,
	address: Option<Address>,
	password_file: Option<PathBuf>,
) -> Result<PrivateKey, String> {
	let keys = list_keys(&keystore_path).map_err(|e| format!("Could not list accounts: {}", e))?;
	let key_filename = if let Some(address) = address {
		let inverted_keys: HashMap<Address, String> =
			keys.iter().map(|(k, v)| (v.clone(), k.clone())).collect();
		inverted_keys.get(&address).unwrap().clone()
	} else {
		prompt_key(&keys)
	};

	let password = if let Some(password_file) = password_file {
		fs::read_to_string(password_file)
			.map_err(|e| format!("Error reading password file: {:?}", e))?
			.trim()
			.to_owned()
	} else {
		rpassword::read_password_from_tty(Some("Password: "))
			.map_err(|e| format!("Could not read password: {:?}", e))?
	};

	PrivateKey::new(key_filename.clone(), password)
		.map_err(|e| format!("Could not unlock private key: {:?}", e))
}

pub fn init_storage(datadir: PathBuf) -> Result<Arc<Storage>, String> {
	let conn = Connection::open(datadir.join("raiden.db"))
		.map_err(|e| format!("Could not connect to database: {}", e))?;

	let storage = Arc::new(Storage::new(conn));
	storage
		.setup_database()
		.map_err(|e| format!("Failed to setup storage: {}", e))?;

	Ok(storage)
}

pub fn init_state_manager(
	contracts_manager: Arc<ContractsManager>,
	storage: Arc<Storage>,
	chain_id: ChainID,
	account: Account<Http>,
) -> Result<(Arc<SyncRwLock<StateManager>>, BlockNumber, DefaultAddresses), String> {
	let token_network_registry_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::TokenNetworkRegistry)
		.map_err(|e| format!("Could not find token network registry deployment info: {:?}", e))?;

	let secret_registry_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::SecretRegistry)
		.map_err(|e| format!("Could not find secret registry deployment info: {:?}", e))?;

	let one_to_n_deployed_contract = contracts_manager
		.get_deployed(contracts::ContractIdentifier::OneToN)
		.map_err(|e| format!("Could not find OneToN deployment info: {:?}", e))?;

	let default_addresses = DefaultAddresses {
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
	let matrix_client = MatrixClient::new(homeserver_url, account.private_key()).await;

	matrix_client
		.init()
		.await
		.map_err(|e| format!("Failed to initialize Matrix client: {}", e))?;

	let our_metadata = matrix_client.address_metadata();

	let (transport_service, sender) = MatrixService::new(transport_config, matrix_client);

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
