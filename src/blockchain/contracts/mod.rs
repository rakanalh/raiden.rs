use std::collections::HashMap;

use crate::state_machine::types::ChainID;

use ethabi::Events;
use serde_json::{
    Map,
    Value,
};
use web3::types::{
    Address,
    Filter,
    FilterBuilder,
    H256,
    U64,
};

pub const CONTRACTS: &str = include_str!("data/contracts.json");
const DEPLOYMENT_MAINNET: &str = include_str!("data/deployment_mainnet.json");
const DEPLOYMENT_ROPSTEN: &str = include_str!("data/deployment_ropsten.json");
const DEPLOYMENT_RINKEBY: &str = include_str!("data/deployment_rinkeby.json");
const DEPLOYMENT_KOVAN: &str = include_str!("data/deployment_kovan.json");
const DEPLOYMENT_GOERLI: &str = include_str!("data/deployment_goerli.json");

#[derive(Clone, Eq, PartialEq, Hash)]
pub enum ContractIdentifier {
    SecretRegistry = 1,
    TokenNetworkRegistry = 2,
    TokenNetwork = 3,
}

#[derive(Clone)]
pub struct Contract {
    pub name: &'static str,
    pub address: Address,
    pub deploy_block_number: U64,
    inner: ethabi::Contract,
}

impl Contract {
    fn new(name: &'static str, address: Address, deploy_block_number: U64, abi: String) -> Result<Self, ethabi::Error> {
        Ok(Self {
            name,
            address,
            deploy_block_number,
            inner: ethabi::Contract::load(abi.as_bytes())?,
        })
    }

    pub fn topics(&self) -> Vec<H256> {
        let events = self.inner.events();
        events.map(|e| e.signature()).collect()
    }

    pub fn filters(&self, from_block: U64, to_block: U64) -> Filter {
        let events = self.inner.events();
        let topics = events.map(|e| e.signature()).collect();
        FilterBuilder::default()
            .address(vec![self.address])
            .topics(Some(topics), None, None, None)
            .from_block(from_block.into())
            .to_block(to_block.into())
            .build()
    }

    pub fn events(&self) -> Events {
        self.inner.events()
    }
}

pub struct ContractRegistry {
    pub contracts: HashMap<ContractIdentifier, Vec<Contract>>,
}

impl ContractRegistry {
    pub fn new(chain_id: ChainID) -> Result<Self, ethabi::Error> {
        let chain_deployment = match chain_id {
            ChainID::Mainnet => DEPLOYMENT_MAINNET,
            ChainID::Ropsten => DEPLOYMENT_ROPSTEN,
            ChainID::Goerli => DEPLOYMENT_GOERLI,
            ChainID::Kovan => DEPLOYMENT_KOVAN,
            ChainID::Rinkeby => DEPLOYMENT_RINKEBY,
        };
        let contracts_data: serde_json::Value = serde_json::from_str(chain_deployment).unwrap();

        let contracts_map = contracts_data.get("contracts").unwrap().as_object().unwrap();

        let token_network_registry = Self::get_contract("TokenNetworkRegistry", contracts_map.clone())?;

        let mut contracts = HashMap::new();
        contracts.insert(ContractIdentifier::TokenNetworkRegistry, vec![token_network_registry]);

        Ok(Self { contracts })
    }

    pub fn filters(&self, from_block: U64, to_block: U64) -> Filter {
        let token_network_registry = self.token_network_registry();
        let mut topics = token_network_registry.topics();
        let mut addresses = vec![token_network_registry.address];

        for token_network in self.contracts.get(&ContractIdentifier::TokenNetwork).unwrap_or(&vec![]) {
            topics.extend(token_network.topics());
            addresses.push(token_network.address);
        }
        FilterBuilder::default()
            .address(addresses)
            .topics(Some(topics), None, None, None)
            .from_block(from_block.into())
            .to_block(to_block.into())
            .build()
    }

    pub fn token_network_registry(&self) -> Contract {
        self.contracts.get(&ContractIdentifier::TokenNetworkRegistry).unwrap()[0].clone()
    }

    fn get_contract(name: &'static str, data: Map<String, Value>) -> Result<Contract, ethabi::Error> {
        let contracts_spec_data: serde_json::Value = serde_json::from_str(CONTRACTS).unwrap();
        let contract_spec_map = contracts_spec_data
            .get("contracts")
            .unwrap()
            .as_object()
            .unwrap()
            .get(name)
            .unwrap();

        let contract_deployment_map = data.get(name).unwrap().as_object().unwrap();
        let address = match contract_deployment_map
            .get("address")
            .unwrap()
            .as_str()
            .unwrap()
            .trim_start_matches("0x")
            .parse()
        {
            Ok(value) => value,
            Err(_) => Address::zero(),
        };

        let block_number = contract_deployment_map
            .get("block_number")
            .map(|v| v.as_u64().unwrap())
            .unwrap();

        let block_number = U64::from(block_number);
        Contract::new(
            name,
            address,
            block_number,
            serde_json::to_string(contract_spec_map.get("abi").unwrap()).unwrap(),
        )
    }

    pub fn add_token_network(&mut self, address: Address, block_number: U64) -> Result<Contract, ethabi::Error> {
        let contracts_data: serde_json::Value = serde_json::from_str(CONTRACTS).unwrap();
        let contract_spec_map = contracts_data
            .get("contracts")
            .unwrap()
            .as_object()
            .unwrap()
            .get("TokenNetwork")
            .unwrap();

        let contract = Contract::new(
            "TokenNetwork",
            address,
            block_number,
            serde_json::to_string(contract_spec_map.get("abi").unwrap()).unwrap(),
        )?;
        match self.contracts.get_mut(&ContractIdentifier::TokenNetwork) {
            Some(contracts) => contracts.push(contract.clone()),
            None => {
                self.contracts
                    .insert(ContractIdentifier::TokenNetwork, vec![contract.clone()]);
            }
        };
        Ok(contract)
    }
}
