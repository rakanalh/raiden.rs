use std::{
	fs,
	net::SocketAddr,
	path::PathBuf,
	process,
	str::FromStr,
	sync::Arc,
};

use futures::FutureExt;
use raiden_api::{
	api::Api,
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
	config::{
		PFSConfig,
		ServicesConfig,
	},
};
use raiden_primitives::{
	payments::PaymentsRegistry,
	traits::{
		Checksum,
		ToPexAddress,
	},
	types::ChainID,
};
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
	sync::{
		mpsc,
		RwLock,
	},
};
use tracing::info;
use tracing_subscriber::{
	filter::EnvFilter,
	prelude::*,
};
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

	// Setup logging
	let mut layers = vec![];
	if cli.log_json && cli.log_file.is_none() {
		layers.push(
			tracing_subscriber::fmt::layer()
				.json()
				.with_file(false)
				.with_line_number(false)
				.with_thread_ids(false)
				.with_target(true)
				.with_filter(get_logging_filter(cli.log_config.clone()))
				.boxed(),
		);
	} else if cli.log_file.is_none() {
		layers.push(
			tracing_subscriber::fmt::layer()
				.pretty()
				.with_file(false)
				.with_line_number(false)
				.with_thread_ids(false)
				.with_target(true)
				.with_filter(get_logging_filter(cli.log_config.clone()))
				.boxed(),
		);
	}
	// Write to file if --log-file is set
	let _guard = if let Some(log_file) = cli.log_file {
		let appender = tracing_appender::rolling::daily(
			log_file.parent().expect("log_file should be a valid path"),
			log_file.file_name().expect("Log should be a file path"),
		);
		let (non_blocking, guard) = tracing_appender::non_blocking(appender);
		layers.push(
			tracing_subscriber::fmt::layer()
				.with_writer(non_blocking)
				.with_filter(get_logging_filter(cli.log_config.clone()))
				.boxed(),
		);
		Some(guard)
	} else {
		None
	};

	tracing_subscriber::registry().with(layers).init();

	info!("Welcome to Raiden");

	// #
	// # Initialize chain related components
	// #
	let chain_id: ChainID = cli.chain_id.into();
	let eth_rpc_http_endpoint = match cli.eth_rpc_endpoint.to_http() {
		Ok(e) => e,
		Err(e) => {
			tracing::error!("Invalid RPC endpoint: {}", e);
			process::exit(1);
		},
	};

	let eth_rpc_socket_endpoint = match cli.eth_rpc_socket_endpoint.to_socket() {
		Ok(e) => e,
		Err(e) => {
			tracing::error!("Invalid RPC endpoint: {}", e);
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
			tracing::error!("{}", e);
			process::exit(1);
		},
	};
	let nonce = match web3.eth().transaction_count(private_key.address(), None).await {
		Ok(nonce) => nonce,
		Err(e) => {
			tracing::error!("Failed to fetch nonce: {}", e);
			process::exit(1);
		},
	};
	let account = Account::new(web3.clone(), private_key, nonce);

	info!(message = "Using account", address = account.address().checksum());

	// #
	// # Initialize state manager
	// #
	let contracts_manager = match contracts::ContractsManager::new(chain_id.clone()) {
		Ok(contracts_manager) => Arc::new(contracts_manager),
		Err(e) => {
			tracing::error!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};
	let default_addresses = match contracts_manager.deployed_addresses() {
		Ok(addresses) => addresses,
		Err(e) => {
			tracing::error!("Failed to construct default deployed addresses: {:?}", e);
			process::exit(1);
		},
	};

	let mut datadir = match expanduser::expanduser(cli.datadir.to_string_lossy()) {
		Ok(p) => p,
		Err(e) => {
			tracing::error!("Error expanding data directory: {}", e);
			process::exit(1);
		},
	};
	datadir.push(format!("node_{}", account.address().pex()));
	datadir.push(format!("netid_{}", chain_id.to_string()));
	datadir.push(format!("network_{}/", default_addresses.token_network_registry.pex()));

	match setup_data_directory(datadir.clone()) {
		Err(e) => {
			tracing::error!("Error initializing data directory: {}", e);
			process::exit(1);
		},
		_ => {},
	};

	let storage = match init_storage(datadir.clone()) {
		Ok(storage) => storage,
		Err(e) => {
			tracing::error!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};

	let (state_manager, sync_start_block_number) = match init_state_manager(
		contracts_manager.clone(),
		default_addresses.clone(),
		storage,
		chain_id,
		account.clone(),
	) {
		Ok(result) => result,
		Err(e) => {
			tracing::error!("Error initializing state: {:?}", e);
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
			.map(|(a, v)| (Address::from_str(&a).expect("Address should be parsable"), v.into()))
			.collect(),
		token_to_proportional_fee: cli
			.mediation_fees
			.proportional_fee
			.into_iter()
			.map(|(a, v)| (Address::from_str(&a).expect("Address should be parsable"), v.into()))
			.collect(),
		token_to_proportional_imbalance_fee: cli
			.mediation_fees
			.proportional_imbalance_fee
			.into_iter()
			.map(|(a, v)| (Address::from_str(&a).expect("Address should be parsable"), v.into()))
			.collect(),
		cap_meditation_fees: cli.mediation_fees.cap_mediation_fees,
	};

	let proxy_manager = match ProxyManager::new(web3.clone(), contracts_manager.clone()) {
		Ok(pm) => Arc::new(pm),
		Err(e) => {
			tracing::error!("Failed to initialize proxy manager: {}", e);
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
				tracing::error!("Could not instantiate services registry: {:?}", e);
				process::exit(1);
			},
		};
	let (transport_service, transport_sender, our_metadata) = match init_transport(
		cli.environment_type.into(),
		cli.matrix_transport_config.into(),
		account.clone(),
		datadir,
		services_registry_proxy,
	)
	.await
	{
		Ok(result) => result,
		Err(e) => {
			tracing::error!("{}", e);
			process::exit(1);
		},
	};

	let services_config: ServicesConfig = cli.services_config.clone().into();
	let pfs_info = match init_pfs_info(
		default_addresses.clone(),
		proxy_manager.clone(),
		services_config.clone(),
	)
	.await
	{
		Ok(info) => info,
		Err(e) => {
			tracing::error!("{}", e);
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
			maximum_fee: services_config.pathfinding_max_fee,
			iou_timeout: services_config.pathfinding_iou_timeout.into(),
			max_paths: services_config.pathfinding_max_paths,
		},
		addresses: default_addresses.clone(),
		default_settle_timeout: cli.default_settle_timeout.into(),
		default_reveal_timeout: cli.default_reveal_timeout.into(),
	};
	let raiden = Arc::new(Raiden {
		web3,
		config: config.clone(),
		contracts_manager,
		proxy_manager: proxy_manager.clone(),
		state_manager: state_manager.clone(),
		transport: transport_sender.clone(),
	});

	let payments_registry = Arc::new(RwLock::new(PaymentsRegistry::new()));
	let event_handler = EventHandler::new(
		raiden.web3.clone(),
		account.clone(),
		state_manager.clone(),
		proxy_manager.clone(),
		transport_sender.clone(),
		default_addresses.clone(),
		payments_registry.clone(),
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
			tracing::error!("Error connecting to websocket: {:?}", e);
			process::exit(1);
		},
	};

	let mut sync_service = SyncService::new(raiden.clone(), transitioner.clone());
	let latest_block_number = raiden.web3.eth().block_number().await.unwrap();

	info!("Performing initial sync from {} to {}", sync_start_block_number, latest_block_number);
	sync_service.sync(sync_start_block_number, latest_block_number.into()).await;
	init_channel_fees(
		state_manager,
		event_handler,
		default_addresses.token_network_registry,
		config.mediation_config.clone(),
	)
	.await;

	let block_monitor_service =
		match BlockMonitorService::new(raiden.clone(), ws, transitioner.clone(), sync_service) {
			Ok(service) => service,
			Err(_) => {
				tracing::error!("Could not initialize block monitor service");
				process::exit(1);
			},
		};
	let api = Api::new(raiden.clone(), transitioner.clone(), payments_registry);

	let socket: SocketAddr = match cli.api_address.parse() {
		Ok(socket) => socket,
		Err(e) => {
			tracing::error!("Error starting HTTP server: {:?}", e);
			process::exit(1);
		},
	};
	let (stop_sender, mut stop_receiver) = mpsc::channel(1);
	let http_service = crate::http::HttpServer::new(socket, raiden, Arc::new(api), stop_sender);

	info!("Raiden is starting");

	let mut hangup = match signal(SignalKind::interrupt()) {
		Ok(s) => s,
		Err(e) => {
			tracing::error!("Could not instantiate listener for hangup signal: {:?}", e);
			return
		},
	};
	select! {
		_ = block_monitor_service.start().fuse() => {},
		_ = transport_service.run(message_handler).fuse() => {},
		_ = http_service.start().fuse() => {},
		_ = stop_receiver.recv().fuse() => {
			println!("Raiden is stopping");
			return
		}
		_ = hangup.recv().fuse() => {
			println!("Raiden is stopping");
			return
		},
	};
}

fn setup_data_directory(path: PathBuf) -> Result<PathBuf, String> {
	let path = expanduser::expanduser(path.to_string_lossy())
		.map_err(|_| "Failed to expand data directory".to_owned())?;

	if !path.exists() {
		match fs::create_dir_all(path.clone()) {
			Err(e) =>
				return Err(format!("Could not create directory: {:?} because {}", path.clone(), e)),
			_ => {},
		}
	}
	Ok(path.to_path_buf())
}
fn get_logging_filter(log_config: String) -> EnvFilter {
	EnvFilter::from_env("RAIDEN_LOG")
		.add_directive(format!("raiden={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_api={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_blockchain={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_client={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_state_machine={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_storage={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_transition={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_network_messages={}", log_config).parse().unwrap())
		.add_directive(format!("raiden_network_transport={}", log_config).parse().unwrap())
		.add_directive(format!("hyper=info").parse().unwrap())
}
