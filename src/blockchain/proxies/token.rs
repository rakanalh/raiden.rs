use std::sync::Arc;

use tokio::sync::RwLock;
use web3::{
    contract::{
        Contract,
        Options,
    },
    types::{
        Address,
        BlockId,
        H256,
        U256,
    },
    Transport,
    Web3,
};

use super::{ProxyError, common::Account};

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct TokenProxy<T: Transport> {
    web3: Web3<T>,
    contract: Contract<T>,
    lock: Arc<RwLock<bool>>,
}

impl<T: Transport> TokenProxy<T> {
    pub fn new(web3: Web3<T>, contract: Contract<T>) -> Self {
        Self {
            web3,
            contract,
            lock: Arc::new(RwLock::new(true)),
        }
    }

    pub async fn allowance(&self, owner: Address, spender: Address, block: H256) -> Result<U256> {
        self.contract
            .query(
                "allowance",
                (owner, spender),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn balance_of(&self, address: Address, block: Option<H256>) -> Result<U256> {
        let block = block.map(|b| BlockId::Hash(b));
        self.contract
            .query(
                "balanceOf",
                (address,),
                address,
                Options::default(),
                block,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn approve(&self, account: Account<T>, allowed_address: Address, allowance: U256) -> Result<H256> {
        let nonce = account.peek_next_nonce().await;
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

        let lock = self.lock.write().await;
        let transaction_hash = self
            .contract
            .call(
                "approve",
                (allowed_address, allowance),
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
