use std::sync::Arc;

use parking_lot::RwLock;
use web3::{
    contract::{
        Contract,
        Error,
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

    pub async fn allowance(&self, owner: Address, spender: Address, block: H256) -> Result<U256, Error> {
        self.contract
            .query(
                "allowance",
                (owner, spender),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
    }

    pub async fn balance_of(&self, address: Address, block: H256) -> Result<U256, Error> {
        self.contract
            .query(
                "balanceOf",
                (address,),
                self.from,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
    }

    pub async fn approve(&self, allowed_address: Address, allowance: U256, block: H256) -> Result<H256, Error> {
        let gas_estimate = self
            .contract
            .estimate_gas("approve", (allowed_address, allowance), self.from, Options::default())
            .await;

        let lock = self.lock.write();

        let transaction_hash = match gas_estimate {
            Ok(gas_estimate) => {
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
                        return Err(e);
                    }
                }
            }
            Err(e) => {
                return Err(e);
            }
        };

        drop(lock);

        Ok(transaction_hash)
    }
}
