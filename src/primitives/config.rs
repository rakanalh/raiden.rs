use std::path::PathBuf;
use web3::transports::Http;

use crate::blockchain::proxies::Account;

use super::{
    ChainID,
    MediationFeeConfig,
};

#[derive(Clone)]
pub struct RaidenConfig {
    pub chain_id: ChainID,
    pub account: Account<Http>,
    pub datadir: PathBuf,
    pub keystore_path: PathBuf,
    pub eth_http_rpc_endpoint: String,
    pub eth_socket_rpc_endpoint: String,
    pub mediation_config: MediationFeeConfig,
}
