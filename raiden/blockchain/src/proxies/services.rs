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

/// The proxy's result type.
type Result<T> = std::result::Result<T, ProxyError>;

/// The service registry proxy to interact with the on-chain contract.
#[derive(Clone)]
pub struct ServiceRegistryProxy<T: Transport> {
	contract: Contract<T>,
}

impl<T: Transport> ServiceRegistryProxy<T> {
	/// Returns a new instance of `ServiceRegistryProxy`.
	pub fn new(contract: Contract<T>) -> Self {
		Self { contract }
	}

	/// Get one of the addresses that have ever made a deposit.
	pub async fn ever_made_deposits(&self, index: u64, block: Option<H256>) -> Result<Address> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("ever_made_deposits", (index,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Get the number of addresses that have ever made a deposit.
	pub async fn ever_made_deposits_len(&self, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("everMadeDepositsLen", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Returns a boolean indicating whether an address has a valid service registration.
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

	/// Returns the block number marking the expiration of the service validity.
	pub async fn service_valid_til(&self, address: Address, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("service_valid_till", (address,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Gets the URL of a service by address. If does not exist return None
	pub async fn get_service_url(&self, address: Address, block: Option<H256>) -> Result<String> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("urls", (address,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Gets the currently required deposit amount.
	pub async fn current_price(&self, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("currentPrice", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Returns the service token address.
	pub async fn token(&self, block: Option<H256>) -> Result<Address> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("token", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}
}
