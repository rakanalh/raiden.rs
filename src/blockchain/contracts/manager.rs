use std::{
    collections::HashMap,
    convert::TryInto,
};

use crate::{
    blockchain::errors::ContractError,
    state_machine::types::ChainID,
};

use ethabi::Event;
use serde_json::{
    Map,
    Value,
};
use web3::types::{
    Address,
    U64,
};

use super::{
    consts::{
        CONTRACTS,
        DEPLOYMENT_GOERLI,
        DEPLOYMENT_KOVAN,
        DEPLOYMENT_MAINNET,
        DEPLOYMENT_RINKEBY,
        DEPLOYMENT_ROPSTEN,
    },
    ContractIdentifier,
};

pub type Result<T> = std::result::Result<T, ContractError>;

#[derive(Clone)]
pub struct Contract {
    pub abi: Vec<u8>,
}

impl Contract {
    pub fn new(abi: Vec<u8>) -> Self {
        Contract { abi }
    }
}

impl TryInto<ethabi::Contract> for Contract {
    type Error = ContractError;

    fn try_into(self) -> std::result::Result<ethabi::Contract, Self::Error> {
        ethabi::Contract::load(self.abi.as_slice()).map_err(ContractError::ABI)
    }
}

impl TryInto<ethabi::Contract> for &Contract {
    type Error = ContractError;

    fn try_into(self) -> std::result::Result<ethabi::Contract, Self::Error> {
        ethabi::Contract::load(self.abi.as_slice()).map_err(ContractError::ABI)
    }
}

#[derive(Clone)]
pub struct DeployedContract {
    inner: Contract,
    pub address: Address,
    pub block: U64,
}

pub struct ContractsManager {
    contracts: HashMap<String, Contract>,
    deployment: Map<String, Value>,
}

impl ContractsManager {
    pub fn new(chain_id: ChainID) -> Result<Self> {
        let chain_deployment = match chain_id {
            ChainID::Mainnet => DEPLOYMENT_MAINNET,
            ChainID::Ropsten => DEPLOYMENT_ROPSTEN,
            ChainID::Goerli => DEPLOYMENT_GOERLI,
            ChainID::Kovan => DEPLOYMENT_KOVAN,
            ChainID::Rinkeby => DEPLOYMENT_RINKEBY,
        };
        let contracts_specs: serde_json::Value = serde_json::from_str(CONTRACTS)?;
        let contracts_deployment: serde_json::Value = serde_json::from_str(chain_deployment)?;

        let specs_map = contracts_specs
            .get("contracts")
            .ok_or(ContractError::SpecNotFound)?
            .as_object()
            .ok_or(ContractError::SpecNotFound)?;

        let deployment = contracts_deployment
            .get("contracts")
            .ok_or(ContractError::SpecNotFound)?
            .as_object()
            .ok_or(ContractError::SpecNotFound)?;

        let mut manager = Self {
            contracts: HashMap::new(),
            deployment: deployment.clone(),
        };

        for (contract_name, contract_data) in specs_map.iter() {
            manager.contracts.insert(
                contract_name.clone(),
                Contract::new(serde_json::to_vec(contract_data.get("abi").unwrap()).unwrap()),
            );
        }

        Ok(manager)
    }

    pub fn get(&self, contract_identifier: ContractIdentifier) -> Contract {
        self.contracts
            .get(&contract_identifier.to_string())
            .map(|c| c.clone())
            .unwrap()
    }

    pub fn get_deployed(&self, contract_identifier: ContractIdentifier) -> Result<DeployedContract> {
        let contract = self
            .contracts
            .get(&contract_identifier.to_string())
            .map(|c| c.clone())
            .ok_or_else(|| ContractError::SpecNotFound)?;

        let address = match self
            .deployment
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

        let block_number = self
            .deployment
            .get("block_number")
            .map(|v| v.as_u64().unwrap())
            .unwrap();

        let block_number = U64::from(block_number);

        Ok(DeployedContract {
            inner: contract,
            address,
            block: block_number,
        })
    }

    pub fn events(&self, contract_identifier: Option<ContractIdentifier>) -> Vec<Event> {
        match contract_identifier {
            Some(id) => {
                let contract: ethabi::Contract = self.get(id).try_into().unwrap();
                contract.events().cloned().collect()
            }
            None => {
                let mut result = vec![];
				for contract in self.contracts.values() {
					let contract: ethabi::Contract = contract.try_into().unwrap();
					let events = contract.events();

					for event in events {
						result.push(event.clone());
					}
				}

				result
            }
        }
    }
}
