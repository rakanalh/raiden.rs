use std::{
	collections::HashMap,
	sync::Arc,
};

use hyper::{
	Body,
	Request,
};
use parking_lot::RwLock;
use raiden_api::api::Api;
use raiden_blockchain::{
	contracts::ContractsManager,
	proxies::Account,
};
use raiden_primitives::types::{
	ChannelIdentifier,
	SecretHash,
	TokenAddress,
};
use raiden_state_machine::types::TransferTask;
use raiden_transition::manager::StateManager;
use routerify::ext::RequestExt;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::Sender;
use web3::transports::Http;

use super::{
	error::Error,
	response::TransferView,
};

pub(crate) fn account(req: &Request<Body>) -> Account<Http> {
	req.data::<Account<Http>>().unwrap().clone()
}

pub(crate) fn api(req: &Request<Body>) -> Arc<Api> {
	req.data::<Arc<Api>>().unwrap().clone()
}

pub(crate) fn state_manager(req: &Request<Body>) -> Arc<RwLock<StateManager>> {
	req.data::<Arc<RwLock<StateManager>>>().unwrap().clone()
}

pub(crate) fn contracts_manager(req: &Request<Body>) -> Arc<ContractsManager> {
	req.data::<Arc<ContractsManager>>().unwrap().clone()
}

pub(crate) fn stop_sender(req: &Request<Body>) -> Sender<bool> {
	req.data::<Sender<bool>>().unwrap().clone()
}

pub(crate) async fn body_to_params<T: DeserializeOwned>(req: Request<Body>) -> Result<T, Error> {
	let body = hyper::body::to_bytes(req.into_body()).await.map_err(Error::Http)?;
	let params: T = serde_json::from_slice(&body).map_err(Error::Serialization)?;
	Ok(params)
}

pub(crate) fn transfer_tasks_view(
	tasks: &HashMap<SecretHash, TransferTask>,
	token_address: Option<TokenAddress>,
	channel_id: Option<ChannelIdentifier>,
) -> Vec<TransferView> {
	let mut view = vec![];
	for (secrethash, transfer_task) in tasks {
		let (role, transfer) = match transfer_task {
			TransferTask::Initiator(inner) => (
				"initiator",
				inner
					.manager_state
					.initiator_transfers
					.get(secrethash)
					.map(|t| t.transfer.clone()),
			),
			TransferTask::Mediator(inner) => (
				"mediator",
				inner.mediator_state.transfers_pair.last().map(|t| t.payer_transfer.clone()),
			),
			TransferTask::Target(inner) => ("target", Some(inner.target_state.transfer.clone())),
		};

		let transfer = match transfer {
			Some(transfer) => transfer,
			None => continue,
		};
		if let Some(token_address) = token_address {
			if transfer.token != token_address {
				continue
			} else if let Some(channel_id) = channel_id {
				if transfer.balance_proof.canonical_identifier.channel_identifier != channel_id {
					continue
				}
			}
		}

		let transfer_view = TransferView {
			payment_identifier: transfer.payment_identifier,
			token_address: transfer.token,
			token_network_address: transfer
				.balance_proof
				.canonical_identifier
				.token_network_address,
			channel_identifier: transfer.balance_proof.canonical_identifier.channel_identifier,
			initiator: transfer.initiator,
			target: transfer.target,
			transferred_amount: transfer.balance_proof.transferred_amount,
			locked_amount: transfer.balance_proof.locked_amount,
			role: role.to_string(),
		};
		view.push(transfer_view)
	}

	view
}
