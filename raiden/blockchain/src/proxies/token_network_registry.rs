use ethabi::ethereum_types::U256;
use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockId,
	SettleTimeout,
	TokenAddress,
};
use web3::{
	contract::{
		Contract,
		Options,
	},
	Transport,
};

use super::ProxyError;

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct TokenNetworkRegistryProxy<T: Transport> {
	contract: Contract<T>,
}

impl<T: Transport> TokenNetworkRegistryProxy<T> {
	pub fn new(contract: Contract<T>) -> Self {
		Self { contract }
	}

	pub async fn get_token_network(
		&self,
		token_address: TokenAddress,
		block: BlockHash,
	) -> Result<Address> {
		self.contract
			.query(
				"token_to_token_networks",
				(token_address,),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map_err(Into::into)
	}

	pub async fn settlement_timeout_min(&self, block: BlockHash) -> Result<SettleTimeout> {
		self.contract
			.query(
				"settlement_timeout_min",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map(|b: U256| b.as_u64().into())
			.map_err(Into::into)
	}

	pub async fn settlement_timeout_max(&self, block: BlockHash) -> Result<SettleTimeout> {
		self.contract
			.query(
				"settlement_timeout_max",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map(|b: U256| b.as_u64().into())
			.map_err(Into::into)
	}
}
