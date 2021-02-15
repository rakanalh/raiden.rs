use std::sync::Arc;

use web3::types::U64;

use crate::{
    blockchain::{
		contracts::{
			Contract,
			ContractIdentifier,
		},
		events::Event,
	},
    raiden_service::RaidenService,
    state_machine::views,
};

struct SyncService {
    raiden: Arc<RaidenService>,
}

impl SyncService {
    fn new(raiden: Arc<RaidenService>) -> Self {
		Self {
			raiden,
		}
	}

    pub async fn sync(&mut self) {
        let block_number = views::block_number(&self.raiden.state_manager.read().await.current_state.as_ref().unwrap());

        let token_network_registry = self.raiden.contracts_registry.token_network_registry();
        self.poll_contract_filters(
            &token_network_registry,
            token_network_registry.deploy_block_number,
            block_number,
        )
        .await;

        let token_networks = self
			.raiden
            .contracts_registry
            .contracts
            .get(&ContractIdentifier::TokenNetwork)
            .unwrap_or(&vec![])
            .clone();

        for token_network in token_networks {
            self.poll_contract_filters(&token_network, token_network.deploy_block_number, block_number).await;
        }
    }

    pub async fn poll_contract_filters(&mut self, contract: &Contract, from_block: U64, to_block: U64) {
        let filter = contract.filters(from_block, to_block);
        match self.raiden.web3.eth().logs((filter).clone()).await {
            Ok(logs) => {
                for log in logs {
                    let current_state = self.raiden.state_manager.read().await.current_state.clone();
                    let contracts_registry = &self.raiden.contracts_registry;
                    // TODO: Event::to_state_change doesn't make sense
                    // TODO: Make trait ToStateChange and implement on Log
                    let state_change = Event::to_state_change(&current_state, contracts_registry, &log);
                    if let Some(state_change) = state_change {
                        if let Err(e) = self.raiden.transition(state_change).await {
                            // error!(self.log, "State transition failed: {}", e);
                        }
                    }
                }
            }
            Err(e) => { } //error!(self.log, "Error fetching logs {}", e),
        }
    }
}
