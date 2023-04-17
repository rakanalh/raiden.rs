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
	traits::ToChecksummed,
	types::{
		Address,
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
		UserDepositParams,
	},
	response::{
		ConnectionManager,
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
			ResponsePaymentHistory,
			ResponsePaymentReceivedSuccess,
			ResponsePaymentSentFailed,
			VersionResponse,
		},
		utils::{
			account,
			transfer_tasks_view,
		},
	},
	json_response,
	unwrap,
};

pub async fn address(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let our_address = state_manager.read().current_state.our_address;

	let response = response::AddressResponse { our_address };

	json_response!(response)
}

pub async fn version(_req: Request<Body>) -> Result<Response<Body>, HttpError> {
	const CARGO_PKG_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
	let response = VersionResponse { version: CARGO_PKG_VERSION };

	json_response!(response)
}

pub async fn contracts(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let contracts_manager = contracts_manager(&req);

	let response = unwrap!(contracts_manager.deployed_addresses());

	json_response!(response)
}

pub async fn settings(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);

	let response =
		SettingsResponse { pathfinding_service_address: api.raiden.config.pfs_config.url.clone() };

	json_response!(response)
}

pub async fn notifications(_req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let notifications: Vec<String> = vec![];
	json_response!(notifications)
}

pub async fn pending_transfers(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let token_address = req
		.param("token_address")
		.map(|address| match hex::decode(address.trim_start_matches("0x")) {
			Ok(address) => Some(Address::from_slice(&address)),
			Err(_) => None,
		})
		.flatten();

	let partner_address = req
		.param("partner_address")
		.map(|address| match hex::decode(address.trim_start_matches("0x")) {
			Ok(address) => Some(Address::from_slice(&address)),
			Err(_) => None,
		})
		.flatten();

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
			return unwrap!(Err(Error::Other(format!("Token {} not found", token_address))))
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
				return unwrap!(Err(Error::Other(format!("Channel with partner was not found"))))
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
	json_response!(view)
}

pub async fn channels(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let token_address = req.param("token_address");

	let chain_state = &state_manager.read().current_state;

	let channels: Vec<response::ChannelResponse> = if token_address.is_some() {
		let token_address: TokenAddress = Address::from_slice(unwrap!(&hex::decode(
			token_address.unwrap().trim_start_matches("0x")
		)
		.map_err(|_| Error::Other(format!("Invalid token address")))));
		let token_network = unwrap!(get_token_network_by_token_address(
			&chain_state,
			addresses.token_network_registry,
			token_address
		)
		.ok_or(Error::Other(format!("Could not find token network by token address"))));

		token_network
			.channelidentifiers_to_channels
			.values()
			.map(|c| c.clone().into())
			.collect()
	} else {
		views::get_channels(&chain_state).iter().map(|c| c.clone().into()).collect()
	};

	json_response!(&channels)
}

pub async fn channel_by_partner_address(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let token_address = req.param("token_address");
	let partner_address = req.param("partner_address");

	let chain_state = &state_manager.read().current_state;

	let token_address: TokenAddress =
		Address::from_slice(unwrap!(&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address")))));

	let partner_address: Address = Address::from_slice(unwrap!(&hex::decode(
		partner_address.unwrap().trim_start_matches("0x")
	)
	.map_err(|_| Error::Other(format!("Invalid partner address")))));

	let channel_state = views::get_channel_state_for(
		&chain_state,
		addresses.token_network_registry,
		token_address,
		partner_address,
	);

	if let Some(channel_state) = channel_state {
		let channel_state: ChannelResponse = channel_state.clone().into();
		json_response!(channel_state)
	} else {
		unwrap!(Err(Error::Other(format!("Channel does not exist"))))
	}
}
pub async fn connections_leave(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let token_address = req.param("token_address");
	let token_address: TokenAddress =
		Address::from_slice(unwrap!(&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address")))));

	debug!(
		message = "Leaving token network",
		registry_address = addresses.token_network_registry.to_checksummed(),
		token_address = token_address.to_checksummed(),
	);

	let closed_channels = unwrap!(api
		.token_network_leave(addresses.token_network_registry, token_address)
		.await
		.map_err(|e| Error::Other(format!("Could not leave token network: {:?}", e))));

	let mut closed_channel_result = vec![];
	for channel_state in closed_channels {
		let result: ChannelResponse = channel_state.into();
		closed_channel_result.push(result);
	}

	json_response!(closed_channel_result)
}

pub async fn connections_info(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let chain_state = &state_manager.read().current_state;

	let mut connection_managers = HashMap::new();
	for token in views::get_token_identifiers(chain_state, addresses.token_network_registry) {
		let open_channels =
			views::get_channelstate_open(chain_state, addresses.token_network_registry, token);
		connection_managers.insert(
			token.to_checksummed(),
			ConnectionManager {
				sum_deposits: open_channels
					.iter()
					.map(|c| c.our_state.contract_balance)
					.fold(TokenAmount::zero(), |current, next| current.saturating_add(next)),
				channels: open_channels.len() as u32,
			},
		);
	}

	json_response!(connection_managers)
}

pub async fn tokens(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let chain_state = &state_manager.read().current_state;

	let tokens: Vec<_> =
		views::get_token_identifiers(chain_state, addresses.token_network_registry)
			.iter()
			.map(|t| t.to_checksummed())
			.collect();

	json_response!(tokens)
}

pub async fn register_token(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let token_address = req.param("token_address");
	let token_address: TokenAddress =
		Address::from_slice(unwrap!(&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address")))));

	debug!(
		message = "Registering a new token",
		registry_address = addresses.token_network_registry.to_checksummed(),
		token_address = token_address.to_checksummed(),
	);

	let token_network_address = unwrap!(api
		.token_network_register(addresses.token_network_registry, token_address)
		.await
		.map_err(|e| Error::Other(format!("Could not register token network: {:?}", e))));

	json_response!(token_network_address)
}

pub async fn partners_by_token_address(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let token_address = req.param("token_address");

	let chain_state = &state_manager.read().current_state;

	let token_address: TokenAddress =
		Address::from_slice(unwrap!(&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address")))));

	let token_network = unwrap!(get_token_network_by_token_address(
		&chain_state,
		addresses.token_network_registry,
		token_address
	)
	.ok_or(Error::Other(format!("Could not find token network by token address"))));

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

	json_response!(&channels)
}

pub async fn user_deposit(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let params: UserDepositParams = unwrap!(body_to_params(req).await);

	if params.total_deposit.is_some() && params.planned_withdraw_amount.is_some() {
		return unwrap!(Err(Error::Param(format!(
			"Cannot deposit to UDC and plan a withdraw at the same time"
		))))
	}
	if params.total_deposit.is_some() && params.withdraw_amount.is_some() {
		return unwrap!(Err(Error::Param(format!(
			"Cannot deposit to UDC and withdraw at the same time"
		))))
	}
	if params.planned_withdraw_amount.is_some() && params.withdraw_amount.is_some() {
		return unwrap!(Err(Error::Param(format!(
			"Cannot withdraw from UDC and plan a withdraw at the same time"
		))))
	}
	if params.total_deposit.is_none() &&
		params.planned_withdraw_amount.is_none() &&
		params.withdraw_amount.is_none()
	{
		return unwrap!(Err(Error::Param(format!(
			"Nothing to do. Should either provide total_deposit, planned_withdraw_amount or withdraw_amount argument"
		))))
	}

	let result = if let Some(total_deposit) = params.total_deposit {
		unwrap!(api.deposit_to_udc(addresses.user_deposit, total_deposit).await);
	} else if let Some(planned_withdraw_amount) = params.planned_withdraw_amount {
		unwrap!(
			api.plan_withdraw_from_udc(addresses.user_deposit, planned_withdraw_amount)
				.await
		);
	} else if let Some(withdraw_amount) = params.withdraw_amount {
		unwrap!(api.withdraw_from_udc(addresses.user_deposit, withdraw_amount).await);
	} else {
		return unwrap!(Err(Error::Param(format!(
			"Nothing to do. Should either provide total_deposit, planned_withdraw_amount or withdraw_amount argument"
		))));
	};
	json_response!(result)
}

pub async fn status(_req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let mut status = HashMap::new();
	status.insert("status", "ready");
	json_response!(status)
}

pub async fn create_channel(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let account = account(&req);

	let params: ChannelOpenParams = unwrap!(body_to_params(req).await);

	let channel_identifier = unwrap!(
		api.create_channel(
			account.clone(),
			params.registry_address,
			params.token_address,
			params.partner_address,
			params.settle_timeout,
			params.reveal_timeout,
			None,
		)
		.await
	);

	if params.total_deposit.is_some() {
		unwrap!(
			api.update_channel(
				account,
				params.registry_address,
				params.token_address,
				params.partner_address,
				None,
				params.total_deposit,
				None,
				None,
				None,
			)
			.await
		);
	}

	let mut data = HashMap::new();
	data.insert("channel_identifier".to_owned(), channel_identifier);
	json_response!(data)
}

pub async fn channel_update(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let account = account(&req);
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let registry_address = addresses.token_network_registry;

	let token_address = req.param("token_address");
	let partner_address = req.param("partner_address");
	let token_address: TokenAddress =
		Address::from_slice(unwrap!(&hex::decode(token_address.unwrap().trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address")))));

	let partner_address: Address = Address::from_slice(unwrap!(&hex::decode(
		partner_address.unwrap().trim_start_matches("0x")
	)
	.map_err(|_| Error::Other(format!("Invalid partner address")))));

	let params: ChannelPatchParams = unwrap!(body_to_params(req).await);

	unwrap!(
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
		.await
	);

	let chain_state = &state_manager.read().current_state.clone();
	let token_network = unwrap!(views::get_token_network_by_token_address(
		chain_state,
		addresses.token_network_registry,
		token_address,
	)
	.ok_or(Error::Other(format!("Token network not found"))));

	if let Some(channel_state) = views::get_channel_by_token_network_and_partner(
		chain_state,
		token_network.address,
		partner_address,
	) {
		let channel_state: ChannelResponse = channel_state.clone().into();
		json_response!(channel_state)
	} else {
		unwrap!(Err(Error::Other(format!("Channel is no longer found"))))
	}
}

pub async fn payments(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let state_manager = state_manager(&req);
	let contracts_manager = contracts_manager(&req);
	let addresses = unwrap!(contracts_manager.deployed_addresses());

	let token_address = req.param("token_address");
	let partner_address = req.param("partner_address");

	let chain_state = &state_manager.read().current_state.clone();

	let token_network = if token_address.is_some() {
		let token_address: TokenAddress = Address::from_slice(unwrap!(&hex::decode(
			token_address.unwrap().trim_start_matches("0x")
		)
		.map_err(|_| Error::Other(format!("Invalid token address")))));

		let token_network = views::get_token_network_by_token_address(
			chain_state,
			addresses.token_network_registry,
			token_address,
		);
		if token_network.is_none() {
			unwrap!(Err(Error::Other(format!(
				"Token address does not match a Raiden token network"
			))));
		}
		token_network
	} else {
		None
	};

	let partner_address = if partner_address.is_some() {
		let partner_address: Address = Address::from_slice(unwrap!(&hex::decode(
			partner_address.unwrap().trim_start_matches("0x")
		)
		.map_err(|_| Error::Other(format!("Invalid partner address")))));
		Some(partner_address)
	} else {
		None
	};

	let token_network_address = token_network.map(|n| n.address);
	let events = unwrap!(state_manager
		.read()
		.storage
		.get_events_payment_history_with_timestamps(token_network_address, partner_address,)
		.map_err(|e| Error::Other(format!("{:?}", e))));

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
				unwrap!(Err(Error::Other(format!("Unexpected event"))))
			},
		};
	}

	json_response!(payment_history)
}

pub async fn initiate_payment(req: Request<Body>) -> Result<Response<Body>, HttpError> {
	let api = api(&req);
	let account = account(&req);
	let contracts_manager = contracts_manager(&req);
	// let state_manager = state_manager(&req);

	let token_address =
		unwrap!(req.param("token_address").ok_or(Error::Uri("Missing token address")));
	let partner_address =
		unwrap!(req.param("partner_address").ok_or(Error::Uri("Missing partner address")));

	let token_address: TokenAddress =
		Address::from_slice(unwrap!(&hex::decode(token_address.trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid token address")))));
	let partner_address: Address =
		Address::from_slice(unwrap!(&hex::decode(partner_address.trim_start_matches("0x"))
			.map_err(|_| Error::Other(format!("Invalid partner address")))));

	let params: InitiatePaymentParams = unwrap!(body_to_params(req).await);

	let default_token_network_registry =
		unwrap!(get_default_token_network_registry(contracts_manager.clone()));
	let default_secret_registry = unwrap!(get_default_secret_registry(contracts_manager.clone()));

	let payment = unwrap!(
		api.initiate_payment(
			account,
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
		.await
	);

	json_response!(unwrap!(serde_json::to_string(&payment)))
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
