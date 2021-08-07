use std::{
    cmp,
    sync::Arc,
};

use parking_lot::RwLock;
use slog::Logger;
use web3::{
    transports::Http,
    types::{
        BlockId,
        BlockNumber,
    },
    Web3,
};

use raiden::{
    blockchain::{
        contracts::ContractsManager,
        decode::EventDecoder,
        events::Event,
        filters::filters_from_chain_state,
        proxies::ProxyManager,
    },
    primitives::{
        RaidenConfig,
        U64,
    },
    services::Transitioner,
    state_machine::types::{
        Block,
        StateChange,
    },
    state_manager::StateManager,
};

struct BlockBatchSizeConfig {
    min: u64,
    _warn_threshold: u64,
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
    config: RaidenConfig,
    state_manager: Arc<RwLock<StateManager>>,
    contracts_manager: Arc<ContractsManager>,
    proxy_manager: Arc<ProxyManager>,
    transition_service: Arc<dyn Transitioner>,
    block_batch_size_adjuster: BlockBatchSizeAdjuster,
    logger: Logger,
}

impl SyncService {
    pub fn new(
        web3: Web3<Http>,
        config: RaidenConfig,
        state_manager: Arc<RwLock<StateManager>>,
        contracts_manager: Arc<ContractsManager>,
        proxy_manager: Arc<ProxyManager>,
        transition_service: Arc<dyn Transitioner>,
        logger: Logger,
    ) -> Self {
        let block_batch_size_adjuster = BlockBatchSizeAdjuster::new(
            BlockBatchSizeConfig {
                min: 5,
                max: 100000,
                initial: 1000,
                _warn_threshold: 50,
            },
            2.0, // base
            1.0, //step size
        );

        Self {
            web3,
            config,
            state_manager,
            contracts_manager,
            proxy_manager,
            transition_service,
            block_batch_size_adjuster,
            logger,
        }
    }

    pub async fn sync(&mut self, start_block_number: U64, end_block_number: U64) {
        info!(
            self.logger,
            "Sync started: {} -> {}", start_block_number, end_block_number
        );
        self.poll_contract_filters(start_block_number, end_block_number).await;
    }

    pub async fn poll_contract_filters(&mut self, start_block_number: U64, end_block_number: U64) {
        let mut from_block = start_block_number;

        while from_block < end_block_number {
            let to_block = cmp::min(
                from_block + self.block_batch_size_adjuster.batch_size().into(),
                end_block_number,
            );

            debug!(self.logger, "Querying from blocks {} to {}", from_block, to_block);

            let mut current_state = self.state_manager.read().current_state.clone();
            let filter = filters_from_chain_state(
                self.contracts_manager.clone(),
                current_state.clone(),
                from_block,
                to_block,
            );

            let logs = match self.web3.eth().logs((filter).clone()).await {
                Ok(logs) => logs,
                Err(e) => {
                    warn!(self.logger, "Error fetching logs: {:?}", e);
                    self.block_batch_size_adjuster.decrease();
                    continue;
                }
            };

            for log in logs {
                let event = match Event::decode(self.contracts_manager.clone(), &log) {
                    Some(event) => event,
                    None => {
                        warn!(self.logger, "Could not find event that matches log: {:?}", log);
                        continue;
                    }
                };

                current_state = self.state_manager.read().current_state.clone();
                let decoder = EventDecoder::new(self.config.clone(), self.proxy_manager.clone());
                let storage = self.state_manager.read().storage.clone();
                match decoder
                    .as_state_change(event.clone(), &current_state.clone(), storage)
                    .await
                {
                    Ok(Some(state_change)) => {
                        self.transition_service.transition(state_change).await;
                    }
                    Err(e) => {
                        warn!(
                            self.logger,
                            "Error converting chain event to state change: {:?} ({})", e, event.name
                        );
                    }
                    _ => {}
                };
            }

            let block_number = BlockNumber::Number(*to_block);
            let block = match self.web3.eth().block(BlockId::Number(block_number)).await {
                Ok(Some(block)) => block,
                Ok(None) => {
                    continue;
                }
                Err(e) => {
                    error!(self.logger, "Error fetching block info: {:?}", e);
                    continue;
                }
            };

            let block_state_change = StateChange::Block(Block {
                block_number: to_block,
                block_hash: block.hash.unwrap(),
                gas_limit: block.gas_limit,
            });
            self.transition_service.transition(block_state_change).await;

            from_block = to_block + 1u64.into();
            self.block_batch_size_adjuster.increase();
        }
    }
}
