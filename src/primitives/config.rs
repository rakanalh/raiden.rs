use std::path::PathBuf;
use web3::transports::Http;

use crate::{
    blockchain::proxies::Account,
    primitives::TokenAmount,
};

use super::{
    ChainID,
    MediationFeeConfig,
};

#[derive(Clone)]
pub struct MatrixTransportConfig {
    pub homeserver_url: String,
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
    pub transport_config: MatrixTransportConfig,
}
