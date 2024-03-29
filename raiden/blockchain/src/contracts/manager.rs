use std::{
	collections::HashMap,
	convert::TryInto,
};

use ethabi::Event;
use raiden_primitives::types::{
	Address,
	ChainID,
	DefaultAddresses,
	U64,
};
use serde_json::{
	Map,
	Value,
};

use super::{
	consts::{
		CONTRACTS,
		DEPLOYMENT_GOERLI,
		DEPLOYMENT_MAINNET,
		DEPLOYMENT_RINKEBY,
		DEPLOYMENT_ROPSTEN,
		DEPLOYMENT_SERVICES_GOERLI,
		DEPLOYMENT_SERVICES_MAINNET,
		DEPLOYMENT_SERVICES_RINKEBY,
		DEPLOYMENT_SERVICES_ROPSTEN,
	},
	ContractIdentifier,
};
use crate::{
	contracts::consts::{
		DEPLOYMENT_PRIVATE,
		DEPLOYMENT_SERVICES_PRIVATE,
	},
	errors::ContractDefError,
};

/// The contract Manager result type
pub type Result<T> = std::result::Result<T, ContractDefError>;

/// A contract ABI wrapper.
#[derive(Clone)]
pub struct Contract {
	pub abi: Vec<u8>,
}

impl Contract {
	/// Returns a new instance of `Contract`.
	pub fn new(abi: Vec<u8>) -> Self {
		Contract { abi }
	}
}

impl TryInto<ethabi::Contract> for Contract {
	type Error = ContractDefError;

	fn try_into(self) -> std::result::Result<ethabi::Contract, Self::Error> {
		ethabi::Contract::load(self.abi.as_slice()).map_err(ContractDefError::ABI)
	}
}

impl TryInto<ethabi::Contract> for &Contract {
	type Error = ContractDefError;

	fn try_into(self) -> std::result::Result<ethabi::Contract, Self::Error> {
		ethabi::Contract::load(self.abi.as_slice()).map_err(ContractDefError::ABI)
	}
}

/// Information about a deployed contract.
#[derive(Clone)]
pub struct DeployedContract {
	pub address: Address,
	pub block: U64,
}

/// A manager container for contracts ABIs and their deployments.
pub struct ContractsManager {
	version: String,
	contracts: HashMap<String, Contract>,
	deployment: Map<String, Value>,
	deployment_services: Map<String, Value>,
}

impl ContractsManager {
	/// Returns a new instance of `ContractsManager`.
	pub fn new(chain_id: ChainID) -> Result<Self> {
		let chain_deployment = match chain_id {
			ChainID::Mainnet => DEPLOYMENT_MAINNET,
			ChainID::Ropsten => DEPLOYMENT_ROPSTEN,
			ChainID::Goerli => DEPLOYMENT_GOERLI,
			ChainID::Rinkeby => DEPLOYMENT_RINKEBY,
			ChainID::Private(_) => DEPLOYMENT_PRIVATE,
		};
		let chain_deployment_services = match chain_id {
			ChainID::Mainnet => DEPLOYMENT_SERVICES_MAINNET,
			ChainID::Ropsten => DEPLOYMENT_SERVICES_ROPSTEN,
			ChainID::Goerli => DEPLOYMENT_SERVICES_GOERLI,
			ChainID::Rinkeby => DEPLOYMENT_SERVICES_RINKEBY,
			ChainID::Private(_) => DEPLOYMENT_SERVICES_PRIVATE,
		};
		let contracts_specs: serde_json::Value = serde_json::from_str(CONTRACTS)?;
		let contracts_deployment: serde_json::Value = serde_json::from_str(chain_deployment)?;
		let contracts_deployment_services: serde_json::Value =
			serde_json::from_str(chain_deployment_services)?;

		let contracts_version = contracts_specs
			.get("contracts_version")
			.ok_or(ContractDefError::SpecNotFound)?
			.as_str()
			.ok_or(ContractDefError::SpecNotFound)?;
		let specs_map = contracts_specs
			.get("contracts")
			.ok_or(ContractDefError::SpecNotFound)?
			.as_object()
			.ok_or(ContractDefError::SpecNotFound)?;

		let deployment = contracts_deployment
			.get("contracts")
			.ok_or(ContractDefError::SpecNotFound)?
			.as_object()
			.ok_or(ContractDefError::SpecNotFound)?;

		let deployment_services = contracts_deployment_services
			.get("contracts")
			.ok_or(ContractDefError::SpecNotFound)?
			.as_object()
			.ok_or(ContractDefError::SpecNotFound)?;

		let mut manager = Self {
			version: contracts_version.to_string(),
			contracts: HashMap::new(),
			deployment: deployment.clone(),
			deployment_services: deployment_services.clone(),
		};

		for (contract_name, contract_data) in specs_map.iter() {
			manager.contracts.insert(
				contract_name.clone(),
				Contract::new(serde_json::to_vec(contract_data.get("abi").unwrap()).unwrap()),
			);
		}

		Ok(manager)
	}

	/// Get a contract by identifier.
	pub fn get(&self, contract_identifier: ContractIdentifier) -> Contract {
		self.contracts.get(&contract_identifier.to_string()).cloned().unwrap()
	}

	/// Get the list of deployed addresses.
	pub fn deployed_addresses(&self) -> Result<DefaultAddresses> {
		let token_network_registry_deployed_contract =
			self.get_deployed(ContractIdentifier::TokenNetworkRegistry)?;

		let secret_registry_deployed_contract =
			self.get_deployed(ContractIdentifier::SecretRegistry)?;

		let service_registry_deployed_contract =
			self.get_deployed(ContractIdentifier::ServiceRegistry)?;

		let monitoring_service_deployed_contract =
			self.get_deployed(ContractIdentifier::MonitoringService)?;

		let user_deposit_deployed_contract = self.get_deployed(ContractIdentifier::UserDeposit)?;

		let one_to_n_deployed_contract = self.get_deployed(ContractIdentifier::OneToN)?;

		Ok(DefaultAddresses {
			contracts_version: self.version.clone(),
			service_registry: service_registry_deployed_contract.address,
			secret_registry: secret_registry_deployed_contract.address,
			token_network_registry: token_network_registry_deployed_contract.address,
			one_to_n: one_to_n_deployed_contract.address,
			monitoring_service: monitoring_service_deployed_contract.address,
			user_deposit: user_deposit_deployed_contract.address,
		})
	}

	/// Gets a deployed contract information.
	pub fn get_deployed(
		&self,
		contract_identifier: ContractIdentifier,
	) -> Result<DeployedContract> {
		let address = match self
			.deployment
			.get(&contract_identifier.to_string())
			.or(self.deployment_services.get(&contract_identifier.to_string()))
			.ok_or(ContractDefError::SpecNotFound)?
			.as_object()
			.ok_or(ContractDefError::Other("Invalid object"))?
			.get("address")
			.ok_or(ContractDefError::Other("No address found"))?
			.as_str()
			.ok_or(ContractDefError::Other("Address not a string"))?
			.trim_start_matches("0x")
			.parse()
		{
			Ok(value) => value,
			Err(_) => Address::zero(),
		};

		let block_number = self
			.deployment
			.get(&contract_identifier.to_string())
			.or(self.deployment_services.get(&contract_identifier.to_string()))
			.ok_or(ContractDefError::SpecNotFound)?
			.as_object()
			.ok_or(ContractDefError::Other("Invalid object"))?
			.get("block_number")
			.map(|v| v.as_u64().unwrap())
			.ok_or(ContractDefError::Other("No deployment block number found"))?;

		let block_number = U64::from(block_number);

		Ok(DeployedContract { address, block: block_number })
	}

	/// Gets the list of events from a specific contract.
	pub fn events(&self, contract_identifier: Option<ContractIdentifier>) -> Vec<Event> {
		match contract_identifier {
			Some(id) => {
				let contract: ethabi::Contract = self.get(id).try_into().unwrap();
				contract.events().cloned().collect()
			},
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
			},
		}
	}
}
