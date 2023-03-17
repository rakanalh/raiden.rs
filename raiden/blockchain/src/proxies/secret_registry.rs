use std::sync::Arc;

use raiden_primitives::types::{
	BlockHash,
	BlockId,
	Secret,
	SecretHash,
	U256,
	U64,
};
use web3::{
	contract::{
		Contract,
		Options,
	},
	Transport,
	Web3,
};

use super::common::{
	Account,
	Result,
};
use crate::{
	contracts::GasMetadata,
	transactions::{
		RegisterSecretTransaction,
		RegisterSecretTransactionParams,
		Transaction,
	},
};

#[derive(Clone)]
pub struct SecretRegistryProxy<T: Transport> {
	web3: Web3<T>,
	gas_metadata: Arc<GasMetadata>,
	pub(crate) contract: Contract<T>,
}

impl<T> SecretRegistryProxy<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	pub fn new(web3: Web3<T>, gas_metadata: Arc<GasMetadata>, contract: Contract<T>) -> Self {
		Self { contract, web3, gas_metadata }
	}

	pub async fn get_secret_registration_block_by_secrethash(
		&self,
		secrethash: SecretHash,
		block: Option<BlockHash>,
	) -> Result<Option<U64>> {
		let block = block.map(|b| BlockId::Hash(b));
		self.contract
			.query("getSecretRevealBlockHeight", (secrethash,), None, Options::default(), block)
			.await
			.map(|b: U256| {
				let b = b.as_u64();
				Some(b.into())
			})
			.map_err(Into::into)
	}

	pub async fn is_secret_registered(
		&self,
		secrethash: SecretHash,
		block: Option<BlockHash>,
	) -> Result<bool> {
		let block = self.get_secret_registration_block_by_secrethash(secrethash, block).await?;
		Ok(block.is_none())
	}

	pub async fn register_secret(
		&self,
		account: Account<T>,
		secret: Secret,
		block_hash: BlockHash,
	) -> Result<()> {
		let transaction = RegisterSecretTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			secret_registry: self.clone(),
			gas_metadata: self.gas_metadata.clone(),
		};
		transaction
			.execute(RegisterSecretTransactionParams { secret }, block_hash)
			.await
	}
}
