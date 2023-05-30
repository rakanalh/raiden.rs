use raiden_primitives::types::{
	Address,
	BlockId,
	H256,
	U256,
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
pub struct ServiceRegistryProxy<T: Transport> {
	contract: Contract<T>,
}

impl<T: Transport> ServiceRegistryProxy<T> {
	pub fn new(contract: Contract<T>) -> Self {
		Self { contract }
	}

	pub async fn ever_made_deposits(&self, index: u64, block: Option<H256>) -> Result<Address> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("ever_made_deposits", (index,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn ever_made_deposits_len(&self, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("everMadeDepositsLen", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn has_valid_registration(
		&self,
		address: Address,
		block: Option<H256>,
	) -> Result<bool> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("hasValidRegistration", (address,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn service_valid_til(&self, address: Address, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("service_valid_till", (address,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn get_service_url(&self, address: Address, block: Option<H256>) -> Result<String> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("urls", (address,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn current_price(&self, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("currentPrice", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn token(&self, block: Option<H256>) -> Result<Address> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("token", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}
}
