use std::sync::Arc;

use parking_lot::RwLock;
use raiden_primitives::types::{
	Address,
	TokenAddress,
	U256,
};
use raiden_state_machine::{
	constants::DEFAULT_RETRY_TIMEOUT,
	types::ChannelState,
	views,
};
use raiden_transition::manager::StateManager;
use tokio::time::{
	sleep,
	Duration,
};

use crate::api::ApiError;

pub async fn wait_for_new_channel(
	state_manager: Arc<RwLock<StateManager>>,
	registry_address: Address,
	token_address: TokenAddress,
	partner_address: Address,
	retry_timeout: Option<u64>,
) -> Result<(), ApiError> {
	let retry_timeout = match retry_timeout {
		Some(timeout) => Duration::from_millis(timeout),
		None => Duration::from_millis(DEFAULT_RETRY_TIMEOUT),
	};
	let chain_state = state_manager.read().current_state.clone();
	let mut channel_state = views::get_channel_state_for(
		&chain_state,
		registry_address,
		token_address,
		partner_address,
	);

	while let None = channel_state {
		sleep(retry_timeout).await;
		channel_state = views::get_channel_state_for(
			&chain_state,
			registry_address,
			token_address,
			partner_address,
		);
	}

	Ok(())
}

pub async fn wait_for_participant_deposit(
	state_manager: Arc<RwLock<StateManager>>,
	registry_address: Address,
	token_address: TokenAddress,
	partner_address: Address,
	target_address: Address,
	target_balance: U256,
	retry_timeout: Option<u64>,
) -> Result<(), ApiError> {
	let retry_timeout = match retry_timeout {
		Some(timeout) => Duration::from_millis(timeout),
		None => Duration::from_millis(DEFAULT_RETRY_TIMEOUT),
	};

	let chain_state = state_manager.read().current_state.clone();
	let mut channel_state = match views::get_channel_state_for(
		&chain_state,
		registry_address,
		token_address,
		partner_address,
	) {
		Some(channel_state) => channel_state,
		None =>
			return Err(ApiError::State(format!(
				"No channel could be found between provided partner and target addresses"
			))),
	};

	let balance = if target_address == chain_state.our_address {
		|channel_state: &ChannelState| channel_state.our_state.contract_balance
	} else {
		|channel_state: &ChannelState| channel_state.partner_state.contract_balance
	};

	let mut current_balance = balance(channel_state);
	while current_balance < target_balance {
		sleep(retry_timeout).await;
		channel_state = views::get_channel_state_for(
			&chain_state,
			registry_address,
			token_address,
			partner_address,
		)
		.unwrap();
		current_balance = balance(channel_state);
	}

	Ok(())
}
