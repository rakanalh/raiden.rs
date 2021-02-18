use std::sync::Arc;

use futures::{SinkExt, channel::mpsc};
use parking_lot::RwLock;
use web3::{Web3, transports::Http, types::U64};

use raiden::{blockchain::{contracts::{Contract, ContractIdentifier, ContractRegistry}, events::Event}, state_machine::types::StateChange, state_manager::StateManager};

use super::TransitionService;

pub struct SyncService {
	web3: Web3<Http>,
	state_manager: Arc<RwLock<StateManager>>,
	contracts_registry: Arc<RwLock<ContractRegistry>>,
	transition_service: Arc<TransitionService>,
}

impl SyncService {
    pub fn new(
		web3: Web3<Http>,
		state_manager: Arc<RwLock<StateManager>>,
		contracts_registry: Arc<RwLock<ContractRegistry>>,
		transition_service: Arc<TransitionService>,
	) -> Self {
		Self {
			web3,
			state_manager,
			contracts_registry,
			transition_service,
		}
	}

    pub async fn sync(
		&mut self,
		start_block_number: U64,
		end_block_number: U64,
	) {
        let token_network_registry = self.contracts_registry.read().token_network_registry();
        self.poll_contract_filters(
            &token_network_registry,
            start_block_number,
            end_block_number,
        )
        .await;

        let token_networks = self
            .contracts_registry
			.read()
            .contracts
            .get(&ContractIdentifier::TokenNetwork)
            .unwrap_or(&vec![])
            .clone();

        for token_network in token_networks {
            self.poll_contract_filters(
				&token_network,
				start_block_number,
				end_block_number,
			).await;
        }
    }

    pub async fn poll_contract_filters(
		&mut self,
		contract: &Contract,
		from_block: U64,
		to_block: U64
	) {
        let filter = contract.filters(from_block, to_block);
		let contracts= &self.contracts_registry.read().contracts.clone();
        match self.web3.eth().logs((filter).clone()).await {
            Ok(logs) => {
                for log in logs {
                    let current_state = self.state_manager.read().current_state.clone();
                    // TODO: Event::to_state_change doesn't make sense
                    // TODO: Make trait ToStateChange and implement on Log
                    let state_change = Event::to_state_change(&current_state, contracts, &log);
                    if let Some(state_change) = state_change {
						self.transition_service.transition(state_change).await
                        // if let Err(_e) = self.transition_service.transition(state_change).await {
                        //     // error!(self.log, "State transition failed: {}", e);
                        // }
                    }
                }
            }
            Err(_e) => { } //error!(self.log, "Error fetching logs {}", e),
        }
    }
}
