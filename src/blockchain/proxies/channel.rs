use std::sync::Arc;

use parking_lot::RwLock;
use web3::{
    types::Address,
    Transport,
};

use super::TokenNetworkProxy;

#[derive(Clone)]
pub struct ChannelProxy<T: Transport> {
    pub token_network: TokenNetworkProxy<T>,
    from: Address,
    lock: Arc<RwLock<bool>>,
}

impl<T: Transport> ChannelProxy<T> {
    pub fn new(token_network: TokenNetworkProxy<T>, address: Address) -> Self {
        Self {
            from: address,
            token_network,
            lock: Arc::new(RwLock::new(true)),
        }
    }
}
