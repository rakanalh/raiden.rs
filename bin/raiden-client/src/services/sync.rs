use std::{cmp, sync::Arc};

use web3::types::{BlockId, BlockNumber};

use raiden::{
	blockchain::{decode::EventDecoder, events::Event, filters::filters_from_chain_state},
	primitives::U64,
	raiden::Raiden,
	services::Transitioner,
	state_machine::types::Block,
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
		Self { config, base, step_size, scale_current }
	}

	fn increase(&mut self) {
		let prev_batch_size = self.batch_size();
		if prev_batch_size >= self.config.max {
			return
		}
		self.scale_current += self.step_size;
	}

	fn decrease(&mut self) {
		let prev_batch_size = self.batch_size();
		if prev_batch_size <= self.config.min {
			return
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
	raiden: Arc<Raiden>,
	transition_service: Arc<dyn Transitioner>,
	block_batch_size_adjuster: BlockBatchSizeAdjuster,
}

impl SyncService {
	pub fn new(raiden: Arc<Raiden>, transition_service: Arc<dyn Transitioner>) -> Self {
		let block_batch_size_adjuster = BlockBatchSizeAdjuster::new(
			BlockBatchSizeConfig { min: 5, max: 100000, initial: 1000, _warn_threshold: 50 },
			2.0, // base
			1.0, //step size
		);

		Self { raiden, transition_service, block_batch_size_adjuster }
	}

	pub async fn sync(&mut self, start_block_number: U64, end_block_number: U64) {
		info!(self.raiden.logger, "Sync started: {} -> {}", start_block_number, end_block_number);
		self.poll_contract_filters(start_block_number, end_block_number).await;
	}

	pub async fn poll_contract_filters(&mut self, start_block_number: U64, end_block_number: U64) {
		let mut from_block = start_block_number;

		while from_block < end_block_number {
			let to_block = cmp::min(
				from_block + self.block_batch_size_adjuster.batch_size().into(),
				end_block_number,
			);

			debug!(self.raiden.logger, "Querying from blocks {} to {}", from_block, to_block);

			let mut current_state = self.raiden.state_manager.read().current_state.clone();
			let filter = filters_from_chain_state(
				self.raiden.contracts_manager.clone(),
				current_state.clone(),
				from_block,
				to_block,
			);

			let logs = match self.raiden.web3.eth().logs((filter).clone()).await {
				Ok(logs) => logs,
				Err(e) => {
					warn!(self.raiden.logger, "Error fetching logs: {:?}", e);
					self.block_batch_size_adjuster.decrease();
					continue
				},
			};

			for log in logs {
				let event = match Event::decode(self.raiden.contracts_manager.clone(), &log) {
					Some(event) => event,
					None => {
						warn!(
							self.raiden.logger,
							"Could not find event that matches log: {:?}", log
						);
						continue
					},
				};

				current_state = self.raiden.state_manager.read().current_state.clone();
				let decoder = EventDecoder::new(
					self.raiden.config.clone(),
					self.raiden.proxy_manager.clone(),
				);
				let storage = self.raiden.state_manager.read().storage.clone();
				match decoder.as_state_change(event.clone(), &current_state.clone(), storage).await
				{
					Ok(Some(state_change)) => {
						self.transition_service.transition(state_change).await;
					},
					Err(e) => {
						warn!(
							self.raiden.logger,
							"Error converting chain event to state change: {:?} ({})",
							e,
							event.name
						);
					},
					_ => {},
				};
			}

			let block_number = BlockNumber::Number(*to_block);
			let block = match self.raiden.web3.eth().block(BlockId::Number(block_number)).await {
				Ok(Some(block)) => block,
				Ok(None) => continue,
				Err(e) => {
					error!(self.raiden.logger, "Error fetching block info: {:?}", e);
					continue
				},
			};

			let block_state_change = Block {
				block_number: to_block,
				block_hash: block.hash.unwrap(),
				gas_limit: block.gas_limit,
			};
			self.transition_service.transition(block_state_change.into()).await;

			from_block = to_block + 1u64.into();
			self.block_batch_size_adjuster.increase();
		}
	}
}
