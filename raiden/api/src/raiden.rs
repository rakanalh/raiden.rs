use std::{
	path::PathBuf,
	sync::Arc,
};

use parking_lot::RwLock;
use raiden_blockchain::{
	contracts::ContractsManager,
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_pathfinding::config::PFSConfig;
use raiden_state_machine::types::{
	ChainID,
	MediationFeeConfig,
};
use raiden_storage::state_manager::StateManager;
use raiden_transport::{
	config::TransportConfig,
	matrix::MatrixClient,
};
use slog::Logger;
use web3::{
	transports::Http,
	types::Address,
	Web3,
};

pub struct DefaultAddresses {
	pub token_network_registry: Address,
	pub one_to_n: Address,
}

#[derive(Clone)]
pub struct RaidenConfig {
	pub chain_id: ChainID,
	pub account: Account<Http>,
	pub datadir: PathBuf,
	pub keystore_path: PathBuf,
	pub eth_http_rpc_endpoint: String,
	pub eth_socket_rpc_endpoint: String,
	pub mediation_config: MediationFeeConfig,
	pub transport_config: TransportConfig,
	pub pfs_config: PFSConfig,
}

pub struct Raiden {
	pub web3: Web3<Http>,
	/// Raiden Configurations
	pub config: RaidenConfig,
	/// Manager for contracts and deployments
	pub contracts_manager: Arc<ContractsManager>,
	/// Contract proxies manager
	pub proxy_manager: Arc<ProxyManager>,
	/// Manager of the current chain state
	pub state_manager: Arc<RwLock<StateManager>>,
	/// Transport layer
	pub transport: Arc<MatrixClient>,
	/// State transition layer
	// pub transition_service: Arc<dyn Transitioner + Send + Sync>,
	/// Default addresses
	pub addresses: DefaultAddresses,
	/// Logging instance
	pub logger: Logger,
}
