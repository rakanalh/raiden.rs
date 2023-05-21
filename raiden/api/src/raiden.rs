use std::sync::Arc;

use parking_lot::RwLock;
use raiden_blockchain::{
	contracts::ContractsManager,
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_network_messages::messages::TransportServiceMessage;
use raiden_pathfinding::{
	config::PFSConfig,
	PFS,
};
use raiden_primitives::types::{
	AddressMetadata,
	ChainID,
	DefaultAddresses,
	RevealTimeout,
	SettleTimeout,
};
use raiden_state_machine::types::MediationFeeConfig;
use raiden_transition::manager::StateManager;
use tokio::sync::mpsc::UnboundedSender;
use web3::{
	transports::Http,
	Web3,
};

#[derive(Clone)]
pub struct RaidenConfig {
	pub chain_id: ChainID,
	pub account: Account<Http>,
	pub mediation_config: MediationFeeConfig,
	pub monitoring_enabled: bool,
	pub pfs_config: PFSConfig,
	pub metadata: AddressMetadata,
	/// Default addresses
	pub addresses: DefaultAddresses,
	pub default_settle_timeout: SettleTimeout,
	pub default_reveal_timeout: RevealTimeout,
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
	pub transport: UnboundedSender<TransportServiceMessage>,
	/// Pathfinding
	pub pfs: Arc<PFS>,
}
