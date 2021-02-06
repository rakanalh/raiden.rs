use crate::blockchain::contracts::ContractRegistry;
use crate::constants;
use crate::enums::StateChange;
use crate::transfer::state::{
    CanonicalIdentifier, ChainState, ChannelState, TokenNetworkState, TransactionExecutionStatus, TransactionResult,
};
use crate::transfer::state_change::{ContractReceiveChannelOpened, ContractReceiveTokenNetworkCreated};
use ethabi::Token;
use web3::types::{Address, H256, Log, U256, U64};

#[derive(Clone, Debug)]
pub struct Event {
    pub name: String,
    pub block_number: U64,
    pub block_hash: H256,
    pub transaction_hash: H256,
    pub data: Vec<ethabi::Token>,
}

impl Event {
    pub fn from_log(contract_registry: &ContractRegistry, log: &Log) -> Option<Event> {
        for contracts in contract_registry.contracts.values() {
            let events = contracts.iter().flat_map(|contract| contract.events());
            for event in events {
                if !log.topics.is_empty() && event.signature() == log.topics[0] {
                    let non_indexed_inputs: Vec<ethabi::ParamType> = event
                        .inputs
                        .iter()
                        .filter(|input| !input.indexed)
                        .map(|input| input.kind.clone())
                        .collect();
                    let mut data: Vec<ethabi::Token> = vec![];

                    if log.topics.len() >= 2 {
                        let indexed_inputs: Vec<&ethabi::EventParam> =
                            event.inputs.iter().filter(|input| input.indexed).collect();
                        for topic in &log.topics[1..] {
                            if let Ok(decoded_value) = ethabi::decode(&[indexed_inputs[0].kind.clone()], &topic.0) {
                                data.push(decoded_value[0].clone());
                            }
                        }
                    }

                    if !log.data.0.is_empty() {
                        data.extend(ethabi::decode(&non_indexed_inputs, &log.data.0).unwrap());
                    }

                    return Some(Event {
                        name: event.name.clone(),
                        block_number: log.block_number.unwrap(),
                        block_hash: log.block_hash.unwrap(),
                        transaction_hash: log.transaction_hash.unwrap(),
                        data,
                    });
                }
            }
        }
        None
    }

	pub fn to_state_change(
		chain_state: &Option<ChainState>,
		contract_registry: &ContractRegistry,
		log: &Log,
	) -> Option<StateChange> {
		let event = Event::from_log(contract_registry, log)?;
		let chain_state = chain_state.as_ref().unwrap();

		match event.name.as_ref() {
			"TokenNetworkCreated" => event.create_token_network_created_state_change(log),
			"ChannelOpened" => event.create_channel_opened_state_change(&chain_state, log),
			_ => None,
		}
	}

	fn create_token_network_created_state_change(&self, log: &Log) -> Option<StateChange> {
		let token_address = match self.data[0] {
			Token::Address(address) => address,
			_ => Address::zero(),
		};
		let token_network_address = match self.data[1] {
			Token::Address(address) => address,
			_ => Address::zero(),
		};
		let token_network = TokenNetworkState::new(token_network_address, token_address);
		let token_network_registry_address = log.address;
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

	fn create_channel_opened_state_change(&self, chain_state: &ChainState, log: &Log) -> Option<StateChange> {
		let channel_identifier = match self.data[0] {
			Token::Uint(identifier) => identifier,
			_ => U256::zero(),
		};
		let participant1 = match self.data[1] {
			Token::Address(address) => address,
			_ => Address::zero(),
		};
		let participant2 = match self.data[2] {
			Token::Address(address) => address,
			_ => Address::zero(),
		};
		let settle_timeout = match self.data[3] {
			Token::Uint(timeout) => timeout,
			_ => U256::zero(),
		};

		let partner_address: Address;
		let our_address = chain_state.our_address;
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
		let token_network_address = log.address;
		let token_address = Address::zero();
		let token_network_registry_address = Address::zero();
		let reveal_timeout = U256::from(constants::DEFAULT_REVEAL_TIMEOUT);
		let open_transaction = TransactionExecutionStatus {
			started_block_number: Some(U64::from(0)),
			finished_block_number: Some(self.block_number),
			result: Some(TransactionResult::SUCCESS),
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
		).ok()?;

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

