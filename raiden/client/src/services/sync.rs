use std::{
	cmp,
	sync::Arc,
};

use raiden_api::raiden::Raiden;
use raiden_blockchain::{
	decode::EventDecoder,
	events::Event,
	filters::filters_from_chain_state,
};
use raiden_primitives::types::U64;
use raiden_state_machine::types::Block;
use raiden_transition::Transitioner;
use tracing::{
	debug,
	error,
	info,
};
use web3::types::{
	BlockId,
	BlockNumber,
};

/// Configuration for adjusting batch size.
struct BlockBatchSizeConfig {
	min: u64,
	_warn_threshold: u64,
	initial: u32,
	max: u64,
}

/// Adjust batch size based on error rate.
struct BlockBatchSizeAdjuster {
	config: BlockBatchSizeConfig,
	scale_current: f64,
	base: f64,
	step_size: f64,
}

impl BlockBatchSizeAdjuster {
	/// Return a instance of `BlockBatchSizeAdjuster`.
	fn new(config: BlockBatchSizeConfig, base: f64, step_size: f64) -> Self {
		let initial: f64 = config.initial.into();
		let scale_current = initial.log(base);
		Self { config, base, step_size, scale_current }
	}

	/// Increase batch size.
	fn increase(&mut self) {
		let prev_batch_size = self.batch_size();
		if prev_batch_size >= self.config.max {
			return
		}
		self.scale_current += self.step_size;
	}

	/// Decrease batch size.
	fn decrease(&mut self) {
		let prev_batch_size = self.batch_size();
		if prev_batch_size <= self.config.min {
			return
		}
		self.scale_current -= self.step_size;
	}

	/// Return current batch size.
	fn batch_size(&self) -> u64 {
		cmp::max(
			self.config.min,
			cmp::min(self.config.max, self.base.powf(self.scale_current) as u64),
		)
	}
}

/// Raiden's sync service.
pub struct SyncService {
	raiden: Arc<Raiden>,
	transition_service: Arc<Transitioner>,
	block_batch_size_adjuster: BlockBatchSizeAdjuster,
}

impl SyncService {
	/// Return an instance of `SyncService`.
	pub fn new(raiden: Arc<Raiden>, transition_service: Arc<Transitioner>) -> Self {
		let block_batch_size_adjuster = BlockBatchSizeAdjuster::new(
			BlockBatchSizeConfig { min: 5, max: 100000, initial: 100, _warn_threshold: 50 },
			2.0, // base
			1.0, //step size
		);

		Self { raiden, transition_service, block_batch_size_adjuster }
	}

	/// Sync with the blockchain for events between start and end blocks.
	pub async fn sync(&mut self, start_block_number: U64, end_block_number: U64) {
		info!("Sync started: {} -> {}", start_block_number, end_block_number);
		self.poll_contract_filters(start_block_number, end_block_number).await;
	}

	/// Poll the blockchain, fetch events and convert them into state changes.
	pub async fn poll_contract_filters(&mut self, start_block_number: U64, end_block_number: U64) {
		let mut from_block = start_block_number;

		while from_block < end_block_number {
			let to_block = cmp::min(
				from_block + self.block_batch_size_adjuster.batch_size().into(),
				end_block_number,
			);

			debug!("Query chain events {} to {}", from_block, to_block);

			let mut current_state = self.raiden.state_manager.read().current_state.clone();
			let filter = filters_from_chain_state(
				self.raiden.config.addresses.clone(),
				self.raiden.contracts_manager.clone(),
				current_state.clone(),
				from_block,
				to_block,
			);

			let logs = match self.raiden.web3.eth().logs((filter).clone()).await {
				Ok(logs) => logs,
				Err(e) => {
					error!("Error fetching logs: {:?}", e);
					self.block_batch_size_adjuster.decrease();
					continue
				},
			};

			debug!(message = "Processing blockchain events", count = logs.len());

			for log in logs {
				let event = match Event::decode(self.raiden.contracts_manager.clone(), &log) {
					Some(event) => event,
					None => {
						error!("Could not find event that matches log: {:?}", log);
						continue
					},
				};

				current_state = self.raiden.state_manager.read().current_state.clone();
				let decoder = EventDecoder::new(
					self.raiden.config.mediation_config.clone(),
					self.raiden.config.default_reveal_timeout,
				);
				let storage = self.raiden.state_manager.read().storage.clone();
				match decoder.as_state_change(event.clone(), &current_state.clone(), storage).await
				{
					Ok(Some(state_change)) => {
						if let Err(e) = self.transition_service.transition(vec![state_change]).await
						{
							error!("{}", e);
						}
					},
					Err(e) => {
						error!(
							"Error converting chain event to state change: {:?} ({})",
							e, event.name
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
					error!("Error fetching block info: {:?}", e);
					continue
				},
			};

			let block_state_change = Block {
				block_number: to_block,
				block_hash: block.hash.unwrap(),
				gas_limit: block.gas_limit,
			};
			if let Err(e) =
				self.transition_service.transition(vec![block_state_change.into()]).await
			{
				error!("{}", e);
			}

			from_block = to_block + 1u64.into();
			self.block_batch_size_adjuster.increase();
		}
	}
}
