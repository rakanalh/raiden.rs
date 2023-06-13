use std::sync::Arc;

use raiden_primitives::types::{
	BlockHash,
	BlockId,
	GasLimit,
	GasPrice,
	TokenAmount,
	TransactionHash,
};
use tokio::sync::RwLockWriteGuard;
use web3::{
	contract::Options,
	types::BlockNumber,
	Transport,
	Web3,
};

use crate::{
	contracts::GasMetadata,
	proxies::{
		Account,
		ProxyError,
		TokenProxy,
		UserDeposit,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct DepositTransactionData {
	pub(crate) previous_total_deposit: TokenAmount,
	pub(crate) current_balance: TokenAmount,
	pub(crate) whole_balance: TokenAmount,
	pub(crate) whole_balance_limit: TokenAmount,
}

#[derive(Clone)]
pub struct DepositTransactionParams {
	pub(crate) total_deposit: TokenAmount,
}

pub struct DepositTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) user_deposit: UserDeposit<T>,
	pub(crate) token: TokenProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for DepositTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = DepositTransactionParams;
	type Data = DepositTransactionData;

	async fn onchain_data(
		&self,
		_params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let previous_total_deposit = self
			.user_deposit
			.total_deposit(self.account.address(), Some(at_block_hash))
			.await?;

		let whole_balance = self.user_deposit.whole_balance(Some(at_block_hash)).await?;

		let whole_balance_limit =
			self.user_deposit.whole_balance_limit(Some(at_block_hash)).await?;

		let current_balance =
			self.token.balance_of(self.account.address(), Some(at_block_hash)).await?;

		Ok(DepositTransactionData {
			previous_total_deposit,
			current_balance,
			whole_balance,
			whole_balance_limit,
		})
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_at_block_hash: BlockHash,
	) -> Result<(), ProxyError> {
		let amount_to_deposit = params.total_deposit - data.previous_total_deposit;

		if params.total_deposit <= data.previous_total_deposit {
			return Err(ProxyError::BrokenPrecondition(format!("Total deposit did not increase")))
		}

		if data.whole_balance.checked_add(amount_to_deposit).is_none() {
			return Err(ProxyError::BrokenPrecondition(format!("Deposit overflow")))
		}

		if data.whole_balance.saturating_add(amount_to_deposit) > data.whole_balance_limit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Deposit of {:?} would have exceeded the UDC balance limit",
				amount_to_deposit
			)))
		}

		if data.current_balance < amount_to_deposit {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Not enough balance to deposit. Available: {:?}, Needed: {:?}",
				data.current_balance, amount_to_deposit
			)))
		}
		Ok(())
	}

	async fn execute_prerequisite(
		&self,
		params: Self::Params,
		data: Self::Data,
	) -> Result<(), ProxyError> {
		let amount_to_deposit = params.total_deposit - data.previous_total_deposit;
		self.token
			.approve(self.account.clone(), self.user_deposit.contract.address(), amount_to_deposit)
			.await?;

		Ok(())
	}

	async fn submit(
		&self,
		params: Self::Params,
		_data: Self::Data,
		gas_estimate: GasLimit,
		gas_price: GasPrice,
	) -> Result<Self::Output, ProxyError> {
		let nonce = self.account.peek_next_nonce().await;
		self.account.next_nonce().await;

		let receipt = self
			.user_deposit
			.contract
			.signed_call_with_confirmations(
				"deposit",
				(self.account.address(), params.total_deposit),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.gas = Some(gas_estimate);
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
				1,
				self.account.private_key(),
			)
			.await?;

		Ok(receipt.transaction_hash)
	}

	async fn validate_postconditions(
		&self,
		params: Self::Params,
		_at_block_hash: BlockHash,
	) -> Result<Self::Output, ProxyError> {
		let failed_at = self
			.web3
			.eth()
			.block(BlockId::Number(BlockNumber::Latest))
			.await
			.map_err(ProxyError::Web3)?
			.ok_or(ProxyError::Recoverable("Block not found".to_string()))?;

		let failed_at_blocknumber = failed_at.number.unwrap();
		let failed_at_blockhash = failed_at.hash.unwrap();

		self.account
			.check_for_insufficient_eth(
				self.gas_metadata.get("UserDeposit.deposit").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		let amount_to_deposit = params.total_deposit - data.previous_total_deposit;

		let has_sufficient_balance = self
			.token
			.balance_of(self.user_deposit.contract.address(), Some(failed_at_blockhash))
			.await? >= amount_to_deposit;

		if !has_sufficient_balance {
			return Err(ProxyError::Recoverable(format!(
				"The account does not have enough balance to complete the deposit"
			)))
		}

		let allowance = self
			.token
			.allowance(
				self.account.address(),
				self.user_deposit.contract.address(),
				Some(failed_at_blockhash),
			)
			.await?;

		if allowance < amount_to_deposit {
			return Err(ProxyError::Recoverable(format!(
				"The allowance of the {} deposit changed, previous is: {}. \
                Check concurrent deposits \
                for the same token network but different proxies.",
				amount_to_deposit, allowance
			)))
		}

		if params.total_deposit <= data.previous_total_deposit {
			return Err(ProxyError::Recoverable(format!(
				"Total deposit did not increase after deposit transaction"
			)))
		}

		if data.whole_balance.checked_add(amount_to_deposit).is_none() {
			return Err(ProxyError::Recoverable(format!("Deposit overflow")))
		}

		if data.whole_balance.saturating_add(amount_to_deposit) > data.whole_balance_limit {
			return Err(ProxyError::Recoverable(format!(
				"Deposit of {:?} would have exceeded the UDC balance limit",
				amount_to_deposit
			)))
		}

		return Err(ProxyError::Recoverable(format!("deposit failed for an unknown reason")))
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ProxyError> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(ProxyError::Web3)?;

		self.user_deposit
			.contract
			.estimate_gas(
				"deposit",
				(self.account.address(), params.total_deposit),
				self.account.address(),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
			)
			.await
			.map(|estimate| (estimate, gas_price))
			.map_err(ProxyError::ChainError)
	}

	async fn acquire_lock(&self) -> Option<RwLockWriteGuard<bool>> {
		Some(self.token.lock.write().await)
	}
}
