use raiden_primitives::{
	deserializers::u256_from_u64,
	types::{
		Address,
		BlockNumber,
		BlockTimeout,
		ChainID,
		TokenAmount,
		TokenNetworkRegistryAddress,
	},
};
use serde::{
	Deserialize,
	Serialize,
};

use crate::types::RoutingMode;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfirmedBlockInfo {
	pub number: BlockNumber,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkInfo {
	pub chain_id: ChainID,
	pub token_network_registry_address: TokenNetworkRegistryAddress,
	pub user_deposit_address: Address,
	pub service_token_address: Address,
	pub confirmed_block: ConfirmedBlockInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PFSInfo {
	#[serde(deserialize_with = "u256_from_u64", rename(deserialize = "price_info"))]
	pub price: TokenAmount,
	#[serde(rename(deserialize = "network_info"))]
	pub network: NetworkInfo,
	pub payment_address: Address,
	pub message: String,
	pub operator: String,
	pub version: String,
	pub matrix_server: String,
}

#[derive(Clone, Debug)]
pub struct PFSConfig {
	pub url: String,
	pub info: PFSInfo,
	pub maximum_fee: TokenAmount,
	pub iou_timeout: BlockTimeout,
	pub max_paths: usize,
}

#[derive(Clone)]
pub struct ServicesConfig {
	pub routing_mode: RoutingMode,
	pub pathfinding_service_random_address: bool,
	pub pathfinding_service_address: String,
	pub pathfinding_max_paths: usize,
	pub pathfinding_max_fee: TokenAmount,
	pub pathfinding_iou_timeout: BlockTimeout,
	pub monitoring_enabled: bool,
}
