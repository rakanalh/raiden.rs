use std::{
	fs,
	net::SocketAddr,
	path::PathBuf,
	process,
	sync::Arc,
};

use futures::FutureExt;
use raiden_api::{
	api::Api,
	payments::PaymentsRegistry,
	raiden::{
		Raiden,
		RaidenConfig,
	},
};
use raiden_bin_common::init_private_key;
use raiden_blockchain::{
	contracts,
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_client::services::{
	BlockMonitorService,
	SyncService,
};
use raiden_pathfinding::{
	self,
	config::PFSConfig,
};
use raiden_primitives::types::ChainID;
use raiden_state_machine::types::MediationFeeConfig;
use raiden_transition::{
	events::EventHandler,
	messages::MessageHandler,
	Transitioner,
};
use structopt::StructOpt;
use tokio::{
	select,
	signal::unix::{
		signal,
		SignalKind,
	},
	sync::RwLock,
};
use tracing::info;
use tracing_subscriber::filter::EnvFilter;
use web3::{
	signing::Key,
	transports::WebSocket,
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
mod init;
mod traits;

use init::*;

#[tokio::main]
async fn main() {
	let cli = Opt::from_args();

	let filter = EnvFilter::from_env("RAIDEN_LOG")
		.add_directive("raiden_api=debug".parse().unwrap())
		.add_directive("raiden_blockchain=debug".parse().unwrap())
		.add_directive("raiden_client=debug".parse().unwrap())
		.add_directive("raiden_state_machine=debug".parse().unwrap())
		.add_directive("raiden_storage=debug".parse().unwrap())
		.add_directive("raiden_transition=debug".parse().unwrap())
		.add_directive("raiden_network_messages=debug".parse().unwrap())
		.add_directive("raiden_network_transport=debug".parse().unwrap());

	let subscriber = tracing_subscriber::fmt()
		.with_env_filter(filter)
		.pretty()
		.with_file(false)
		.with_line_number(false)
		.with_thread_ids(false)
		.with_target(true)
		.finish();

	let _ = tracing::subscriber::set_global_default(subscriber);

	match setup_data_directory(cli.datadir.clone()) {
		Err(e) => {
			eprintln!("Error initializing data directory: {}", e);
			process::exit(1);
		},
		_ => {},
	};

	info!("Welcome to Raiden");

	// #
	// # Initialize chain related components
	// #
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

	// #
	// # Initialize web3
	// #
	let http = web3::transports::Http::new(&eth_rpc_http_endpoint).unwrap();
	let web3 = web3::Web3::new(http);
	let private_key = match init_private_key(
		web3.clone(),
		cli.keystore_path.clone(),
		cli.address,
		cli.password_file,
	)
	.await
	{
		Ok(key) => key,
		Err(e) => {
			eprintln!("{}", e);
			process::exit(1);
		},
	};
	let nonce = match web3.eth().transaction_count(private_key.address(), None).await {
		Ok(nonce) => nonce - 1,
		Err(e) => {
			eprintln!("Failed to fetch nonce: {}", e);
			process::exit(1);
		},
	};
	let account = Account::new(web3.clone(), private_key, nonce);

	// #
	// # Initialize state manager
	// #
	let datadir = match expanduser::expanduser(cli.datadir.to_string_lossy()) {
		Ok(p) => p,
		Err(e) => {
			eprintln!("Error expanding data directory: {}", e);
			process::exit(1);
		},
	};

	let storage = match init_storage(datadir.clone()) {
		Ok(storage) => storage,
		Err(e) => {
			eprintln!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};

	let contracts_manager = match contracts::ContractsManager::new(chain_id.clone()) {
		Ok(contracts_manager) => Arc::new(contracts_manager),
		Err(e) => {
			eprintln!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};
	let (state_manager, sync_start_block_number, default_addresses) =
		match init_state_manager(contracts_manager.clone(), storage, chain_id, account.clone()) {
			Ok(result) => result,
			Err(e) => {
				eprintln!("Error initializing state: {:?}", e);
				process::exit(1);
			},
		};

	// #
	// # Initialize PFS
	// #
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

	let proxy_manager = match ProxyManager::new(web3.clone(), contracts_manager.clone()) {
		Ok(pm) => Arc::new(pm),
		Err(e) => {
			eprintln!("Failed to initialize proxy manager: {}", e);
			process::exit(1);
		},
	};

	// #
	// # Initialize transport
	// #
	let services_registry_proxy =
		match proxy_manager.service_registry(default_addresses.service_registry).await {
			Ok(proxy) => proxy,
			Err(e) => {
				eprintln!("Could not instantiate services registry: {:?}", e);
				process::exit(1);
			},
		};
	let (transport_service, transport_sender, our_metadata) = match init_transport(
		cli.environment_type.into(),
		cli.matrix_transport_config.matrix_server,
		cli.matrix_transport_config.retry_timeout,
		cli.matrix_transport_config.retry_count,
		cli.matrix_transport_config.retry_timeout_max,
		account.clone(),
		datadir,
		services_registry_proxy,
	)
	.await
	{
		Ok(result) => result,
		Err(e) => {
			eprintln!("{}", e);
			process::exit(1);
		},
	};

	let pfs_info = match init_pfs_info(
		contracts_manager.clone(),
		proxy_manager.clone(),
		cli.services_config.clone().into(),
	)
	.await
	{
		Ok(info) => info,
		Err(e) => {
			eprintln!("{}", e);
			process::exit(1);
		},
	};

	// #
	// # Initialize Raiden
	// #
	//
	let config = RaidenConfig {
		chain_id,
		mediation_config,
		account: account.clone(),
		metadata: our_metadata,
		pfs_config: PFSConfig {
			url: cli.services_config.pathfinding_service_address.clone(),
			info: pfs_info,
			maximum_fee: cli.services_config.pathfinding_max_fee,
			iou_timeout: cli.services_config.pathfinding_iou_timeout.into(),
			max_paths: cli.services_config.pathfinding_max_paths,
		},
		addresses: default_addresses.clone(),
	};
	let raiden = Arc::new(Raiden {
		web3,
		config: config.clone(),
		contracts_manager,
		proxy_manager: proxy_manager.clone(),
		state_manager: state_manager.clone(),
		transport: transport_sender.clone(),
	});

	let event_handler = EventHandler::new(
		account.clone(),
		state_manager.clone(),
		proxy_manager.clone(),
		transport_sender.clone(),
		default_addresses.clone(),
	);
	let transitioner = Arc::new(Transitioner::new(state_manager.clone(), event_handler.clone()));
	let message_handler = MessageHandler::new(
		account.private_key(),
		cli.services_config.pathfinding_service_address,
		transport_sender.clone(),
		state_manager.clone(),
		transitioner.clone(),
	);

	let ws = match WebSocket::new(&eth_rpc_socket_endpoint).await {
		Ok(ws) => ws,
		Err(e) => {
			eprintln!("Error connecting to websocket: {:?}", e);
			process::exit(1);
		},
	};

	init_channel_fees(
		state_manager,
		event_handler,
		default_addresses.token_network_registry,
		config.mediation_config.clone(),
	)
	.await;

	let mut sync_service = SyncService::new(raiden.clone(), transitioner.clone());
	let latest_block_number = raiden.web3.eth().block_number().await.unwrap();

	info!("Performing initial sync from {} to {}", sync_start_block_number, latest_block_number);
	sync_service.sync(sync_start_block_number, latest_block_number.into()).await;

	let block_monitor_service =
		match BlockMonitorService::new(raiden.clone(), ws, transitioner.clone(), sync_service) {
			Ok(service) => service,
			Err(_) => {
				eprintln!("Could not initialize block monitor service");
				process::exit(1);
			},
		};
	let payments_registry = Arc::new(RwLock::new(PaymentsRegistry::new()));
	let api = Api::new(raiden.clone(), transitioner.clone(), payments_registry);

	let socket: SocketAddr = match format!("{}:{}", cli.http_host, cli.http_port).parse() {
		Ok(socket) => socket,
		Err(e) => {
			eprintln!("Error starting HTTP server: {:?}", e);
			process::exit(1);
		},
	};
	let http_service = crate::http::HttpServer::new(socket, raiden, Arc::new(api));

	info!("Raiden is starting");

	let mut hangup = match signal(SignalKind::interrupt()) {
		Ok(s) => s,
		Err(e) => {
			eprintln!("Could not instantiate listener for hangup signal: {:?}", e);
			return
		},
	};
	select! {
		_ = block_monitor_service.start().fuse() => {},
		_ = transport_service.run(message_handler).fuse() => {},
		_  = http_service.start().fuse() => {},
		_ = hangup.recv().fuse() => {
			println!("Raiden is stopping");
			return
		},
	};
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
