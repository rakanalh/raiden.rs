use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashMap, path::PathBuf};
use web3::{transports::Http, types::Address};

use crate::{
	blockchain::proxies::Account,
	primitives::{
		TokenAmount, DEFAULT_MEDIATION_FLAT_FEE, DEFAULT_MEDIATION_PROPORTIONAL_FEE,
		DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE,
	},
};

use super::{
	BlockNumber, BlockTimeout, ChainID, FeeAmount, ProportionalFeeAmount, RoutingMode,
	TokenNetworkRegistryAddress,
};

#[derive(Clone)]
pub struct TransportConfig {
	pub retry_timeout: u8,
	pub retry_timeout_max: u8,
	pub retry_count: u32,
	pub matrix: MatrixTransportConfig,
}

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
	pub transport_config: TransportConfig,
	pub pfs_config: PFSConfig,
}

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

#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct MediationFeeConfig {
	pub token_to_flat_fee: HashMap<Address, FeeAmount>,
	pub token_to_proportional_fee: HashMap<Address, ProportionalFeeAmount>,
	pub token_to_proportional_imbalance_fee: HashMap<Address, ProportionalFeeAmount>,
	pub cap_meditation_fees: bool,
}

impl MediationFeeConfig {
	pub fn get_flat_fee(&self, token_address: &Address) -> FeeAmount {
		*self
			.token_to_flat_fee
			.get(token_address)
			.unwrap_or(&DEFAULT_MEDIATION_FLAT_FEE.into())
	}

	pub fn get_proportional_fee(&self, token_address: &Address) -> ProportionalFeeAmount {
		*self
			.token_to_proportional_fee
			.get(token_address)
			.unwrap_or(&DEFAULT_MEDIATION_PROPORTIONAL_FEE.into())
	}

	pub fn get_proportional_imbalance_fee(self, token_address: &Address) -> ProportionalFeeAmount {
		*self
			.token_to_proportional_imbalance_fee
			.get(token_address)
			.unwrap_or(&DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE.into())
	}
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
