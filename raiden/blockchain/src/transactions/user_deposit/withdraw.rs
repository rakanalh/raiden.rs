use std::sync::Arc;

use raiden_primitives::types::{
	BlockHash,
	BlockId,
	GasLimit,
	GasPrice,
	TokenAmount,
	TransactionHash,
	U256,
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
		WithdrawPlan,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct WithdrawTransactionData {
	pub(crate) withdraw_plan: WithdrawPlan,
	pub(crate) whole_balance: TokenAmount,
}

#[derive(Clone)]
pub struct WithdrawTransactionParams {
	pub(crate) amount: TokenAmount,
}

pub struct WithdrawTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) user_deposit: UserDeposit<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for WithdrawTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = TransactionHash;
	type Params = WithdrawTransactionParams;
	type Data = WithdrawTransactionData;

	async fn onchain_data(
		&self,
		_params: Self::Params,
		at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		let withdraw_plan = self
			.user_deposit
			.withdraw_plan(self.account.address(), Some(at_block_hash))
			.await?;
		let whole_balance = self.user_deposit.whole_balance(Some(at_block_hash)).await?;

		Ok(WithdrawTransactionData { withdraw_plan, whole_balance })
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		data: Self::Data,
		_at_block_hash: BlockHash,
	) -> Result<(), ProxyError> {
		let block_number: U256 = self.web3.eth().block_number().await?.as_u64().into();

		if block_number < data.withdraw_plan.withdraw_block.into() {
			return Err(ProxyError::BrokenPrecondition(format!(
				"Cannot withdraw at block, \
                The current withdraw plan requires block number {}",
				data.withdraw_plan.withdraw_block
			)))
		}

		if data.whole_balance.checked_sub(params.amount).is_none() {
			return Err(ProxyError::BrokenPrecondition(format!("Whole balance underflow")))
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
				"withdraw",
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
				self.gas_metadata.get("UserDeposit.withdraw").into(),
				failed_at_blocknumber,
			)
			.await?;

		let data = self.onchain_data(params.clone(), failed_at_blockhash).await?;

		if failed_at_blocknumber < data.withdraw_plan.withdraw_block.into() {
			return Err(ProxyError::Recoverable(format!(
				"Cannot withdraw at block, \
                The current withdraw plan requires block number {}",
				data.withdraw_plan.withdraw_block
			)))
		}

		if data.whole_balance.checked_sub(params.amount).is_none() {
			return Err(ProxyError::Recoverable(format!("Whole balance underflow")))
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
				"withdraw",
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
			.map_err(ProxyError::ChainError)
	}
}
