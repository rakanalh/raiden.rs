use std::sync::Arc;

use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockId,
	BlockNumber,
	TokenAmount,
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
	TokenProxy,
};
use crate::{
	contracts::GasMetadata,
	transactions::{
		DepositTransaction,
		DepositTransactionParams,
		PlanWithdrawTransaction,
		PlanWithdrawTransactionParams,
		Transaction,
		WithdrawTransaction,
		WithdrawTransactionParams,
	},
};

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct WithdrawPlan {
	pub withdraw_amount: TokenAmount,
	pub withdraw_block: BlockNumber,
}

#[derive(Clone)]
pub struct UserDeposit<T: Transport> {
	web3: Web3<T>,
	gas_metadata: Arc<GasMetadata>,
	pub(crate) contract: Contract<T>,
	lock: Arc<RwLock<bool>>,
}

impl<T> UserDeposit<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	pub fn new(web3: Web3<T>, gas_metadata: Arc<GasMetadata>, contract: Contract<T>) -> Self {
		Self { web3, gas_metadata, contract, lock: Arc::new(RwLock::new(true)) }
	}

	pub async fn token_address(&self, block: Option<H256>) -> Result<Address> {
		let block = block.map(|b| BlockId::Hash(b));
		self.contract
			.query("token", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn balance(&self, owner: Address, block: Option<H256>) -> Result<U256> {
		let block = block.map(|b| BlockId::Hash(b));
		self.contract
			.query("balances", (owner,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn total_deposit(&self, owner: Address, block: Option<H256>) -> Result<U256> {
		let block = block.map(|b| BlockId::Hash(b));
		self.contract
			.query("total_deposit", (owner,), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn whole_balance(&self, block: Option<H256>) -> Result<U256> {
		let block = block.map(|b| BlockId::Hash(b));
		self.contract
			.query("whole_balance", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn whole_balance_limit(&self, block: Option<H256>) -> Result<U256> {
		let block = block.map(|b| BlockId::Hash(b));
		self.contract
			.query("whole_balance_limit", (), None, Options::default(), block)
			.await
			.map_err(Into::into)
	}

	pub async fn withdraw_plan(
		&self,
		address: Address,
		block: Option<H256>,
	) -> Result<WithdrawPlan> {
		let block = block.map(|b| BlockId::Hash(b));
		let (withdraw_amount, withdraw_block): (TokenAmount, U256) = self
			.contract
			.query("withdraw_plans", (address,), None, Options::default(), block)
			.await?;

		Ok(WithdrawPlan { withdraw_amount, withdraw_block: withdraw_block.as_u64().into() })
	}

	pub async fn deposit(
		&self,
		account: Account<T>,
		token_proxy: TokenProxy<T>,
		new_total_deposit: U256,
		block_hash: BlockHash,
	) -> Result<H256> {
		let lock = self.lock.write().await;
		let deposit_transaction = DepositTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			user_deposit: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
			token: token_proxy.clone(),
		};

		let params = DepositTransactionParams { total_deposit: new_total_deposit };
		let result = deposit_transaction.execute(params, block_hash).await;
		drop(lock);
		result
	}

	pub async fn plan_withdraw(
		&self,
		account: Account<T>,
		amount: U256,
		block_hash: BlockHash,
	) -> Result<H256> {
		let lock = self.lock.write().await;
		let plan_withdraw_transaction = PlanWithdrawTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			user_deposit: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		let params = PlanWithdrawTransactionParams { amount };
		let result = plan_withdraw_transaction.execute(params, block_hash).await;
		drop(lock);
		result
	}

	pub async fn withdraw(
		&self,
		account: Account<T>,
		amount: U256,
		block_hash: BlockHash,
	) -> Result<H256> {
		let lock = self.lock.write().await;
		let withdraw_transaction = WithdrawTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			user_deposit: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};

		let params = WithdrawTransactionParams { amount };
		let result = withdraw_transaction.execute(params, block_hash).await;
		drop(lock);
		result
	}
}
