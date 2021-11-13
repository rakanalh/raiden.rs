use std::path::PathBuf;
use web3::{
    transports::Http,
    types::Address,
};

use crate::{
    blockchain::proxies::Account,
    primitives::TokenAmount,
};

use super::{
    BlockNumber,
    BlockTimeout,
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

#[derive(Clone)]
pub struct PFSInfo {
    pub url: String,
    pub price: TokenAmount,
    pub chain_id: ChainID,
    pub token_network_registry_address: Address,
    pub user_deposit_address: Address,
    pub payment_address: Address,
    pub message: String,
    pub operator: String,
    pub version: String,
    pub confirmed_block_number: BlockNumber,
    pub matrix_server: String,
}

#[derive(Clone)]
pub struct PFSConfig {
    pub info: PFSInfo,
    pub maximum_fee: TokenAmount,
    pub iou_timeout: BlockTimeout,
    pub max_paths: usize,
}
