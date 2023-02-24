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

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct UserDeposit<T: Transport> {
	web3: Web3<T>,
	contract: Contract<T>,
	lock: Arc<RwLock<bool>>,
}

impl<T: Transport> UserDeposit<T> {
	pub fn new(web3: Web3<T>, contract: Contract<T>) -> Self {
		Self { web3, contract, lock: Arc::new(RwLock::new(true)) }
	}

	pub async fn total_deposit(&self, owner: Address, block: Option<H256>) -> Result<U256> {
		let block = block.map(|b| BlockId::Hash(b));
		self.contract
			.query("total_deposit", (owner,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn deposit(
		&self,
		account: Account<T>,
		beneficiary: Address,
		new_total_deposit: U256,
	) -> Result<H256> {
		let nonce = account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;
		let gas_estimate = self
			.contract
			.estimate_gas(
				"deposit",
				(beneficiary, new_total_deposit),
				account.address(),
				Options::default(),
			)
			.await
			.map_err(ProxyError::ChainError)?;

		let lock = self.lock.write().await;
		let transaction_hash = self
			.contract
			.call(
				"deposit",
				(beneficiary, new_total_deposit),
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
