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
    },
    Transport,
};

pub struct TokenNetworkProxy<T: Transport> {
    from: Address,
    contract: Contract<T>,
    lock: RwLock<bool>,
}

impl<T: Transport> TokenNetworkProxy<T> {
    pub fn new(contract: Contract<T>, address: Address) -> Self {
        Self {
            from: address,
            contract,
            lock: RwLock::new(true),
        }
    }

    pub async fn address_by_token_address(&self, token_address: Address, block: H256) -> Result<Address, Error> {
        self.contract
            .query(
                "token_to_token_networks",
                (token_address,),
                None,
                Options::default(),
                Some(BlockId::Hash(block)),
            )
            .await
    }
}
