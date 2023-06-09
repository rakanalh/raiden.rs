use std::{
	collections::HashMap,
	sync::Arc,
};

use hyper::{
	header,
	Body,
	Error as HttpError,
	Request,
	Response,
	StatusCode,
};
use raiden_blockchain::contracts::{
	self,
	ContractsManager,
};
use raiden_primitives::{
	traits::Checksum,
	types::{
		Address,
		CanonicalIdentifier,
		TokenAddress,
		TokenAmount,
	},
};
use raiden_state_machine::{
	types::Event,
	views::{
		self,
		get_token_network_by_token_address,
	},
};
use routerify::ext::RequestExt;
use tracing::debug;

use super::{
	error::Error,
	request::{
		InitiatePaymentParams,
		MintTokenParams,
		UserDepositParams,
	},
	response::{
		ConnectionManager,
		ResponseEvent,
		ResponsePaymentSentSuccess,
		SettingsResponse,
	},
	utils::{
		api,
		body_to_params,
		contracts_manager,
		state_manager,
	},
};
use crate::{
	http::{
		request::{
			ChannelOpenParams,
			ChannelPatchParams,
		},
		response::{
			self,
			ChannelResponse,
			PaymentSuccess,
			ResponsePaymentHistory,
			ResponsePaymentReceivedSuccess,
			ResponsePaymentSentFailed,
			VersionResponse,
		},
		utils::{
			account,
			stop_sender,
			transfer_tasks_view,
		},
	},
	json_response,
	unwrap_option_or_error,
	unwrap_result_or_error,
};

pub async fn address(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let our_address = state_manager.read().current_state.our_address;

	let response = response::AddressResponse { our_address };

	json_response!(response, StatusCode::OK)
}

pub async fn version(_req: Request<Body>) -> Result<Response<Body>, HttpError> {
	const CARGO_PKG_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
	let response = VersionResponse { version: CARGO_PKG_VERSION };

	json_response!(response, StatusCode::OK)
}

pub async fn contracts(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let contracts_manager = contracts_manager(&req);

	let response = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	json_response!(response, StatusCode::OK)
}

pub async fn settings(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);

	let response =
		SettingsResponse { pathfinding_service_address: api.raiden.config.pfs_config.url.clone() };

	json_response!(response, StatusCode::OK)
}

pub async fn notifications(_req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let notifications: Vec<String> = vec![];
	json_response!(notifications, StatusCode::OK)
}

pub async fn pending_transfers(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address").and_then(|address| {
		match hex::decode(address.trim_start_matches("0x")) {
			Ok(address) => Some(Address::from_slice(&address)),
			Err(_) => None,
		}
	});

	let partner_address = req.param("partner_address").and_then(|address| {
		match hex::decode(address.trim_start_matches("0x")) {
			Ok(address) => Some(Address::from_slice(&address)),
			Err(_) => None,
		}
	});

	let chain_state = &state_manager.read().current_state;
	let all_tasks = &chain_state.payment_mapping.secrethashes_to_task;

	let channel_id = if token_address.is_some() {
		let token_address = token_address.unwrap();
		let token_network = views::get_token_network_by_token_address(
			chain_state,
			addresses.token_network_registry,
			token_address,
		);
		if token_network.is_none() {
			return unwrap_result_or_error!(
				Err(Error::Other(format!("Token {} not found", token_address))),
				StatusCode::NOT_FOUND
			)
		}
		let channel_id = if partner_address.is_some() {
			let partner_address = partner_address.unwrap();
			let partner_channel = views::get_channel_state_for(
				chain_state,
				addresses.token_network_registry,
				token_address,
				partner_address,
			);
			if partner_channel.is_none() {
				return unwrap_result_or_error!(
					Err(Error::Other(format!("Channel with partner was not found"))),
					StatusCode::NOT_FOUND
				)
			}
			Some(partner_channel.unwrap().canonical_identifier.channel_identifier)
		} else {
			None
		};
		channel_id
	} else {
		None
	};
	let view = transfer_tasks_view(all_tasks, token_address, channel_id);
	json_response!(view, StatusCode::OK)
}

pub async fn channels(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address");

	let chain_state = &state_manager.read().current_state;

	let channels: Vec<response::ChannelResponse> = if token_address.is_some() {
		let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
			&hex::decode(token_address.unwrap().trim_start_matches("0x"))
				.map_err(|_| Error::Other(format!("Invalid token address"))),
			StatusCode::BAD_REQUEST
		));
		let token_network = unwrap_result_or_error!(
			get_token_network_by_token_address(
				chain_state,
				addresses.token_network_registry,
				token_address
			)
			.ok_or(Error::Other(format!("Could not find token network by token address"))),
			StatusCode::BAD_REQUEST
		);

		token_network
			.channelidentifiers_to_channels
			.values()
			.map(|c| c.clone().into())
			.collect()
	} else {
		views::get_channels(chain_state).iter().map(|c| c.clone().into()).collect()
	};

	json_response!(&channels, StatusCode::OK)
}

pub async fn channel_by_partner_address(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address");
	let partner_address = req.param("partner_address");

	let chain_state = &state_manager.read().current_state;

	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));

	let partner_address: Address = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(partner_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid partner address"))),
		StatusCode::BAD_REQUEST
	));

	let channel_state = views::get_channel_state_for(
		chain_state,
		addresses.token_network_registry,
		token_address,
		partner_address,
	);

	if let Some(channel_state) = channel_state {
		let channel_state: ChannelResponse = channel_state.clone().into();
		json_response!(channel_state, StatusCode::OK)
	} else {
		unwrap_result_or_error!(
			Err(Error::Other(format!("Channel does not exist"))),
			StatusCode::NOT_FOUND
		)
	}
}
pub async fn connections_leave(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address");
	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));

	debug!(
		message = "Leaving token network",
		registry_address = addresses.token_network_registry.checksum(),
		token_address = token_address.checksum(),
	);

	let closed_channels = unwrap_result_or_error!(
		api.token_network_leave(addresses.token_network_registry, token_address)
			.await
			.map_err(|e| Error::Other(format!("Could not leave token network: {:?}", e))),
		StatusCode::BAD_REQUEST
	);

	let mut closed_channel_result = vec![];
	for channel_state in closed_channels {
		let result: ChannelResponse = channel_state.into();
		closed_channel_result.push(result);
	}

	json_response!(closed_channel_result, StatusCode::OK)
}

pub async fn connections_info(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let chain_state = &state_manager.read().current_state;

	let mut connection_managers = HashMap::new();
	for token in views::get_token_identifiers(chain_state, addresses.token_network_registry) {
		let open_channels =
			views::get_channelstate_open(chain_state, addresses.token_network_registry, token);
		connection_managers.insert(
			token.checksum(),
			ConnectionManager {
				sum_deposits: open_channels
					.iter()
					.map(|c| c.our_state.contract_balance)
					.fold(TokenAmount::zero(), |current, next| current.saturating_add(next)),
				channels: open_channels.len() as u32,
			},
		);
	}

	json_response!(connection_managers, StatusCode::OK)
}

pub async fn tokens(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses =
		unwrap_result_or_error!(contracts_manager.deployed_addresses(), StatusCode::BAD_REQUEST);

	let chain_state = &state_manager.read().current_state;

	let tokens: Vec<_> =
		views::get_token_identifiers(chain_state, addresses.token_network_registry)
			.iter()
			.map(|t| t.checksum())
			.collect();

	json_response!(tokens, StatusCode::OK)
}

pub async fn get_token_network_by_token(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address");
	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));

	let chain_state = &state_manager.read().current_state;

	let token_network_address = unwrap_option_or_error!(
		views::get_token_network_by_token_address(
			chain_state,
			addresses.token_network_registry,
			token_address,
		)
		.map(|t| t.address.checksum()),
		StatusCode::NOT_FOUND
	);

	json_response!(token_network_address, StatusCode::OK)
}

pub async fn register_token(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address");
	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));

	debug!(
		message = "Registering a new token",
		registry_address = addresses.token_network_registry.checksum(),
		token_address = token_address.checksum(),
	);

	let token_network_address = unwrap_result_or_error!(
		api.token_network_register(addresses.token_network_registry, token_address)
			.await
			.map_err(|e| Error::Other(format!("Could not register token network: {:?}", e))),
		StatusCode::CONFLICT
	);

	json_response!(token_network_address, StatusCode::CREATED)
}

pub async fn partners_by_token_address(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address");

	let chain_state = &state_manager.read().current_state;

	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));

	let token_network = unwrap_result_or_error!(
		get_token_network_by_token_address(
			chain_state,
			addresses.token_network_registry,
			token_address
		)
		.ok_or(Error::Other(format!("Could not find token network by token address"))),
		StatusCode::BAD_REQUEST
	);

	let channels: Vec<HashMap<&'static str, String>> = token_network
		.channelidentifiers_to_channels
		.values()
		.map(|c| {
			let mut map = HashMap::new();
			map.insert("partner_address", c.partner_state.address.to_string());
			map.insert(
				"channel",
				format!("/api/v1/channels/{}/{}", token_address.clone(), c.partner_state.address),
			);
			map
		})
		.collect();

	json_response!(&channels, StatusCode::OK)
}

pub async fn user_deposit(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let params: UserDepositParams =
		unwrap_result_or_error!(body_to_params(req).await, StatusCode::BAD_REQUEST);

	if params.total_deposit.is_some() && params.planned_withdraw_amount.is_some() {
		return unwrap_result_or_error!(
			Err(Error::Param(format!(
				"Cannot deposit to UDC and plan a withdraw at the same time"
			))),
			StatusCode::BAD_REQUEST
		)
	}
	if params.total_deposit.is_some() && params.withdraw_amount.is_some() {
		return unwrap_result_or_error!(
			Err(Error::Param(format!("Cannot deposit to UDC and withdraw at the same time"))),
			StatusCode::BAD_REQUEST
		)
	}
	if params.planned_withdraw_amount.is_some() && params.withdraw_amount.is_some() {
		return unwrap_result_or_error!(
			Err(Error::Param(format!(
				"Cannot withdraw from UDC and plan a withdraw at the same time"
			))),
			StatusCode::BAD_REQUEST
		)
	}
	if params.total_deposit.is_none() &&
		params.planned_withdraw_amount.is_none() &&
		params.withdraw_amount.is_none()
	{
		return unwrap_result_or_error!(Err(Error::Param(format!(
			"Nothing to do. Should either provide total_deposit, planned_withdraw_amount or withdraw_amount argument"
		))), StatusCode::BAD_REQUEST)
	}

	if let Some(total_deposit) = params.total_deposit {
		unwrap_result_or_error!(
			api.deposit_to_udc(addresses.user_deposit, total_deposit).await,
			StatusCode::CONFLICT
		);
	} else if let Some(planned_withdraw_amount) = params.planned_withdraw_amount {
		unwrap_result_or_error!(
			api.plan_withdraw_from_udc(addresses.user_deposit, planned_withdraw_amount)
				.await,
			StatusCode::CONFLICT
		);
	} else if let Some(withdraw_amount) = params.withdraw_amount {
		unwrap_result_or_error!(
			api.withdraw_from_udc(addresses.user_deposit, withdraw_amount).await,
			StatusCode::CONFLICT
		);
	} else {
		return unwrap_result_or_error!(Err(Error::Param(format!(
			"Nothing to do. Should either provide total_deposit, planned_withdraw_amount or withdraw_amount argument"
		))), StatusCode::CONFLICT);
	};
	json_response!((), StatusCode::OK)
}

pub async fn status(_req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let mut status = HashMap::new();
	status.insert("status", "ready");
	json_response!(status, StatusCode::OK)
}

pub async fn create_channel(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let account = account(&req);
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let params: ChannelOpenParams =
		unwrap_result_or_error!(body_to_params(req).await, StatusCode::BAD_REQUEST);

	let token_network_registry =
		params.registry_address.unwrap_or(addresses.token_network_registry);

	let channel_identifier = unwrap_result_or_error!(
		api.create_channel(
			account.clone(),
			token_network_registry,
			params.token_address,
			params.partner_address,
			params.settle_timeout,
			params.reveal_timeout,
			None,
		)
		.await,
		StatusCode::CONFLICT
	);

	if let Some(total_deposit) = params.total_deposit {
		if !total_deposit.is_zero() {
			unwrap_result_or_error!(
				api.update_channel(
					account,
					token_network_registry,
					params.token_address,
					params.partner_address,
					None,
					params.total_deposit,
					None,
					None,
					None,
				)
				.await,
				StatusCode::CONFLICT
			);
		}
	}

	let chain_state = &state_manager.read().current_state;
	let token_network = unwrap_option_or_error!(
		views::get_token_network_by_token_address(
			chain_state,
			token_network_registry,
			params.token_address
		),
		StatusCode::NOT_FOUND
	);
	if let Some(channel_state) = views::get_channel_by_canonical_identifier(
		chain_state,
		CanonicalIdentifier {
			chain_identifier: chain_state.chain_id,
			token_network_address: token_network.address,
			channel_identifier,
		},
	) {
		let channel_state: ChannelResponse = channel_state.clone().into();
		json_response!(channel_state, StatusCode::CREATED)
	} else {
		unwrap_result_or_error!(
			Err(Error::Other(format!("Channel is no longer found"))),
			StatusCode::CONFLICT
		)
	}
}

pub async fn channel_update(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let account = account(&req);
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let registry_address = addresses.token_network_registry;

	let token_address = req.param("token_address");
	let partner_address = req.param("partner_address");
	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));

	let partner_address: Address = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(partner_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid partner address"))),
		StatusCode::BAD_REQUEST
	));

	let params: ChannelPatchParams =
		unwrap_result_or_error!(body_to_params(req).await, StatusCode::BAD_REQUEST);

	unwrap_result_or_error!(
		api.update_channel(
			account,
			registry_address,
			token_address,
			partner_address,
			params.reveal_timeout,
			params.total_deposit,
			params.total_withdraw,
			params.state,
			None,
		)
		.await,
		StatusCode::CONFLICT
	);

	let chain_state = &state_manager.read().current_state.clone();
	let token_network = unwrap_result_or_error!(
		views::get_token_network_by_token_address(
			chain_state,
			addresses.token_network_registry,
			token_address,
		)
		.ok_or(Error::Other(format!("Token network not found"))),
		StatusCode::NOT_FOUND
	);

	if let Some(channel_state) = views::get_channel_by_token_network_and_partner(
		chain_state,
		token_network.address,
		partner_address,
	) {
		let channel_state: ChannelResponse = channel_state.clone().into();
		json_response!(channel_state, StatusCode::OK)
	} else {
		unwrap_result_or_error!(
			Err(Error::Other(format!("Channel is no longer found"))),
			StatusCode::CONFLICT
		)
	}
}

pub async fn payments(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap_result_or_error!(
		contracts_manager.deployed_addresses(),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let token_address = req.param("token_address");
	let partner_address = req.param("partner_address");

	let chain_state = &state_manager.read().current_state.clone();

	let token_network = if token_address.is_some() {
		let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
			&hex::decode(token_address.unwrap().trim_start_matches("0x"))
				.map_err(|_| Error::Other(format!("Invalid token address"))),
			StatusCode::BAD_REQUEST
		));

		let token_network = views::get_token_network_by_token_address(
			chain_state,
			addresses.token_network_registry,
			token_address,
		);
		if token_network.is_none() {
			unwrap_result_or_error!(
				Err(Error::Other(format!("Token address does not match a Raiden token network"))),
				StatusCode::NOT_FOUND
			);
		}
		token_network
	} else {
		None
	};

	let partner_address = if partner_address.is_some() {
		let partner_address: Address = Address::from_slice(unwrap_result_or_error!(
			&hex::decode(partner_address.unwrap().trim_start_matches("0x"))
				.map_err(|_| Error::Other(format!("Invalid partner address"))),
			StatusCode::BAD_REQUEST
		));
		Some(partner_address)
	} else {
		None
	};

	let token_network_address = token_network.map(|n| n.address);
	let events = unwrap_result_or_error!(
		state_manager
			.read()
			.storage
			.get_events_payment_history_with_timestamps(token_network_address, partner_address,)
			.map_err(|e| Error::Other(format!("{:?}", e))),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let mut payment_history: Vec<ResponsePaymentHistory> = vec![];
	for event_record in events {
		match event_record.data {
			Event::PaymentSentSuccess(e) => {
				let token_network =
					views::get_token_network_by_address(chain_state, e.token_network_address)
						.expect("Token network should exist");
				let mut result: ResponsePaymentSentSuccess = e.into();
				result.log_time = Some(event_record.timestamp);
				result.token_address = Some(token_network.token_address);
				result.identifier = Some(event_record.identifier.to_string());
				payment_history.push(ResponsePaymentHistory::SentSuccess(result))
			},
			Event::PaymentReceivedSuccess(e) => {
				let token_network =
					views::get_token_network_by_address(chain_state, e.token_network_address)
						.expect("Token network should exist");
				let mut result: ResponsePaymentReceivedSuccess = e.into();
				result.log_time = Some(event_record.timestamp);
				result.token_address = Some(token_network.token_address);
				result.identifier = Some(event_record.identifier.to_string());
				payment_history.push(ResponsePaymentHistory::ReceivedSuccess(result))
			},
			Event::ErrorPaymentSentFailed(e) => {
				let token_network =
					views::get_token_network_by_address(chain_state, e.token_network_address)
						.expect("Token network should exist");
				let mut result: ResponsePaymentSentFailed = e.into();
				result.log_time = Some(event_record.timestamp);
				result.token_address = Some(token_network.token_address);
				result.identifier = Some(event_record.identifier.to_string());
				payment_history.push(ResponsePaymentHistory::SentFailed(result))
			},
			_ => {
				unwrap_result_or_error!(
					Err(Error::Other(format!("Unexpected event"))),
					StatusCode::CONFLICT
				)
			},
		};
	}

	json_response!(payment_history, StatusCode::OK)
}

pub async fn mint_token(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);

	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(req.param("token_address").unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));

	let params: MintTokenParams =
		unwrap_result_or_error!(body_to_params(req).await, StatusCode::BAD_REQUEST);
	let transaction_hash = unwrap_result_or_error!(
		api.mint_token_for(token_address, params.to, params.value,).await,
		StatusCode::CONFLICT
	);

	json_response!(transaction_hash, StatusCode::OK)
}

pub async fn raiden_events(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);

	let events: Vec<ResponseEvent> = unwrap_result_or_error!(
		state_manager
			.read()
			.storage
			.get_events_with_timestamps()
			.map_err(|e| Error::Other(format!("{:?}", e))),
		StatusCode::INTERNAL_SERVER_ERROR
	)
	.into_iter()
	.map(|e| e.into())
	.collect();

	json_response!(events, StatusCode::OK)
}

pub async fn shutdown(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let stop_sender = stop_sender(&req);
	let _ = stop_sender.send(true).await;
	json_response!("", StatusCode::OK)
}

pub async fn initiate_payment(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let account = account(&req);
	let contracts_manager = contracts_manager(&req);

	let token_address = unwrap_result_or_error!(
		req.param("token_address").ok_or(Error::Uri("Missing token address")),
		StatusCode::BAD_REQUEST
	);
	let partner_address = unwrap_result_or_error!(
		req.param("partner_address").ok_or(Error::Uri("Missing partner address")),
		StatusCode::BAD_REQUEST
	);

	let token_address: TokenAddress = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(token_address.trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address"))),
		StatusCode::BAD_REQUEST
	));
	let partner_address: Address = Address::from_slice(unwrap_result_or_error!(
		&hex::decode(partner_address.trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid partner address"))),
		StatusCode::BAD_REQUEST
	));

	let params: InitiatePaymentParams =
		unwrap_result_or_error!(body_to_params(req).await, StatusCode::BAD_REQUEST);

	let default_token_network_registry = unwrap_result_or_error!(
		get_default_token_network_registry(contracts_manager.clone()),
		StatusCode::INTERNAL_SERVER_ERROR
	);
	let default_secret_registry = unwrap_result_or_error!(
		get_default_secret_registry(contracts_manager.clone()),
		StatusCode::INTERNAL_SERVER_ERROR
	);

	let payment = unwrap_result_or_error!(
		api.initiate_payment(
			account.clone(),
			default_token_network_registry,
			default_secret_registry,
			token_address,
			partner_address,
			params.amount,
			params.payment_identifier,
			params.secret,
			params.secret_hash,
			params.lock_timeout,
		)
		.await,
		StatusCode::CONFLICT
	);

	let result = PaymentSuccess {
		initiator_address: account.address(),
		registry_address: default_token_network_registry,
		token_address,
		target_address: partner_address,
		amount: params.amount,
		identifier: payment.payment_identifier,
		secret: hex::encode(payment.secret.0),
		secret_hash: hex::encode(payment.secrethash),
	};

	json_response!(
		unwrap_result_or_error!(serde_json::to_string(&result), StatusCode::INTERNAL_SERVER_ERROR),
		StatusCode::OK
	)
}

fn get_default_token_network_registry(
	contracts_manager: Arc<ContractsManager>,
) -> Result<Address, Error> {
	let token_network_registry_deployed_contract =
		match contracts_manager.get_deployed(contracts::ContractIdentifier::TokenNetworkRegistry) {
			Ok(contract) => contract,
			Err(e) =>
				return Err(Error::Other(format!(
					"Could not find token network registry deployment info: {:?}",
					e
				))),
		};
	Ok(token_network_registry_deployed_contract.address)
}

fn get_default_secret_registry(contracts_manager: Arc<ContractsManager>) -> Result<Address, Error> {
	let secret_registry_deployed_contract =
		match contracts_manager.get_deployed(contracts::ContractIdentifier::SecretRegistry) {
			Ok(contract) => contract,
			Err(e) =>
				return Err(Error::Other(format!(
					"Could not find secret registry deployment info: {:?}",
					e
				))),
		};
	Ok(secret_registry_deployed_contract.address)
}
