#[macro_use]
extern crate slog;
use std::{
	fs,
	path::{
		Path,
		PathBuf,
	},
	process,
	sync::Arc,
};

use cli::RaidenApp;
use raiden_api::raiden::RaidenConfig;
use raiden_blockchain::{
	contracts,
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_cli::utils::get_private_key;
use raiden_pathfinding::{
	self,
	config::PFSConfig,
};
use raiden_primitives::types::ChainID;
use raiden_state_machine::types::MediationFeeConfig;
use raiden_transport::{
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
	},
};
use slog::Drain;
use structopt::StructOpt;
use web3::{
	signing::Key,
	types::Address,
};

use crate::{
	cli::Opt,
	traits::{
		ToHTTPEndpoint,
		ToSocketEndpoint,
	},
};

mod cli;
mod http;
mod services;
mod traits;

#[tokio::main]
async fn main() {
	let cli = Opt::from_args();

	let decorator = slog_term::TermDecorator::new().build();
	let drain = slog_term::FullFormat::new(decorator).build().fuse();
	let drain = slog_async::Async::new(drain).build().fuse();
	let logger = slog::Logger::root(drain, o!());

	match setup_data_directory(cli.datadir.clone()) {
		Err(e) => {
			eprintln!("Error initializing data directory: {}", e);
			process::exit(1);
		},
		_ => {},
	};

	let private_key = match get_private_key(cli.keystore_path.clone()) {
		Ok(result) => result,
		Err(e) => {
			eprintln!("{}", e);
			process::exit(1);
		},
	};

	info!(logger, "Welcome to Raiden");
	info!(logger, "Initializing");

	let chain_id: ChainID = cli.chain_id.into();
	let eth_rpc_http_endpoint = match cli.eth_rpc_endpoint.to_http() {
		Ok(e) => e,
		Err(e) => {
			eprintln!("Invalid RPC endpoint: {}", e);
			process::exit(1);
		},
	};

	let eth_rpc_socket_endpoint = match cli.eth_rpc_socket_endpoint.to_socket() {
		Ok(e) => e,
		Err(e) => {
			eprintln!("Invalid RPC endpoint: {}", e);
			process::exit(1);
		},
	};

	let keystore_path = Path::new(&cli.keystore_path);
	let datadir = match expanduser::expanduser(cli.datadir.to_string_lossy()) {
		Ok(p) => p,
		Err(e) => {
			eprintln!("Error expanding data directory: {}", e);
			process::exit(1);
		},
	};

	let http = web3::transports::Http::new(&eth_rpc_http_endpoint).unwrap();
	let web3 = web3::Web3::new(http);

	let nonce = match web3.eth().transaction_count(private_key.address(), None).await {
		Ok(nonce) => nonce,
		Err(e) => {
			eprintln!("Failed to fetch nonce: {}", e);
			process::exit(1);
		},
	};

	let mediation_config = MediationFeeConfig {
		token_to_flat_fee: cli
			.mediation_fees
			.flat_fee
			.into_iter()
			.map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
			.collect(),
		token_to_proportional_fee: cli
			.mediation_fees
			.proportional_fee
			.into_iter()
			.map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
			.collect(),
		token_to_proportional_imbalance_fee: cli
			.mediation_fees
			.proportional_imbalance_fee
			.into_iter()
			.map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
			.collect(),
		cap_meditation_fees: cli.mediation_fees.cap_mediation_fees,
	};

	let account = Account::new(web3.clone(), private_key, nonce);

	let homeserver_url = if cli.matrix_transport_config.matrix_server == MATRIX_AUTO_SELECT_SERVER {
		let servers = match get_default_matrix_servers(cli.environment_type.into()).await {
			Ok(p) => p,
			Err(e) => {
				eprintln!("Could not fetch default matrix servers: {:?}", e);
				process::exit(1);
			},
		};
		select_best_server(servers)
	} else {
		cli.matrix_transport_config.matrix_server
	};

	let transport_config = TransportConfig {
		retry_timeout: cli.matrix_transport_config.retry_timeout,
		retry_timeout_max: cli.matrix_transport_config.retry_timeout_max,
		retry_count: cli.matrix_transport_config.retry_count,
		matrix: MatrixTransportConfig { homeserver_url },
	};

	let contracts_manager = match contracts::ContractsManager::new(chain_id.clone()) {
		Ok(contracts_manager) => Arc::new(contracts_manager),
		Err(e) => {
			eprintln!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};

	let proxy_manager = match ProxyManager::new(web3.clone(), contracts_manager.clone()) {
		Ok(pm) => Arc::new(pm),
		Err(e) => {
			eprintln!("Failed to initialize proxy manager: {}", e);
			process::exit(1);
		},
	};

	let service_registry_deployed_contract =
		match contracts_manager.get_deployed(contracts::ContractIdentifier::ServiceRegistry) {
			Ok(contract) => contract,
			Err(e) => {
				eprintln!("Could not find service registry deployment info {:?}", e);
				process::exit(1);
			},
		};

	let service_registry =
		match proxy_manager.service_registry(service_registry_deployed_contract.address).await {
			Ok(sr) => sr,
			Err(e) => {
				eprintln!("Could not create service registry {:?}", e);
				process::exit(1);
			},
		};

	let pfs_info = match raiden_pathfinding::configure_pfs(
		cli.services_config.clone().into(),
		service_registry.clone(),
	)
	.await
	{
		Ok(pfs_info) => pfs_info,
		Err(e) => {
			eprintln!("Failed to initialize PFS: {}", e);
			process::exit(1);
		},
	};

	let config = RaidenConfig {
		chain_id,
		account,
		datadir,
		mediation_config,
		transport_config,
		keystore_path: keystore_path.to_path_buf(),
		eth_http_rpc_endpoint: eth_rpc_http_endpoint,
		eth_socket_rpc_endpoint: eth_rpc_socket_endpoint,
		pfs_config: PFSConfig {
			url: cli.services_config.pathfinding_service_specific_address,
			info: pfs_info,
			maximum_fee: cli.services_config.pathfinding_max_fee,
			iou_timeout: cli.services_config.pathfinding_iou_timeout.into(),
			max_paths: cli.services_config.pathfinding_max_paths,
		},
	};

	let matrix_client = MatrixClient::new(
		config.transport_config.matrix.homeserver_url.clone(),
		config.account.private_key(),
	)
	.await;

	if let Err(e) = matrix_client.init().await {
		eprintln!("Failed to initialize Matrix client: {}", e);
		process::exit(1);
	};

	let raiden_app = match RaidenApp::new(
		config,
		web3,
		matrix_client,
		contracts_manager,
		proxy_manager,
		logger.clone(),
	) {
		Ok(app) => app,
		Err(e) => {
			eprintln!("Error initializing app: {}", e);
			process::exit(1);
		},
	};

	info!(logger, "Raiden is starting");
	raiden_app.run().await;
}

fn setup_data_directory(path: PathBuf) -> Result<PathBuf, String> {
	let path = expanduser::expanduser(path.to_string_lossy())
		.map_err(|_| "Failed to expand data directory".to_owned())?;

	if !path.is_dir() {
		return Err("Datadir has to be a directory".to_owned())
	}

	if !path.exists() {
		match fs::create_dir(path.clone()) {
			Err(e) =>
				return Err(format!("Could not create directory: {:?} because {}", path.clone(), e)),
			_ => {},
		}
	}
	Ok(path.to_path_buf())
}
