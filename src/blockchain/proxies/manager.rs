use std::{
    collections::HashMap,
    sync::Arc,
};

use tokio::sync::RwLock;
use web3::{
    contract::Contract,
    transports::Http,
    types::{
        Address,
        U256,
    },
    Web3,
};

use crate::blockchain::{
    contracts::{
        ContractIdentifier,
        ContractsManager,
        GasMetadata,
    },
    errors::ContractDefError,
    key::PrivateKey,
};

use super::{
    common::Account,
    ProxyError,
    TokenNetworkProxy,
    TokenNetworkRegistryProxy,
    TokenProxy,
};

pub struct ProxyManager {
    web3: Web3<Http>,
    account: Account<Http>,
    gas_metadata: Arc<GasMetadata>,
    contracts_manager: Arc<ContractsManager>,
    tokens: RwLock<HashMap<Address, TokenProxy<Http>>>,
    token_networks: RwLock<HashMap<Address, TokenNetworkProxy<Http>>>,
    token_network_registries: RwLock<HashMap<Address, TokenNetworkRegistryProxy<Http>>>,
}

impl ProxyManager {
    pub fn new(
        web3: Web3<Http>,
        contracts_manager: Arc<ContractsManager>,
        private_key: PrivateKey,
        nonce: U256,
    ) -> Result<Self, ProxyError> {
        let gas_metadata = Arc::new(GasMetadata::new());
        let account = Account::new(web3.clone(), private_key, nonce);

        Ok(Self {
            web3,
            contracts_manager,
            account,
            gas_metadata,
            tokens: RwLock::new(HashMap::new()),
            token_networks: RwLock::new(HashMap::new()),
            token_network_registries: RwLock::new(HashMap::new()),
        })
    }

    pub async fn token_network_registry(
        &self,
        token_network_registry_address: Address,
        account_address: Address,
    ) -> Result<TokenNetworkRegistryProxy<Http>, ContractDefError> {
        if !self
            .token_network_registries
            .read()
            .await
            .contains_key(&token_network_registry_address)
        {
            let token_network_registry_contract = self.contracts_manager.get(ContractIdentifier::TokenNetworkRegistry);
            let token_network_registry_web3_contract = Contract::from_json(
                self.web3.eth(),
                token_network_registry_address,
                token_network_registry_contract.abi.as_slice(),
            )
            .map_err(ContractDefError::ABI)?;
            let proxy = TokenNetworkRegistryProxy::new(token_network_registry_web3_contract, account_address);
            let mut token_network_registries = self.token_network_registries.write().await;
            token_network_registries.insert(token_network_registry_address, proxy);
        }
        Ok(self
            .token_network_registries
            .read()
            .await
            .get(&token_network_registry_address)
            .unwrap()
            .clone())
    }

    pub async fn token(
        &self,
        token_address: Address,
        account_address: Address,
    ) -> Result<TokenProxy<Http>, ContractDefError> {
        if !self.tokens.read().await.contains_key(&token_address) {
            let token_contract = self.contracts_manager.get(ContractIdentifier::HumanStandardToken);
            let token_web3_contract =
                Contract::from_json(self.web3.eth(), token_address, token_contract.abi.as_slice())
                    .map_err(ContractDefError::ABI)?;
            let proxy = TokenProxy::new(token_web3_contract, account_address);
            let mut tokens = self.tokens.write().await;
            tokens.insert(token_address, proxy);
        }
        Ok(self.tokens.read().await.get(&token_address).unwrap().clone())
    }

    pub async fn token_network(
        &self,
        token_address: Address,
        token_network_address: Address,
        account_address: Address,
    ) -> Result<TokenNetworkProxy<Http>, ContractDefError> {
        if !self.token_networks.read().await.contains_key(&token_network_address) {
            let token_proxy = self.token(token_address, account_address).await?;
            let token_network_contract = self.contracts_manager.get(ContractIdentifier::TokenNetwork);
            let token_network_web3_contract = Contract::from_json(
                self.web3.eth(),
                token_network_address,
                token_network_contract.abi.as_slice(),
            )
            .map_err(ContractDefError::ABI)?;
            let proxy = TokenNetworkProxy::new(
                self.web3.clone(),
                self.account.clone(),
                self.gas_metadata.clone(),
                token_network_web3_contract,
                token_proxy,
            );
            let mut token_networks = self.token_networks.write().await;
            token_networks.insert(token_network_address, proxy);
        }
        Ok(self
            .token_networks
            .read()
            .await
            .get(&token_network_address)
            .unwrap()
            .clone())
    }
}
