use std::sync::Arc;

use parking_lot::RwLock;
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
};

use super::ProxyError;

type Result<T> = std::result::Result<T, ProxyError>;

#[derive(Clone)]
pub struct TokenProxy<T: Transport> {
    from: Address,
    contract: Contract<T>,
    lock: Arc<RwLock<bool>>,
}

impl<T: Transport> TokenProxy<T> {
    pub fn new(contract: Contract<T>, address: Address) -> Self {
        Self {
            from: address,
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

    pub async fn balance_of(&self, address: Address, block: H256) -> Result<U256> {
        self.contract
            .query(
                "balanceOf",
                (address,),
                self.from,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn approve(&self, allowed_address: Address, allowance: U256, block: H256) -> Result<H256> {
        let gas_estimate = self
            .contract
            .estimate_gas("approve", (allowed_address, allowance), self.from, Options::default())
            .await;

        let lock = self.lock.write();

        let transaction_hash = match gas_estimate {
            Ok(_gas_estimate) => {
                match self
                    .contract
                    .call("approve", (allowed_address, allowance), self.from, Options::default())
                    .await
                {
                    Ok(transaction_hash) => transaction_hash,
                    Err(e) => {
                        // check_transaction_failure
                        let balance = self.balance_of(self.from, block).await?;
                        if balance < allowance {}
                        return Err(ProxyError::ChainError(e));
                    }
                }
            }
            Err(e) => {
                return Err(ProxyError::ChainError(e));
            }
        };

        drop(lock);

        Ok(transaction_hash)
    }
}
