use std::sync::Arc;

use crate::blockchain::{
    contracts::ContractsManager,
    proxies::ProxyManager,
};
use crate::primitives::RaidenConfig;
use crate::state_manager::StateManager;
use crate::transport::matrix::MatrixClient;
use parking_lot::RwLock;
use slog::Logger;
use web3::transports::Http;
use web3::types::Address;
use web3::Web3;

pub struct DefaultAddresses {
    pub token_network_registry: Address,
    pub one_to_n: Address,
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
