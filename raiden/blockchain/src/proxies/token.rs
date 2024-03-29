use std::sync::Arc;

use raiden_primitives::types::{
	Address,
	BlockId,
	H256,
	U256,
};
use tokio::sync::RwLock;
use web3::{
	contract::{
		Contract,
		Options,
	},
	Transport,
	Web3,
};

use super::{
	common::Account,
	ProxyError,
};

/// The token proxy error type.
type Result<T> = std::result::Result<T, ProxyError>;

/// Token proxy to interact with the on-chain contract.
#[derive(Clone)]
pub struct TokenProxy<T: Transport> {
	web3: Web3<T>,
	pub(crate) contract: Contract<T>,
	pub(crate) lock: Arc<RwLock<bool>>,
}

impl<T: Transport> TokenProxy<T> {
	/// Returns a new instance of `TokenProxy`.
	pub fn new(web3: Web3<T>, contract: Contract<T>) -> Self {
		Self { web3, contract, lock: Arc::new(RwLock::new(true)) }
	}

	/// Returns the allowance amount for spender by the current account.
	pub async fn allowance(
		&self,
		address: Address,
		spender: Address,
		block: Option<H256>,
	) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("allowance", (address, spender), address, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Returns the total supply of a token.
	pub async fn total_supply(&self, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("totalSupply", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Returns the current balance of an address.
	pub async fn balance_of(&self, address: Address, block: Option<H256>) -> Result<U256> {
		let block = block.map(BlockId::Hash);
		self.contract
			.query("balanceOf", (address,), address, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	/// Approves a specific amount for `allowed_address`.
	pub async fn approve(
		&self,
		account: Account<T>,
		allowed_address: Address,
		allowance: U256,
	) -> Result<H256> {
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;
		let gas_estimate = self
			.contract
			.estimate_gas(
				"approve",
				(allowed_address, allowance),
				account.address(),
				Options::default(),
			)
			.await
			.map_err(ProxyError::ChainError)?;

		let nonce = account.peek_next_nonce().await;
		account.next_nonce().await;

		let receipt = self
			.contract
			.signed_call_with_confirmations(
				"approve",
				(allowed_address, allowance),
				Options::with(|opt| {
					opt.gas = Some(gas_estimate);
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
				1,
				account.private_key(),
			)
			.await
			.map_err(ProxyError::Web3)?;

		Ok(receipt.transaction_hash)
	}

	/// Mint a specific amount and deposit into caller's account.
	pub async fn mint(&self, account: Account<T>, amount: U256) -> Result<H256> {
		let nonce = account.peek_next_nonce().await;
		account.next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;
		let gas_estimate = self
			.contract
			.estimate_gas("mint", (amount,), account.address(), Options::default())
			.await
			.map_err(ProxyError::ChainError)?;

		let lock = self.lock.write().await;
		let transaction_hash = self
			.contract
			.call(
				"mint",
				(amount,),
				account.address(),
				Options::with(|opt| {
					opt.gas = Some(gas_estimate);
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
			)
			.await
			.map_err(ProxyError::ChainError)?;

		drop(lock);

		Ok(transaction_hash)
	}

	/// Mint a specific amount and deposit into `to`'s account.
	pub async fn mint_for(&self, account: Account<T>, to: Address, amount: U256) -> Result<H256> {
		let nonce = account.peek_next_nonce().await;
		account.next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;
		let gas_estimate = self
			.contract
			.estimate_gas("mintFor", (amount, to), account.address(), Options::default())
			.await
			.map_err(ProxyError::ChainError)?;

		let lock = self.lock.write().await;
		let transaction_hash = self
			.contract
			.call(
				"mintFor",
				(amount, to),
				account.address(),
				Options::with(|opt| {
					opt.gas = Some(gas_estimate);
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
			)
			.await
			.map_err(ProxyError::ChainError)?;

		drop(lock);

		Ok(transaction_hash)
	}
}
