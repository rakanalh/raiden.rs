use std::{
    collections::HashMap,
    sync::Arc,
};

use parking_lot::RwLock;
use web3::{
    contract::Contract,
    transports::Http,
    types::Address,
};

use crate::blockchain::contracts::ContractRegistry;

use super::{
    TokenNetworkProxy,
    TokenProxy,
};

struct ProxyManager {
    contracts_registry: Arc<RwLock<ContractRegistry>>,
    tokens: RwLock<HashMap<Address, TokenProxy<Http>>>,
    token_networks: RwLock<HashMap<Address, TokenNetworkProxy<Http>>>,
}

impl ProxyManager {
    pub fn new(contracts_registry: Arc<RwLock<ContractRegistry>>) -> Self {
        Self {
            contracts_registry,
            tokens: RwLock::new(HashMap::new()),
            token_networks: RwLock::new(HashMap::new()),
        }
    }

    fn token(&self, contract: Contract<Http>, address: Address) -> TokenProxy<Http> {
        let contract_address = contract.address();
        if !self.tokens.read().contains_key(&contract_address) {
            let proxy = TokenProxy::new(contract, address);
            let mut tokens = self.tokens.write();
            tokens.insert(contract_address, proxy);
        }
        self.tokens.read().get(&contract_address).unwrap().clone()
    }

    // fn token_network_registry(&self, contract: Contract<T>, address: Address) -> &TokenNetworkProxy<T> {
    // 	let mut token_networks = self.token_networks.write();
    //     match token_networks.get(&address) {
    //         Some(proxy) => proxy,
    //         None => {
    // 			let contract_address = contract.address();
    // 			let proxy = TokenNetworkProxy::new(contract, address);
    // 			token_networks.insert(contract_address, proxy);
    // 			token_networks.get(&address).unwrap()
    // 		}
    //     }
    // }
}
