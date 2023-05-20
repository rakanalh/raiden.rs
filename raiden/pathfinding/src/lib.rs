use std::collections::HashMap;

use chrono::Utc;
use derive_more::Display;
use raiden_primitives::{
	deserializers::u256_from_str,
	serializers::u256_to_str,
	traits::{
		Checksum,
		Stringify,
		ToBytes,
	},
	types::{
		Address,
		AddressMetadata,
		BlockExpiration,
		BlockNumber,
		Bytes,
		ChainID,
		OneToNAddress,
		TokenAmount,
		TokenNetworkAddress,
	},
};
use rand::prelude::SliceRandom;
use reqwest::Url;
use serde::{
	Deserialize,
	Serialize,
};
use thiserror::Error;
use tokio::{
	sync::Mutex,
	time::{
		self,
		Duration,
	},
};
use web3::{
	signing::{
		Key,
		SigningError,
	},
	transports::Http,
};

pub mod config;
pub mod routing;
pub mod types;

use raiden_blockchain::{
	keys::PrivateKey,
	proxies::{
		ProxyError,
		ServiceRegistryProxy,
	},
};
use tracing::{
	debug,
	info,
	trace,
};

use crate::{
	config::{
		PFSConfig,
		PFSInfo,
		ServicesConfig,
	},
	types::{
		RoutingMode,
		IOU,
	},
};

const MAX_PATHS_QUERY_ATTEMPT: usize = 2;

#[derive(Error, Display, Debug)]
pub enum RoutingError {
	#[display(fmt = "Token network does not exist")]
	TokenNetworkUnknown,
	#[display(fmt = "Pathfinding service could not be used")]
	PFServiceUnusable,
	#[display(fmt = "Pathfinding service error: {}", _0)]
	PFServiceRequestFailed(String),
	#[display(fmt = "Pathfinding service invalid response")]
	PFServiceInvalidResponse,
	#[display(fmt = "Could not find usable channels for this transfer")]
	NoUsableChannels,
	#[display(fmt = "Malformed matrix server for Pathfinding service")]
	MalformedMatrixUrl,
	#[display(fmt = "Failed to sign IOU: {:?}", _0)]
	Signing(SigningError),
	#[display(fmt = "Service registry error: {}", _0)]
	ServiceRegistry(ProxyError),
	#[display(fmt = "Invalid routing mode")]
	InvalidRoutingMode,
	#[display(fmt = "No valid pathfinding service provider found")]
	NoPathFindingServiceFound,
}

#[derive(Clone, Debug, Serialize)]
pub struct PFSRequest {
	from: String,
	to: String,
	#[serde(serialize_with = "u256_to_str")]
	value: TokenAmount,
	max_paths: usize,
	iou: Option<IOU>,
}

#[derive(Debug, Deserialize)]
pub struct PFSPath {
	pub path: Vec<Address>,
	pub address_metadata: HashMap<Address, AddressMetadata>,
	#[serde(deserialize_with = "u256_from_str")]
	pub estimated_fee: TokenAmount,
}

#[derive(Debug, Deserialize)]
pub struct PFSPathsResponse {
	feedback_token: String,
	result: Vec<PFSPath>,
}

#[derive(Debug, Deserialize)]
pub struct PFSErrorResponse {
	#[serde(rename = "errors")]
	msg: String,
}

#[derive(Debug, Deserialize)]
pub struct PFSLastIOUResponse {
	last_iou: IOU,
}

pub struct PFS {
	chain_id: ChainID,
	pfs_config: PFSConfig,
	private_key: PrivateKey,
	iou_creation: Mutex<()>,
}

impl PFS {
	pub fn new(chain_id: ChainID, pfs_config: PFSConfig, private_key: PrivateKey) -> Self {
		Self { chain_id, pfs_config, private_key, iou_creation: Mutex::new(()) }
	}

	pub async fn query_paths(
		&self,
		our_address: Address,
		token_network_address: TokenNetworkAddress,
		one_to_n_address: OneToNAddress,
		current_block_number: BlockNumber,
		route_from: Address,
		route_to: Address,
		value: TokenAmount,
		pfs_wait_for_block: BlockNumber,
	) -> Result<(Vec<PFSPath>, String), RoutingError> {
		let offered_fee = self.pfs_config.info.price;
		info!(
			message = "Query PFS for paths",
			route_from = route_from.checksum(),
			route_to = route_to.checksum(),
			offered_fee = offered_fee.to_string(),
			value = value.to_string(),
		);
		let mut payload = PFSRequest {
			from: route_from.checksum(),
			to: route_to.checksum(),
			max_paths: self.pfs_config.max_paths,
			iou: None,
			value,
		};

		let mut current_info = self.get_pfs_info().await?;
		while current_info.network.confirmed_block.number < pfs_wait_for_block {
			time::sleep(Duration::from_millis(500)).await;
			current_info = self.get_pfs_info().await?;
		}

		// Lock IOU creation until we have updated the current active IOU on the PFS
		let lock = self.iou_creation.lock().await;

		let scrap_existing_iou = false;
		for _retries in (0..MAX_PATHS_QUERY_ATTEMPT).rev() {
			if !offered_fee.is_zero() {
				let iou = self
					.create_current_iou(
						token_network_address,
						one_to_n_address,
						our_address,
						current_block_number,
						offered_fee,
						scrap_existing_iou,
					)
					.await?;
				payload.iou = Some(iou);
			}

			debug!(
				message = "Requesting PFS paths",
				token_network_address = token_network_address.checksum()
			);
			let response = self.post_pfs_paths(token_network_address, payload.clone()).await?;
			drop(lock);

			return Ok((response.result, response.feedback_token))
		}

		Ok((vec![], String::new()))
	}

	pub async fn get_pfs_info(&self) -> Result<PFSInfo, RoutingError> {
		get_pfs_info(self.pfs_config.url.clone()).await
	}

	pub async fn post_pfs_paths(
		&self,
		token_network_address: TokenNetworkAddress,
		payload: PFSRequest,
	) -> Result<PFSPathsResponse, RoutingError> {
		let client = reqwest::Client::new();
		let token_network_address = token_network_address.checksum();
		let response = client
			.post(format!("{}/api/v1/{}/paths", &self.pfs_config.url, token_network_address))
			.json(&payload)
			.send()
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e))
			})?;

		if response.status() == 200 {
			Ok(response.json().await.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
			})?)
		} else {
			let error_response: PFSErrorResponse = response.json().await.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
			})?;
			Err(RoutingError::PFServiceRequestFailed(format!("{}", error_response.msg)))
		}
	}

	pub async fn create_current_iou(
		&self,
		token_network_address: TokenNetworkAddress,
		one_to_n_address: OneToNAddress,
		our_address: Address,
		block_number: BlockNumber,
		offered_fee: TokenAmount,
		scrap_existing_iou: bool,
	) -> Result<IOU, RoutingError> {
		if scrap_existing_iou {
			trace!("Scrap existing IOU, create new...");
			return self.make_iou(our_address, one_to_n_address, block_number, offered_fee).await
		}

		let latest_iou = self.get_last_iou(token_network_address, our_address).await?;
		if let Some(latest_iou) = latest_iou {
			debug!(message = "Fetched last IOU", last_iou = latest_iou.to_string());
			self.update_iou(latest_iou, offered_fee, None).await
		} else {
			self.make_iou(our_address, one_to_n_address, block_number, offered_fee).await
		}
	}

	pub async fn get_last_iou(
		&self,
		token_network_address: TokenNetworkAddress,
		sender: Address,
	) -> Result<Option<IOU>, RoutingError> {
		let mut timestamp = Utc::now().naive_local().to_string();
		let timestamp: String = timestamp.drain(0..timestamp.len() - 2).collect();

		let signature = self
			.iou_signature_data(sender, self.pfs_config.info.payment_address, timestamp.clone())
			.map_err(RoutingError::Signing)?;

		let client = reqwest::Client::new();
		let response = client
			.request(
				reqwest::Method::GET,
				format!("{}/api/v1/{}/payment/iou", self.pfs_config.url, token_network_address),
			)
			.query(&[
				("sender", sender.checksum()),
				("receiver", self.pfs_config.info.payment_address.checksum()),
				("timestamp", timestamp.to_string()),
				("signature", signature.as_string()),
			])
			.send()
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e))
			})?;

		trace!(message = "PFS response", status = response.status().to_string());

		let response: PFSLastIOUResponse = if response.status() == 200 {
			response.json().await.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
			})?
		} else if response.status() == 404 {
			return Ok(None)
		} else {
			let error_response: PFSErrorResponse = response.json().await.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
			})?;
			return Err(RoutingError::PFServiceRequestFailed(format!("{}", error_response.msg)))
		};

		Ok(Some(IOU {
			sender: response.last_iou.sender,
			receiver: response.last_iou.receiver,
			one_to_n_address: response.last_iou.one_to_n_address,
			amount: response.last_iou.amount,
			expiration_block: response.last_iou.expiration_block,
			chain_id: response.last_iou.chain_id,
			signature: response.last_iou.signature,
		}))
	}

	pub async fn make_iou(
		&self,
		our_address: Address,
		one_to_n_address: OneToNAddress,
		block_number: BlockNumber,
		offered_fee: TokenAmount,
	) -> Result<IOU, RoutingError> {
		let expiration_block = block_number + self.pfs_config.iou_timeout.into();

		debug!(
			message = "Create IOU",
			receiver = self.pfs_config.info.payment_address.checksum(),
			amount = offered_fee.to_string(),
			expiration = expiration_block.to_string()
		);

		let mut iou = IOU {
			sender: our_address,
			receiver: self.pfs_config.info.payment_address,
			one_to_n_address,
			amount: offered_fee,
			expiration_block,
			chain_id: self.chain_id.clone(),
			signature: None,
		};
		iou.sign(self.private_key.clone()).map_err(RoutingError::Signing)?;
		Ok(iou)
	}

	pub async fn update_iou(
		&self,
		mut iou: IOU,
		added_amount: TokenAmount,
		expiration_block: Option<BlockExpiration>,
	) -> Result<IOU, RoutingError> {
		let old_amount = iou.amount;
		iou.amount = iou.amount + added_amount;
		if let Some(expiration_block) = expiration_block {
			iou.expiration_block = expiration_block;
		}
		debug!(
			message = "Update IOU",
			receiver = self.pfs_config.info.payment_address.checksum(),
			old_amount = old_amount.to_string(),
			new_amount = iou.amount.to_string(),
			expiration = iou.expiration_block.to_string()
		);
		iou.sign(self.private_key.clone()).map_err(RoutingError::Signing)?;
		Ok(iou)
	}

	fn iou_signature_data(
		&self,
		sender: Address,
		receiver: Address,
		timestamp: String,
	) -> Result<Bytes, SigningError> {
		let mut data = vec![];
		data.extend_from_slice(sender.as_bytes());
		data.extend_from_slice(receiver.as_bytes());
		data.extend_from_slice(timestamp.as_bytes());
		Ok(Bytes(self.private_key.sign_message(&data)?.to_bytes()))
	}
}

pub async fn query_address_metadata(
	url: String,
	address: Address,
) -> Result<AddressMetadata, RoutingError> {
	let response = reqwest::get(format!("{}/api/v1/address/{}/metadata", url, address.checksum()))
		.await
		.map_err(|e| RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e)))?;

	if response.status() == 200 {
		Ok(response.json().await.map_err(|e| {
			RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
		})?)
	} else {
		let error_response: PFSErrorResponse = response.json().await.map_err(|e| {
			RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
		})?;
		Err(RoutingError::PFServiceRequestFailed(format!("{}", error_response.msg)))
	}
}

pub async fn configure_pfs(
	services_config: ServicesConfig,
	service_registry: ServiceRegistryProxy<Http>,
) -> Result<PFSInfo, RoutingError> {
	if services_config.routing_mode != RoutingMode::PFS {
		return Err(RoutingError::InvalidRoutingMode)
	}

	let pfs_url = if services_config.pathfinding_service_random_address {
		get_random_pfs(service_registry, services_config.pathfinding_max_fee).await?
	} else {
		services_config.pathfinding_service_address
	};

	get_pfs_info(pfs_url).await
}

pub async fn get_random_pfs(
	service_registry: ServiceRegistryProxy<Http>,
	pathfinding_max_fee: TokenAmount,
) -> Result<String, RoutingError> {
	let number_of_addresses = service_registry
		.ever_made_deposits_len(None)
		.await
		.map_err(|e| RoutingError::ServiceRegistry(e))?;
	let mut indicies_to_try: Vec<u64> = (0..number_of_addresses.as_u64()).collect();
	indicies_to_try.shuffle(&mut rand::thread_rng());

	while let Some(index) = indicies_to_try.pop() {
		if let Ok(url) =
			get_valid_pfs_url(service_registry.clone(), index, pathfinding_max_fee).await
		{
			return Ok(url)
		}
	}
	Err(RoutingError::NoPathFindingServiceFound)
}

async fn get_valid_pfs_url(
	service_registry: ServiceRegistryProxy<Http>,
	index_in_service_registry: u64,
	pathfinding_max_fee: TokenAmount,
) -> Result<String, RoutingError> {
	let address = service_registry
		.ever_made_deposits(index_in_service_registry, None)
		.await
		.map_err(|e| RoutingError::ServiceRegistry(e))?;

	let has_valid_registration = !service_registry
		.has_valid_registration(address, None)
		.await
		.map_err(|e| RoutingError::ServiceRegistry(e))?;

	if !has_valid_registration {
		return Err(RoutingError::PFServiceUnusable)
	}

	let url = service_registry
		.get_service_url(address, None)
		.await
		.map_err(|e| RoutingError::ServiceRegistry(e))?;

	let pfs_info = get_pfs_info(url.clone()).await?;
	if pfs_info.price > pathfinding_max_fee {
		return Err(RoutingError::PFServiceUnusable)
	}

	Ok(url)
}

async fn get_pfs_info(url: String) -> Result<PFSInfo, RoutingError> {
	let infos: PFSInfo = reqwest::get(format!("{}/api/v1/info", &url))
		.await
		.map_err(|e| RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e)))?
		.json()
		.await
		.map_err(|e| {
			RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
		})?;

	let _ = Url::parse(&infos.matrix_server)
		.map_err(|_| RoutingError::MalformedMatrixUrl)?
		.host()
		.ok_or(RoutingError::MalformedMatrixUrl)?;

	Ok(infos)
}
