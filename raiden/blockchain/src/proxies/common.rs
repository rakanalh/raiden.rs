use std::sync::Arc;

use raiden_primitives::types::{
	Address,
	U256,
};
use tokio::sync::Mutex;
use web3::{
	signing::Key,
	types::{
		BlockNumber,
		U64,
	},
	Transport,
	Web3,
};

use super::ProxyError;
use crate::keys::PrivateKey;

/// Proxies error type.
pub type Result<T> = std::result::Result<T, ProxyError>;

/// Stores account's nonce in sync with the one on-chain.
#[derive(Clone)]
pub struct Nonce {
	inner: Arc<Mutex<U256>>,
}

impl Nonce {
	/// Creates a new instance of `Nonce`.
	pub fn new(current: U256) -> Self {
		Self { inner: Arc::new(Mutex::new(current)) }
	}

	/// Retrieves the next nonce
	pub async fn next(&self) -> U256 {
		let mut inner = self.inner.lock().await;
		*inner += U256::from(1);
		*inner
	}

	/// Immutably get the next nonce.
	pub async fn peek_next(&self) -> U256 {
		let inner = self.inner.lock().await;
		*inner
	}
}

/// The account type holding nonce and private key.
#[derive(Clone)]
pub struct Account<T: Transport> {
	web3: Web3<T>,
	private_key: PrivateKey,
	nonce: Nonce,
}

impl<T: Transport> Account<T> {
	/// Returns a new instance of `Account`.
	pub fn new(web3: Web3<T>, private_key: PrivateKey, nonce: U256) -> Self {
		Self { web3, private_key, nonce: Nonce::new(nonce) }
	}

	/// Returns a copy of the private key.
	pub fn private_key(&self) -> PrivateKey {
		self.private_key.clone()
	}

	/// Returns the ethereum address of a key.
	pub fn address(&self) -> Address {
		self.private_key.address()
	}

	/// Retrieves the next usable nonce.
	pub async fn next_nonce(&self) -> U256 {
		self.nonce.next().await
	}

	/// Immutably retrieve the next usable nonce.
	pub async fn peek_next_nonce(&self) -> U256 {
		self.nonce.peek_next().await
	}

	/// Check account's balance and check if eth balance is insufficient.
	pub async fn check_for_insufficient_eth(&self, required_gas: U256, block: U64) -> Result<()> {
		let actual_balance = self
			.web3
			.eth()
			.balance(self.address(), Some(BlockNumber::Number(block)))
			.await?;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;
		let required_balance = required_gas * gas_price;
		if actual_balance < required_balance {
			return Err(ProxyError::InsufficientEth(format!(
				"Balance is not enough to execute transaction. Current: {}, required: {}",
				actual_balance, required_balance,
			)))
		}
		Ok(())
	}
}
