use clap::ArgMatches;
use ethsign::SecretKey;
use raiden::state_machine::types::ChainID;
use slog::{Drain, Logger};
use std::path::{Path, PathBuf};
use web3::types::Address;
use crate::{raiden_service, traits::{ToHTTPEndpoint, ToSocketEndpoint}};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone)]
pub struct Config {
	pub chain_id: ChainID,
    pub keystore_path: PathBuf,
    pub eth_http_rpc_endpoint: String,
    pub eth_socket_rpc_endpoint: String,
}

impl Config {
	pub fn new(args: ArgMatches) -> Result<Self> {
		let chain_name = args.value_of("chain-id").unwrap();
		let chain_id = chain_name.parse().unwrap();

		let eth_rpc_http_endpoint = args.value_of("eth-rpc-endpoint").unwrap();
		let eth_rpc_socket_endpoint = args.value_of("eth-rpc-socket-endpoint").unwrap();
		let http_endpoint = eth_rpc_http_endpoint.to_http();
		if let Err(e) = http_endpoint {
			return Err(format!("Invalid RPC endpoint: {}", e));
		}

		let socket_endpoint = eth_rpc_socket_endpoint.to_socket();
		if let Err(e) = socket_endpoint {
			return Err(format!("Invalid RPC endpoint: {}", e));
		}

		let keystore_path = Path::new(args.value_of("keystore-path").unwrap());

		Ok(Self {
			chain_id,
			keystore_path: keystore_path.to_path_buf(),
			eth_http_rpc_endpoint: http_endpoint.unwrap(),
			eth_socket_rpc_endpoint: socket_endpoint.unwrap(),
		})
	}
}

pub struct RaidenApp {
	config: Config,
	node_address: Address,
	private_key: SecretKey,
	logger: Logger,
}

impl RaidenApp {
	pub fn new(config: Config, node_address: Address, private_key: SecretKey) -> Self {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::FullFormat::new(decorator).build().fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		let logger = slog::Logger::root(drain, o!());

		Self {
			config,
			node_address,
			private_key,
			logger,
		}
	}

	pub async fn run(&self) {
		let http = web3::transports::Http::new(&self.config.eth_http_rpc_endpoint).unwrap();
		let web3 = web3::Web3::new(http);
		let latest_block_number = web3.eth().block_number().await.unwrap();

		let service =
			raiden_service::RaidenService::new(web3, self.config.chain_id.clone(), self.node_address, self.private_key.clone(), self.logger.clone());

		service.initialize(latest_block_number).await;
		service.start(self.config.clone()).await;
	}
}
