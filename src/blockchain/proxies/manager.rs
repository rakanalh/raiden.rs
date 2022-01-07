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

use crate::{
    blockchain::{
        contracts::{
            ContractIdentifier,
            ContractsManager,
            GasMetadata,
        },
        errors::ContractDefError,
    },
    state_machine::types::ChannelState,
};

use super::{
    channel::ChannelProxy,
    ProxyError,
    SecretRegistryProxy,
    ServiceRegistryProxy,
    TokenNetworkProxy,
    TokenNetworkRegistryProxy,
    TokenProxy,
};

pub struct ProxyManager {
    web3: Web3<Http>,
    gas_metadata: Arc<GasMetadata>,
    contracts_manager: Arc<ContractsManager>,
    pub tokens: RwLock<HashMap<Address, TokenProxy<Http>>>,
    pub token_networks: RwLock<HashMap<Address, TokenNetworkProxy<Http>>>,
    pub token_network_registries: RwLock<HashMap<Address, TokenNetworkRegistryProxy<Http>>>,
    pub secret_registries: RwLock<HashMap<Address, SecretRegistryProxy<Http>>>,
    pub service_registries: RwLock<HashMap<Address, ServiceRegistryProxy<Http>>>,
    channels: RwLock<HashMap<U256, ChannelProxy<Http>>>,
}

impl ProxyManager {
    pub fn new(web3: Web3<Http>, contracts_manager: Arc<ContractsManager>) -> Result<Self, ProxyError> {
        let gas_metadata = Arc::new(GasMetadata::new());

        Ok(Self {
            web3,
            contracts_manager,
            gas_metadata,
            tokens: RwLock::new(HashMap::new()),
            token_networks: RwLock::new(HashMap::new()),
            token_network_registries: RwLock::new(HashMap::new()),
            secret_registries: RwLock::new(HashMap::new()),
            service_registries: RwLock::new(HashMap::new()),
            channels: RwLock::new(HashMap::new()),
        })
    }

    pub fn web3(&self) -> Web3<Http> {
        self.web3.clone()
    }

    pub fn gas_metadata(&self) -> Arc<GasMetadata> {
        self.gas_metadata.clone()
    }

    pub async fn token_network_registry(
        &self,
        token_network_registry_address: Address,
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
            let proxy = TokenNetworkRegistryProxy::new(token_network_registry_web3_contract);
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

    pub async fn secret_registry(
        &self,
        secret_registry_address: Address,
    ) -> Result<SecretRegistryProxy<Http>, ContractDefError> {
        if !self
            .secret_registries
            .read()
            .await
            .contains_key(&secret_registry_address)
        {
            let secret_registry_contract = self.contracts_manager.get(ContractIdentifier::SecretRegistry);
            let secret_registry_web3_contract = Contract::from_json(
                self.web3.eth(),
                secret_registry_address,
                secret_registry_contract.abi.as_slice(),
            )
            .map_err(ContractDefError::ABI)?;
            let proxy = SecretRegistryProxy::new(secret_registry_web3_contract);
            let mut secret_registries = self.secret_registries.write().await;
            secret_registries.insert(secret_registry_address, proxy);
        }
        Ok(self
            .secret_registries
            .read()
            .await
            .get(&secret_registry_address)
            .unwrap()
            .clone())
    }

    pub async fn service_registry(
        &self,
        service_registry_address: Address,
    ) -> Result<ServiceRegistryProxy<Http>, ContractDefError> {
        if !self
            .service_registries
            .read()
            .await
            .contains_key(&service_registry_address)
        {
            let service_registry_contract = self.contracts_manager.get(ContractIdentifier::ServiceRegistry);
            let service_registry_web3_contract = Contract::from_json(
                self.web3.eth(),
                service_registry_address,
                service_registry_contract.abi.as_slice(),
            )
            .map_err(ContractDefError::ABI)?;
            let proxy = ServiceRegistryProxy::new(service_registry_web3_contract);
            let mut service_registries = self.service_registries.write().await;
            service_registries.insert(service_registry_address, proxy);
        }
        Ok(self
            .service_registries
            .read()
            .await
            .get(&service_registry_address)
            .unwrap()
            .clone())
    }

    pub async fn token(&self, token_address: Address) -> Result<TokenProxy<Http>, ContractDefError> {
        if !self.tokens.read().await.contains_key(&token_address) {
            let token_contract = self.contracts_manager.get(ContractIdentifier::HumanStandardToken);
            let token_web3_contract =
                Contract::from_json(self.web3.eth(), token_address, token_contract.abi.as_slice())
                    .map_err(ContractDefError::ABI)?;
            let proxy = TokenProxy::new(self.web3.clone(), token_web3_contract);
            let mut tokens = self.tokens.write().await;
            tokens.insert(token_address, proxy);
        }
        Ok(self.tokens.read().await.get(&token_address).unwrap().clone())
    }

    pub async fn token_network(
        &self,
        token_address: Address,
        token_network_address: Address,
    ) -> Result<TokenNetworkProxy<Http>, ContractDefError> {
        if !self.token_networks.read().await.contains_key(&token_network_address) {
            let token_proxy = self.token(token_address).await?;
            let token_network_contract = self.contracts_manager.get(ContractIdentifier::TokenNetwork);
            let token_network_web3_contract = Contract::from_json(
                self.web3.eth(),
                token_network_address,
                token_network_contract.abi.as_slice(),
            )
            .map_err(ContractDefError::ABI)?;
            let proxy = TokenNetworkProxy::new(
                self.web3.clone(),
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

    pub async fn payment_channel(&self, channel_state: &ChannelState) -> Result<ChannelProxy<Http>, ContractDefError> {
        let token_network_address = channel_state.canonical_identifier.token_network_address;
        let token_address = channel_state.token_address;
        let channel_identifier = channel_state.canonical_identifier.channel_identifier;

        if !self.channels.read().await.contains_key(&channel_identifier) {
            let token_network_proxy = self.token_network(token_address, token_network_address).await?;
            let proxy = ChannelProxy::new(token_network_proxy);
            let mut channels = self.channels.write().await;
            channels.insert(channel_identifier, proxy);
        }
        Ok(self.channels.read().await.get(&channel_identifier).unwrap().clone())
    }
}
