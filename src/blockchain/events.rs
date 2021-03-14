use std::{collections::HashMap, sync::Arc};

use crate::constants;
use crate::state_machine::state::{
    CanonicalIdentifier,
    ChannelState,
    TokenNetworkState,
    TransactionExecutionStatus,
    TransactionResult,
};
use crate::state_machine::types::StateChange;
use crate::state_machine::types::{
    ContractReceiveChannelOpened,
    ContractReceiveTokenNetworkCreated,
};
use ethabi::Token;
use web3::types::{
    Address,
    Log,
    H256,
    U256,
    U64,
};

use super::contracts::ContractsManager;

pub trait ToStateChange {
    fn to_state_change(&self, our_address: Address) -> Option<StateChange>;
}

#[derive(Clone, Debug)]
pub struct Event {
    pub name: String,
    pub address: Address,
    pub block_number: U64,
    pub block_hash: H256,
    pub transaction_hash: H256,
    pub data: HashMap<String, ethabi::Token>,
}

impl ToStateChange for Event {
    fn to_state_change(&self, our_address: Address) -> Option<StateChange> {
        match self.name.as_ref() {
            "TokenNetworkCreated" => self.create_token_network_created_state_change(),
            "ChannelOpened" => self.create_channel_opened_state_change(our_address),
            _ => None,
        }
    }
}

impl Event {
    pub fn from_log(contracts_manager: Arc<ContractsManager>, log: &Log) -> Option<Event> {
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
						if let Ok(decoded_value) = ethabi::decode(&[input.kind.clone()], &log.data.0) {
							data.insert(name, decoded_value[0].clone());
						}
					}
                }

                return Some(Event {
                    name: event.name.clone(),
                    address: log.address,
                    block_number: log.block_number.unwrap(),
                    block_hash: log.block_hash.unwrap(),
                    transaction_hash: log.transaction_hash.unwrap(),
                    data,
                });
            }
        }
        None
    }

    fn create_token_network_created_state_change(&self) -> Option<StateChange> {
        let token_address = match self.data.get("token_address")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let token_network_address = match self.data.get("token_network_address")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let token_network = TokenNetworkState::new(token_network_address, token_address);
        let token_network_registry_address = self.address;
        Some(StateChange::ContractReceiveTokenNetworkCreated(
            ContractReceiveTokenNetworkCreated {
                transaction_hash: Some(self.transaction_hash),
                block_number: self.block_number,
                block_hash: self.block_hash,
                token_network_registry_address,
                token_network,
            },
        ))
    }

    fn create_channel_opened_state_change(&self, our_address: Address) -> Option<StateChange> {
        let channel_identifier = match self.data.get("channel_identifier")? {
            Token::Uint(identifier) => identifier.clone(),
            _ => U256::zero(),
        };
        let participant1 = match self.data.get("participant1")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let participant2 = match self.data.get("participant2")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let settle_timeout = match self.data.get("settle_timeout")? {
            Token::Uint(timeout) => timeout.clone(),
            _ => U256::zero(),
        };

        let partner_address: Address;
        let our_address = our_address;
        if participant1 == our_address {
            partner_address = participant2;
        } else {
            partner_address = participant1;
        }
        // } else if participant2 == our_address {
        //     partner_address = participant1;
        // } else {
        //     return None;
        // }

        let chain_identifier = 1;
        let token_network_address = self.address;
        let token_address = Address::zero();
        let token_network_registry_address = Address::zero();
        let reveal_timeout = U256::from(constants::DEFAULT_REVEAL_TIMEOUT);
        let open_transaction = TransactionExecutionStatus {
            started_block_number: Some(U64::from(0)),
            finished_block_number: Some(self.block_number),
            result: Some(TransactionResult::Success),
        };
        let channel_state = ChannelState::new(
            CanonicalIdentifier {
                chain_identifier,
                token_network_address,
                channel_identifier,
            },
            token_address,
            token_network_registry_address,
            our_address,
            partner_address,
            reveal_timeout,
            settle_timeout,
            open_transaction,
        )
        .ok()?;

        Some(StateChange::ContractReceiveChannelOpened(
            ContractReceiveChannelOpened {
                transaction_hash: Some(self.transaction_hash),
                block_number: self.block_number,
                block_hash: self.block_hash,
                channel_state,
            },
        ))
    }
}
