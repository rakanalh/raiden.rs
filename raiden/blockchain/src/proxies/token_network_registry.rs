use std::sync::Arc;

use ethabi::ethereum_types::U256;
use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockId,
	SecretRegistryAddress,
	SettleTimeout,
	TokenAddress,
	TokenAmount,
	TokenNetworkAddress,
	TransactionHash,
};
use web3::{
	contract::{
		Contract,
		Options,
	},
	Transport,
	Web3,
};

use super::{
	Account,
	ProxyError,
	TokenProxy,
};
use crate::{
	contracts::GasMetadata,
	transactions::{
		RegisterTokenTransaction,
		RegisterTokenTransactionParams,
		Transaction,
	},
};

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct TokenNetworkRegistryProxy<T: Transport> {
	web3: Web3<T>,
	contract: Contract<T>,
	gas_metadata: Arc<GasMetadata>,
}

impl<T> TokenNetworkRegistryProxy<T>
where
	T: Transport + Send + Sync,
	T::Out: Send,
{
	pub fn new(web3: Web3<T>, gas_metadata: Arc<GasMetadata>, contract: Contract<T>) -> Self {
		Self { web3, gas_metadata, contract }
	}

	pub async fn add_token(
		&self,
		account: Account<T>,
		token_proxy: TokenProxy<T>,
		token_address: TokenAddress,
		block: BlockHash,
	) -> Result<(TransactionHash, TokenNetworkAddress)> {
		let add_token_transaction = RegisterTokenTransaction {
			web3: self.web3.clone(),
			account: account.clone(),
			token_network_registry: self.clone(),
			token: token_proxy,
			gas_metadata: self.gas_metadata.clone(),
		};

		add_token_transaction
			.execute(
				RegisterTokenTransactionParams {
					token_address,
					channel_participant_deposit_limit: TokenAmount::MAX,
					token_network_deposit_limit: TokenAmount::MAX,
				},
				block,
			)
			.await
	}

	pub async fn get_controller(&self, block: BlockHash) -> Result<Address> {
		self.contract
			.query("controller", (), None, Options::default(), Some(BlockId::Hash(block)))
			.await
			.map_err(Into::into)
	}

	pub async fn get_token_network(
		&self,
		token_address: TokenAddress,
		block: BlockHash,
	) -> Result<Address> {
		self.contract
			.query(
				"token_to_token_networks",
				(token_address,),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map_err(Into::into)
	}

	pub async fn settlement_timeout_min(&self, block: BlockHash) -> Result<SettleTimeout> {
		self.contract
			.query(
				"settlement_timeout_min",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map(|b: U256| b.as_u64().into())
			.map_err(Into::into)
	}

	pub async fn settlement_timeout_max(&self, block: BlockHash) -> Result<SettleTimeout> {
		self.contract
			.query(
				"settlement_timeout_max",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map(|b: U256| b.as_u64().into())
			.map_err(Into::into)
	}

	pub async fn get_secret_registry_address(
		&self,
		block: BlockHash,
	) -> Result<SecretRegistryAddress> {
		self.contract
			.query(
				"secret_registry_address",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map_err(Into::into)
	}

	pub async fn get_max_token_networks(&self, block: BlockHash) -> Result<U256> {
		self.contract
			.query("max_token_networks", (), None, Options::default(), Some(BlockId::Hash(block)))
			.await
			.map_err(Into::into)
	}

	pub async fn get_token_networks_created(&self, block: BlockHash) -> Result<U256> {
		self.contract
			.query(
				"token_network_created",
				(),
				None,
				Options::default(),
				Some(BlockId::Hash(block)),
			)
			.await
			.map_err(Into::into)
	}
}
