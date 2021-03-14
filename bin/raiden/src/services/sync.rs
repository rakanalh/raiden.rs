use std::{
    cmp,
    sync::Arc,
};

use parking_lot::RwLock;
use slog::Logger;
use web3::{
    transports::Http,
    types::U64,
    Web3,
};

use raiden::{
    blockchain::{
        contracts::ContractsManager,
        events::Event,
        filters::filters_from_chain_state,
    },
    state_machine::types::StateChange,
    state_manager::StateManager,
};

use super::TransitionService;

struct BlockBatchSizeConfig {
    min: u64,
    warn_threshold: u64,
    initial: u32,
    max: u64,
}

struct BlockBatchSizeAdjuster {
    config: BlockBatchSizeConfig,
    scale_current: f64,
    base: f64,
    step_size: f64,
}

impl BlockBatchSizeAdjuster {
    fn new(config: BlockBatchSizeConfig, base: f64, step_size: f64) -> Self {
        let initial: f64 = config.initial.into();
        let scale_current = initial.log(base);
        Self {
            config,
            base,
            step_size,
            scale_current,
        }
    }

    fn increase(&mut self) {
        let prev_batch_size = self.batch_size();
        if prev_batch_size >= self.config.max {
            return;
        }
        self.scale_current += self.step_size;
    }

    fn decrease(&mut self) {
        let prev_batch_size = self.batch_size();
        if prev_batch_size <= self.config.min {
            return;
        }
        self.scale_current -= self.step_size;
    }

    fn batch_size(&self) -> u64 {
        cmp::max(
            self.config.min,
            cmp::min(self.config.max, self.base.powf(self.scale_current) as u64),
        )
    }
}

pub struct SyncService {
    web3: Web3<Http>,
    state_manager: Arc<RwLock<StateManager>>,
    contracts_manager: Arc<ContractsManager>,
    transition_service: Arc<TransitionService>,
    block_batch_size_adjuster: BlockBatchSizeAdjuster,
    logger: Logger,
}

impl SyncService {
    pub fn new(
        web3: Web3<Http>,
        state_manager: Arc<RwLock<StateManager>>,
        contracts_manager: Arc<ContractsManager>,
        transition_service: Arc<TransitionService>,
        logger: Logger,
    ) -> Self {
        let block_batch_size_adjuster = BlockBatchSizeAdjuster::new(
            BlockBatchSizeConfig {
                min: 5,
                max: 100000,
                initial: 1000,
                warn_threshold: 50,
            },
            2.0, // base
            1.0, //step size
        );

        Self {
            web3,
            state_manager,
            contracts_manager,
            transition_service,
            block_batch_size_adjuster,
            logger,
        }
    }

    pub async fn sync(&mut self, start_block_number: U64, end_block_number: U64) {
        self.poll_contract_filters(start_block_number, end_block_number).await;
    }

    pub async fn poll_contract_filters(&mut self, start_block_number: U64, end_block_number: U64) {
        let mut from_block = start_block_number;

        // Clone here to prevent holding the lock
        let current_state = &self.state_manager.read().current_state.clone();
        let our_address = current_state.our_address.clone();

        while from_block < end_block_number {
            let to_block = cmp::min(
                from_block + self.block_batch_size_adjuster.batch_size(),
                end_block_number,
            );

            debug!(self.logger, "Querying from blocks {} to {}", from_block, to_block);

            let filter = filters_from_chain_state(
                self.contracts_manager.clone(),
                current_state.clone(),
                from_block,
                to_block,
            );

            match self.web3.eth().logs((filter).clone()).await {
                Ok(logs) => {
                    for log in logs {
                        let current_state = &self.state_manager.read().current_state.clone();
                        let state_change = Event::from_log(self.contracts_manager.clone(), &log)
                            .map(|e| StateChange::from_blockchain_event(current_state, e))
                            .flatten();
                        match state_change {
                            Some(state_change) => self.transition_service.transition(state_change).await,
                            None => {
                                error!(self.logger, "Error converting log to state change: {:?}", log);
                            }
                        }
                    }
                    from_block = to_block + 1;
                    self.block_batch_size_adjuster.increase();
                }
                Err(_) => {
                    self.block_batch_size_adjuster.decrease();
                }
            }
        }
    }
}
