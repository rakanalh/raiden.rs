use std::{
	ops::Mul,
	sync::Arc,
};

use raiden_primitives::types::{
	Address,
	U256,
};
use raiden_state_machine::{
	types::ChainState,
	views,
};
use web3::Transport;

use super::{
	common::{
		Account,
		Result,
	},
	ProxyManager,
};
use crate::{
	constants::{
		GAS_RESERVE_ESTIMATE_SECURITY_FACTOR,
		UNLOCK_TX_GAS_LIMIT,
	},
	contracts::GasMetadata,
};

const GAS_REQUIRED_FOR_CHANNEL_LIFECYCLE_AFTER_SETTLE: u64 = UNLOCK_TX_GAS_LIMIT;

pub struct GasReserve {
	proxy_manager: Arc<ProxyManager>,
	registry_address: Address,
	gas_metadata: Arc<GasMetadata>,
}

impl GasReserve {
	pub fn new(proxy_manager: Arc<ProxyManager>, registry_address: Address) -> Self {
		Self { gas_metadata: proxy_manager.gas_metadata(), proxy_manager, registry_address }
	}
	fn gas_required_for_channel_lifecycle_after_close(&self) -> u64 {
		self.gas_metadata.get("TokenNetwork.settleChannel") +
			GAS_REQUIRED_FOR_CHANNEL_LIFECYCLE_AFTER_SETTLE
	}

	fn gas_required_for_channel_lifecycle_after_open(&self) -> u64 {
		self.gas_metadata.get("TokenNetwork.closeChannel") +
			self.gas_required_for_channel_lifecycle_after_close()
	}

	fn gas_required_for_channel_lifecycle_complete(&self) -> u64 {
		self.gas_metadata.get("TokenNetwork.openChannel") +
			self.gas_metadata.get("TokenNetwork.setTotalDeposit") +
			self.gas_required_for_channel_lifecycle_after_open()
	}

	fn calc_required_gas_estimate(
		&self,
		opening_channels: u64,
		opened_channels: u64,
		closing_channels: u64,
		closed_channels: u64,
		settling_channels: u64,
		settled_channels: u64,
	) -> u64 {
		let mut estimate = 0;

		estimate += opening_channels * self.gas_required_for_channel_lifecycle_complete();
		estimate += opened_channels * self.gas_required_for_channel_lifecycle_after_open();
		estimate += closing_channels * self.gas_required_for_channel_lifecycle_after_close();
		estimate += closed_channels * self.gas_required_for_channel_lifecycle_after_close();
		estimate += settling_channels * GAS_REQUIRED_FOR_CHANNEL_LIFECYCLE_AFTER_SETTLE;
		estimate += settled_channels * GAS_REQUIRED_FOR_CHANNEL_LIFECYCLE_AFTER_SETTLE;

		estimate
	}

	async fn get_required_gas_estimate(
		&self,
		chain_state: &ChainState,
		channels_to_open: u64,
	) -> Result<u64> {
		let mut num_opened_channels = 0;
		let mut num_closing_channels = 0;
		let mut num_closed_channels = 0;
		let mut num_settling_channels = 0;
		let mut num_settled_channels = 0;

		let num_opening_channels = self
			.proxy_manager
			.token_networks
			.read()
			.await
			.values()
			.fold(0u64, |sum, tn| sum + tn.opening_channels_count as u64);

		let token_addresses = views::get_token_identifiers(&chain_state, self.registry_address);

		for token_address in token_addresses {
			num_opened_channels +=
				views::get_channelstate_open(&chain_state, self.registry_address, token_address)
					.len() as u64;
			num_closing_channels +=
				views::get_channelstate_closing(&chain_state, self.registry_address, token_address)
					.len() as u64;
			num_closed_channels +=
				views::get_channelstate_closed(&chain_state, self.registry_address, token_address)
					.len() as u64;
			num_settling_channels += views::get_channelstate_settling(
				&chain_state,
				self.registry_address,
				token_address,
			)
			.len() as u64;
			num_settled_channels +=
				views::get_channelstate_settled(&chain_state, self.registry_address, token_address)
					.len() as u64;
		}

		Ok(self.calc_required_gas_estimate(
			num_opening_channels + channels_to_open,
			num_opened_channels,
			num_closing_channels,
			num_closed_channels,
			num_settling_channels,
			num_settled_channels,
		))
	}

	pub async fn get_estimate(
		&self,
		chain_state: &ChainState,
		channels_to_open: u64,
	) -> Result<U256> {
		let gas_estimate = self.get_required_gas_estimate(chain_state, channels_to_open).await?;
		let gas_price = self.proxy_manager.web3().eth().gas_price().await?;

		let reserve_amount: U256 = gas_price.mul(gas_estimate);

		Ok(reserve_amount.mul((100.0 * GAS_RESERVE_ESTIMATE_SECURITY_FACTOR).round() as u32 / 100))
	}

	pub async fn has_enough<T: Transport>(
		&self,
		account: Account<T>,
		chain_state: &ChainState,
		channels_to_open: u64,
	) -> Result<(bool, U256)> {
		let gas_reserve_estimate = self.get_estimate(chain_state, channels_to_open).await?;
		let balance = self.proxy_manager.web3().eth().balance(account.address(), None).await?;

		Ok((gas_reserve_estimate < balance, gas_reserve_estimate))
	}
}
