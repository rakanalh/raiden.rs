use std::{
	collections::HashMap,
	sync::Arc,
};

use raiden_primitives::types::{
	Address,
	H256,
	U64,
};
use web3::types::Log;

use super::contracts::ContractsManager;

/// Contains information about the event triggered on the Ethereum chain.
#[derive(Clone, Debug)]
pub struct Event {
	pub name: String,
	pub address: Address,
	pub block_number: U64,
	pub block_hash: H256,
	pub transaction_hash: H256,
	pub data: HashMap<String, ethabi::Token>,
}

impl Event {
	/// Decodes a log into an event based on information about the contracts from the contracts
	/// manager.
	///
	/// Returns None if the event is unknown.
	pub fn decode(contracts_manager: Arc<ContractsManager>, log: &Log) -> Option<Event> {
		let events = contracts_manager.events(None);
		for event in events {
			if !log.topics.is_empty() && event.signature() == log.topics[0] {
				let non_indexed_inputs: Vec<(String, &ethabi::EventParam)> = event
					.inputs
					.iter()
					.filter(|input| !input.indexed)
					.map(|input| (input.name.clone(), input))
					.collect();

				let indexed_inputs: Vec<(String, &ethabi::EventParam)> = event
					.inputs
					.iter()
					.filter(|input| input.indexed)
					.map(|input| (input.name.clone(), input))
					.collect();

				let mut data: HashMap<String, ethabi::Token> = HashMap::new();

				if log.topics.len() >= 2 {
					let mut indexed_inputs = indexed_inputs.into_iter();
					for topic in &log.topics[1..] {
						let (name, input) = indexed_inputs.next()?;
						if let Ok(decoded_value) = ethabi::decode(&[input.kind.clone()], &topic.0) {
							data.insert(name.clone(), decoded_value[0].clone());
						}
					}
				}

				if !log.data.0.is_empty() {
					for (name, input) in non_indexed_inputs {
						if let Ok(decoded_value) =
							ethabi::decode(&[input.kind.clone()], &log.data.0)
						{
							data.insert(name, decoded_value[0].clone());
						}
					}
				}

				return Some(Event {
					name: event.name.clone(),
					address: log.address,
					block_number: log.block_number.unwrap().into(),
					block_hash: log.block_hash.unwrap(),
					transaction_hash: log.transaction_hash.unwrap(),
					data,
				})
			}
		}
		None
	}
}
