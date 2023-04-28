use std::sync::Arc;

use raiden_primitives::types::{
	BlockHash,
	BlockId,
	GasLimit,
	GasPrice,
	TokenAmount,
	TransactionHash,
};
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
		UserDeposit,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct PlanWithdrawTransactionData {
	pub(crate) current_balance: TokenAmount,
}

#[derive(Clone)]
pub struct PlanWithdrawTransactionParams {
	pub(crate) amount: TokenAmount,
}

pub struct PlanWithdrawTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) user_deposit: UserDeposit<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for PlanWithdrawTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = PlanWithdrawTransactionParams;
	type Data = PlanWithdrawTransactionData;

	async fn onchain_data(
		&self,
		_params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let current_balance =
			self.user_deposit.balance(self.account.address(), Some(at_block_hash)).await?;

		Ok(PlanWithdrawTransactionData { current_balance })
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_at_block_hash: BlockHash,
	) -> Result<(), ProxyError> {
		if params.amount == TokenAmount::zero() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Planned withdraw amount must be greater than zero"
			)))
		}

		if data.current_balance < params.amount {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Cannot withdraw more than the current balance"
			)))
		}

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
				"planWithdraw",
				(params.amount,),
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
				self.gas_metadata.get("UserDeposit.planWithdraw").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		if data.current_balance < params.amount {
			return Err(ProxyError::Recoverable(format!(
				"Could not withdraw more than the current balance"
			)))
		}

		return Err(ProxyError::Recoverable(format!("deposit failed for an unknown reason")))
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ()> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(|_| ())?;

		self.user_deposit
			.contract
			.estimate_gas(
				"planWithdraw",
				(params.amount,),
				self.account.address(),
				Options::with(|opt| {
					opt.value = Some(GasLimit::from(0));
					opt.nonce = Some(nonce);
					opt.gas_price = Some(gas_price);
				}),
			)
			.await
			.map(|estimate| (estimate, gas_price))
			.map_err(|_| ())
	}
}
