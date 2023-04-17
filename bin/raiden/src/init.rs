use std::{
	path::PathBuf,
	sync::Arc,
};

use parking_lot::RwLock as SyncRwLock;
use raiden_blockchain::{
	contracts::{
		ContractIdentifier,
		ContractsManager,
	},
	proxies::{
		Account,
		ProxyManager,
		ServiceRegistryProxy,
	},
};
use raiden_network_messages::messages::TransportServiceMessage;
use raiden_network_transport::{
	config::TransportConfig,
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
	AddressMetadata,
	BlockNumber,
	ChainID,
	DefaultAddresses,
};
use raiden_state_machine::{
	types::{
		ChannelStatus,
		Event,
		FeeScheduleState,
		MediationFeeConfig,
		PFSUpdate,
	},
	views,
};
use raiden_storage::{
	matrix::MatrixStorage,
	state::StateStorage,
};
use raiden_transition::{
	events::EventHandler,
	manager::StateManager,
};
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
	let default_addresses = contracts_manager
		.deployed_addresses()
		.map_err(|e| format!("Failed to construct default deployed addresses: {:?}", e))?;

	let token_network_registry_contract = contracts_manager
		.get_deployed(ContractIdentifier::TokenNetworkRegistry)
		.map_err(|e| format!("Could not find token network registry contract: {:?}", e))?;
	let (state_manager, block_number) = StateManager::restore_or_init_state(
		storage,
		chain_id,
		account.address(),
		default_addresses.token_network_registry,
		token_network_registry_contract.block,
	)
	.map_err(|e| format!("Failed to initialize state: {}", e))?;

	Ok((Arc::new(SyncRwLock::new(state_manager)), block_number, default_addresses))
}

pub async fn init_channel_fees(
	state_manager: Arc<SyncRwLock<StateManager>>,
	event_handler: EventHandler,
	registry_address: Address,
	mut fee_config: MediationFeeConfig,
) {
	let mut chain_state = state_manager.read().current_state.clone();
	let token_addresses = views::get_token_identifiers(&chain_state, registry_address);
	let token_network_registry =
		match chain_state.identifiers_to_tokennetworkregistries.get_mut(&registry_address) {
			Some(tnr) => tnr,
			None => return,
		};

	for token_address in token_addresses {
		let token_network = match token_network_registry
			.tokennetworkaddresses_to_tokennetworks
			.values_mut()
			.find(|tn| tn.token_address == token_address)
		{
			Some(tn) => tn,
			None => continue,
		};

		for channel in token_network.channelidentifiers_to_channels.values_mut() {
			if channel.status() != ChannelStatus::Opened {
				continue
			}

			let flat_fee = fee_config.get_flat_fee(&channel.token_address);
			let proportional_fee = fee_config.get_proportional_fee(&channel.token_address);
			let _proportional_imbalance_fee =
				fee_config.get_proportional_imbalance_fee(&channel.token_address);
			// let imbalance_penalty =
			// 	calculate_imbalance_fees(channel.capacity(), proportional_imbalance_fee);
			let imbalance_penalty = Some(vec![]);
			channel.fee_schedule = FeeScheduleState {
				cap_fees: fee_config.cap_meditation_fees,
				flat: flat_fee,
				proportional: proportional_fee,
				imbalance_penalty,
			};

			event_handler
				.handle_event(Event::SendPFSUpdate(PFSUpdate {
					canonical_identifier: channel.canonical_identifier.clone(),
					update_fee_schedule: true,
				}))
				.await;
		}
	}

	state_manager.write().current_state = chain_state.clone();
}

pub async fn init_transport(
	environment_type: EnvironmentType,
	transport_config: TransportConfig,
	account: Account<Http>,
	storage_path: PathBuf,
	service_registry_proxy: ServiceRegistryProxy<Http>,
) -> Result<(MatrixService, UnboundedSender<TransportServiceMessage>, AddressMetadata), String> {
	let homeserver_url = if transport_config.matrix.homeserver_url == MATRIX_AUTO_SELECT_SERVER {
		let servers = get_default_matrix_servers(environment_type)
			.await
			.map_err(|e| format!("Could not fetch default matrix servers: {:?}", e))?;
		select_best_server(servers)
	} else {
		transport_config.matrix.homeserver_url.clone()
	};

	let conn = Connection::open(storage_path.join("raiden.db"))
		.map_err(|e| format!("Could not connect to database: {}", e))?;
	let storage = MatrixStorage::new(conn);
	storage
		.setup_database()
		.map_err(|e| format!("Failed to setup storage: {}", e))?;

	let mut matrix_client = MatrixClient::new(homeserver_url, account.private_key()).await;
	let _ = matrix_client.populate_services_addresses(service_registry_proxy).await;

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
	default_addresses: DefaultAddresses,
	proxy_manager: Arc<ProxyManager>,
	services_config: ServicesConfig,
) -> Result<PFSInfo, String> {
	let service_registry = proxy_manager
		.service_registry(default_addresses.service_registry)
		.await
		.map_err(|e| format!("Could not create service registry {:?}", e))?;

	Ok(raiden_pathfinding::configure_pfs(services_config, service_registry)
		.await
		.map_err(|e| format!("Failed to initialize PFS: {}", e))?)
}
