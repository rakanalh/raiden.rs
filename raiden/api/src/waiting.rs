use std::{
	collections::HashSet,
	sync::Arc,
};

use parking_lot::RwLock;
use raiden_primitives::types::{
	Address,
	CanonicalIdentifier,
	RetryTimeout,
	TokenAddress,
	TokenAmount,
	U256,
	U64,
};
use raiden_state_machine::{
	constants::DEFAULT_RETRY_TIMEOUT,
	types::{
		ChannelState,
		ChannelStatus,
	},
	views,
};
use raiden_transition::manager::StateManager;
use tokio::time::{
	sleep,
	Duration,
};
use web3::{
	transports::Http,
	Web3,
};

use crate::api::ApiError;

pub async fn wait_for_token_network(
	state_manager: Arc<RwLock<StateManager>>,
	registry_address: Address,
	token_address: TokenAddress,
	retry_timeout: Option<RetryTimeout>,
) -> Result<(), ApiError> {
	let retry_timeout = match retry_timeout {
		Some(timeout) => Duration::from_millis(timeout),
		None => Duration::from_millis(DEFAULT_RETRY_TIMEOUT),
	};

	loop {
		let chain_state = state_manager.read().current_state.clone();
		let token_network = views::get_token_network_by_token_address(
			&chain_state,
			registry_address,
			token_address,
		);
		if token_network.is_some() {
			break
		}
		sleep(retry_timeout).await;
	}

	Ok(())
}

pub async fn wait_for_new_channel(
	state_manager: Arc<RwLock<StateManager>>,
	registry_address: Address,
	token_address: TokenAddress,
	partner_address: Address,
	retry_timeout: Option<RetryTimeout>,
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

pub async fn wait_for_close(
	state_manager: Arc<RwLock<StateManager>>,
	canonical_ids: Vec<CanonicalIdentifier>,
	retry_timeout: Option<RetryTimeout>,
) -> Result<(), ApiError> {
	let retry_timeout = retry_timeout
		.map(|t| Duration::from_millis(t))
		.unwrap_or(Duration::from_millis(DEFAULT_RETRY_TIMEOUT));

	loop {
		let chain_state = state_manager.read().current_state.clone();
		let mut all_closed = true;
		for canonical_id in canonical_ids.iter() {
			let channel_state = match views::get_channel_by_canonical_identifier(
				&chain_state,
				canonical_id.clone(),
			) {
				Some(channel_state) => channel_state,
				None =>
					return Err(ApiError::State(format!(
						"No channel could be found for provided canonical identifier"
					))),
			};
			let channel_status = channel_state.status();
			if channel_status != ChannelStatus::Opened && channel_status != ChannelStatus::Closing {
				all_closed = false;
			}
		}
		if all_closed {
			return Ok(())
		}
		sleep(retry_timeout).await;
	}
}

pub async fn wait_for_coop_settle(
	web3: Web3<Http>,
	state_manager: Arc<RwLock<StateManager>>,
	canonical_ids: Vec<CanonicalIdentifier>,
	retry_timeout: Option<RetryTimeout>,
) -> Result<(), ApiError> {
	let retry_timeout = retry_timeout
		.map(|t| Duration::from_millis(t))
		.unwrap_or(Duration::from_millis(DEFAULT_RETRY_TIMEOUT));

	loop {
		let chain_state = state_manager.read().current_state.clone();
		let mut completed: HashSet<CanonicalIdentifier> = HashSet::new();
		for canonical_id in canonical_ids.iter() {
			let channel_state = match views::get_channel_by_canonical_identifier(
				&chain_state,
				canonical_id.clone(),
			) {
				Some(channel_state) => channel_state,
				None =>
					return Err(ApiError::State(format!(
						"No channel could be found for provided canonical identifier"
					))),
			};
			let mut expired = true;
			let mut settled = true;
			if let Some(coop_settle) = &channel_state.our_state.initiated_coop_settle {
				let current_block_number: U64 =
					web3.eth().block_number().await.map_err(ApiError::Web3)?.into();
				if current_block_number < coop_settle.expiration {
					expired = false;
				}
			}
			let channel_status = channel_state.status();
			if channel_status != ChannelStatus::Settled {
				settled = false;
			}

			if expired && settled {
				completed.insert(canonical_id.clone());
			} else if expired && !settled {
				completed.insert(canonical_id.clone());
			} else if !expired && settled {
				completed.insert(canonical_id.clone());
			}
		}

		if completed.len() == canonical_ids.len() {
			return Ok(())
		}
		sleep(retry_timeout).await;
	}
}

pub async fn wait_for_participant_deposit(
	state_manager: Arc<RwLock<StateManager>>,
	registry_address: Address,
	token_address: TokenAddress,
	partner_address: Address,
	target_address: Address,
	target_balance: U256,
	retry_timeout: Option<RetryTimeout>,
) -> Result<(), ApiError> {
	let retry_timeout = retry_timeout
		.map(|t| Duration::from_millis(t))
		.unwrap_or(Duration::from_millis(DEFAULT_RETRY_TIMEOUT));

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

pub async fn wait_for_withdraw_complete(
	state_manager: Arc<RwLock<StateManager>>,
	canonical_identifier: CanonicalIdentifier,
	total_withdraw: TokenAmount,
	retry_timeout: Option<RetryTimeout>,
) -> Result<(), ApiError> {
	let retry_timeout = retry_timeout
		.map(|t| Duration::from_millis(t))
		.unwrap_or(Duration::from_millis(DEFAULT_RETRY_TIMEOUT));

	loop {
		let chain_state = state_manager.read().current_state.clone();
		let channel_state = match views::get_channel_by_canonical_identifier(
			&chain_state,
			canonical_identifier.clone(),
		) {
			Some(channel_state) => channel_state,
			None =>
				return Err(ApiError::State(format!(
					"No channel could be found for provided canonical identifier"
				))),
		};

		if channel_state.our_state.onchain_total_withdraw == total_withdraw {
			return Ok(())
		}
		sleep(retry_timeout).await;
	}
}
