use std::sync::Arc;

use raiden_primitives::{
	hashing::hash_secret,
	types::{
		BlockHash,
		BlockId,
		GasLimit,
		GasPrice,
		Secret,
		SecretHash,
	},
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
		SecretRegistryProxy,
	},
	transactions::Transaction,
};

#[derive(Clone)]
pub struct RegisterSecretTransactionParams {
	pub(crate) secret: Secret,
}

pub struct RegisterSecretTransaction<T: Transport> {
	pub(crate) web3: Web3<T>,
	pub(crate) account: Account<T>,
	pub(crate) secret_registry: SecretRegistryProxy<T>,
	pub(crate) gas_metadata: Arc<GasMetadata>,
}

#[async_trait::async_trait]
impl<T> Transaction for RegisterSecretTransaction<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	type Output = ();
	type Params = RegisterSecretTransactionParams;
	type Data = ();

	async fn onchain_data(
		&self,
		_params: Self::Params,
		_at_block_hash: BlockHash,
	) -> Result<Self::Data, ProxyError> {
		Ok(())
	}

	async fn validate_preconditions(
		&self,
		params: Self::Params,
		_data: Self::Data,
		at_block_hash: BlockHash,
	) -> Result<(), ProxyError> {
		let secrethash = hash_secret(&params.secret.0);
		let secret_registered = self
			.secret_registry
			.is_secret_registered(SecretHash::from_slice(&secrethash), Some(at_block_hash))
			.await?;
		if secret_registered {
			return Err(ProxyError::BrokenPrecondition(format!("Secret is already registered",)))
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
		let nonce = self.account.next_nonce().await;

		self.secret_registry
			.contract
			.signed_call_with_confirmations(
				"registerSecret",
				(params.secret,),
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
		Ok(())
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

		self.account
			.check_for_insufficient_eth(
				self.gas_metadata.get("SecretRegistry.registerSecret").into(),
				failed_at_blocknumber,
			)
			.await?;

		let secrethash = hash_secret(&params.secret.0);
		let secret_registeration_block = self
			.secret_registry
			.get_secret_registration_block_by_secrethash(SecretHash::from_slice(&secrethash), None)
			.await?;

		if secret_registeration_block <= Some(failed_at_blocknumber.into()) {
			return Err(ProxyError::Recoverable(format!("Secret was already registered",)))
		}

		return Err(ProxyError::Recoverable(format!("withdraw failed for an unknown reason")))
	}

	async fn estimate_gas(
		&self,
		params: Self::Params,
		_data: Self::Data,
	) -> Result<(GasLimit, GasPrice), ()> {
		let nonce = self.account.peek_next_nonce().await;
		let gas_price = self.web3.eth().gas_price().await.map_err(|_| ())?;

		self.secret_registry
			.contract
			.estimate_gas(
				"setTotalWithdraw",
				(params.secret,),
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
