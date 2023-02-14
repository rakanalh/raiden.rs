use raiden_state_machine::types::{
	BlockNumber,
	BlockTimeout,
	ChainID,
	TokenAmount,
	TokenNetworkRegistryAddress,
};
use serde::{
	Deserialize,
	Deserializer,
	Serialize,
};
use web3::types::Address;

use crate::types::RoutingMode;

#[derive(Clone, Serialize, Deserialize)]
pub struct ConfirmedBlockInfo {
	#[serde(deserialize_with = "deserialize_block_number")]
	pub number: BlockNumber,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
	#[serde(deserialize_with = "deserialize_chain_id")]
	pub chain_id: ChainID,
	pub token_network_registry_address: TokenNetworkRegistryAddress,
	pub user_deposit_address: Address,
	pub service_token_address: Address,
	pub confirmed_block: ConfirmedBlockInfo,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PFSInfo {
	#[serde(deserialize_with = "deserialize_token_amount", rename(deserialize = "price_info"))]
	pub price: TokenAmount,
	#[serde(rename(deserialize = "network_info"))]
	pub network: NetworkInfo,
	pub payment_address: Address,
	pub message: String,
	pub operator: String,
	pub version: String,
	pub matrix_server: String,
}

#[derive(Clone)]
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
	pub pathfinding_service_specific_address: String,
	pub pathfinding_max_paths: usize,
	pub pathfinding_max_fee: TokenAmount,
	pub pathfinding_iou_timeout: BlockTimeout,
	pub monitoring_enabled: bool,
}

fn deserialize_chain_id<'de, D>(deserializer: D) -> Result<ChainID, D::Error>
where
	D: Deserializer<'de>,
{
	let buf = u64::deserialize(deserializer)?;
	let chain_id = match buf {
		1 => ChainID::Mainnet,
		2 => ChainID::Goerli,
		3 => ChainID::Rinkeby,
		4 => ChainID::Ropsten,
		_ => ChainID::Private,
	};
	Ok(chain_id)
}

fn deserialize_block_number<'de, D>(deserializer: D) -> Result<BlockNumber, D::Error>
where
	D: Deserializer<'de>,
{
	let buf = u64::deserialize(deserializer)?;
	Ok(BlockNumber::from(buf))
}

fn deserialize_token_amount<'de, D>(deserializer: D) -> Result<TokenAmount, D::Error>
where
	D: Deserializer<'de>,
{
	let buf = u64::deserialize(deserializer)?;
	Ok(TokenAmount::from(buf))
}
