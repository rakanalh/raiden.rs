use std::{
	collections::HashMap,
	str::FromStr,
};

use chrono::{
	DateTime,
	Utc,
};
use derive_more::Display;
use raiden_primitives::{
	traits::ToChecksummed,
	types::{
		Address,
		AddressMetadata,
		BlockExpiration,
		BlockNumber,
		ChainID,
		OneToNAddress,
		TokenAmount,
		TokenNetworkAddress,
		TokenNetworkRegistryAddress,
		H256,
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
pub mod iou;
pub mod routing;
pub mod types;

use raiden_blockchain::{
	keys::PrivateKey,
	proxies::{
		ProxyError,
		ServiceRegistryProxy,
	},
	signature::SignatureUtils,
};

use crate::{
	config::{
		PFSConfig,
		PFSInfo,
		ServicesConfig,
	},
	iou::IOU,
	types::RoutingMode,
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
	#[display(fmt = "Failed to sign IOU")]
	Signing(SigningError),
	#[display(fmt = "Service registry error: {}", _0)]
	ServiceRegistry(ProxyError),
	#[display(fmt = "Invalid routing mode")]
	InvalidRoutingMode,
	#[display(fmt = "No valid pathfinding service provider found")]
	NoPathFindingServiceFound,
}

#[derive(Clone, Serialize)]
pub struct PFSRequest {
	from: String,
	to: String,
	value: TokenAmount,
	max_paths: usize,
	iou: Option<IOU>,
}

#[derive(Deserialize)]
pub struct PFSNetworkInfo {
	chain_id: ChainID,
	token_network_registry_address: TokenNetworkRegistryAddress,
	user_deposit_address: Address,
	confirmed_block_number: BlockNumber,
}

#[derive(Deserialize)]
pub struct PFSPath {
	pub nodes: Vec<Address>,
	pub address_metadata: HashMap<Address, AddressMetadata>,
	pub estimated_fee: TokenAmount,
}

#[derive(Deserialize)]
pub struct PFSPathsResponse {
	feedback_token: String,
	result: Vec<PFSPath>,
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
		let mut payload = PFSRequest {
			from: route_from.to_checksummed(),
			to: route_to.to_checksummed(),
			max_paths: self.pfs_config.max_paths,
			iou: None,
			value,
		};
		let offered_fee = self.pfs_config.info.price;

		let mut current_info = self.get_pfs_info().await?;
		while current_info.network.confirmed_block.number < pfs_wait_for_block {
			time::sleep(Duration::from_millis(500)).await;
			current_info = self.get_pfs_info().await?;
		}

		let scrap_existing_iou = false;
		for _retries in (0..MAX_PATHS_QUERY_ATTEMPT).rev() {
			let lock = self.iou_creation.lock().await;
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
		let token_network_address = token_network_address.to_checksummed();
		let response: PFSPathsResponse = client
			.post(format!("{}/api/v1/{}/paths", &self.pfs_config.url, token_network_address))
			.json(&payload)
			.send()
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e))
			})?
			.json()
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
			})?;

		Ok(response)
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
			return self.make_iou(our_address, one_to_n_address, block_number, offered_fee).await
		}

		let latest_iou = self.get_last_iou(token_network_address, our_address).await?;

		self.update_iou(latest_iou, offered_fee, None).await
	}

	pub async fn get_last_iou(
		&self,
		token_network_address: TokenNetworkAddress,
		sender: Address,
	) -> Result<IOU, RoutingError> {
		let timestamp = Utc::now();
		let signature = self
			.iou_signature_data(sender, self.pfs_config.info.payment_address, timestamp)
			.map_err(RoutingError::Signing)?;

		let client = reqwest::Client::new();
		let response = client
			.request(
				reqwest::Method::GET,
				format!("{}/api/v1/{}/payment/iou", self.pfs_config.url, token_network_address),
			)
			.query(&[
				("sender", sender.to_string()),
				("receiver", self.pfs_config.info.payment_address.to_string()),
				("timestamp", timestamp.to_string()),
				("signature", signature.to_string()),
			])
			.send()
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e))
			})?
			.json::<HashMap<String, String>>()
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
			})?;

		let sender = Address::from_slice(
			response.get("sender").ok_or(RoutingError::PFServiceInvalidResponse)?.as_bytes(),
		);
		let receiver = Address::from_slice(
			response
				.get("receiver")
				.ok_or(RoutingError::PFServiceInvalidResponse)?
				.as_bytes(),
		);
		let one_to_n_address = Address::from_slice(
			response
				.get("one_to_n_address")
				.ok_or(RoutingError::PFServiceInvalidResponse)?
				.as_bytes(),
		);
		let amount = TokenAmount::from_dec_str(
			&response.get("amount").ok_or(RoutingError::PFServiceInvalidResponse)?,
		)
		.map_err(|_| RoutingError::PFServiceInvalidResponse)?;
		let expiration_block = BlockNumber::from_str(
			response
				.get("expiration_block")
				.ok_or(RoutingError::PFServiceInvalidResponse)?
				.as_str(),
		)
		.map_err(|_| RoutingError::PFServiceInvalidResponse)?;
		let chain_id = ChainID::from_str(
			response.get("chain_id").ok_or(RoutingError::PFServiceInvalidResponse)?.as_str(),
		)
		.map_err(|_| RoutingError::PFServiceInvalidResponse)?;
		let signature = H256::from_slice(
			response
				.get("signature")
				.ok_or(RoutingError::PFServiceInvalidResponse)?
				.as_bytes(),
		);

		Ok(IOU {
			sender,
			receiver,
			one_to_n_address,
			amount,
			expiration_block,
			chain_id,
			signature: Some(signature),
		})
	}

	pub async fn make_iou(
		&self,
		our_address: Address,
		one_to_n_address: OneToNAddress,
		block_number: BlockNumber,
		offered_fee: TokenAmount,
	) -> Result<IOU, RoutingError> {
		let expiration_block = block_number + self.pfs_config.iou_timeout.into();

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
		iou.amount = iou.amount + added_amount;
		if let Some(expiration_block) = expiration_block {
			iou.expiration_block = expiration_block;
		}
		iou.sign(self.private_key.clone()).map_err(RoutingError::Signing)?;
		Ok(iou)
	}

	fn iou_signature_data(
		&self,
		sender: Address,
		receiver: Address,
		timestamp: DateTime<Utc>,
	) -> Result<H256, SigningError> {
		let timestamp = format!("{}", timestamp.format("%+"));
		let chain_id = self.chain_id.clone().into();

		let mut data = vec![];
		data.extend_from_slice(sender.as_bytes());
		data.extend_from_slice(receiver.as_bytes());
		data.extend_from_slice(timestamp.as_bytes());
		Ok(self.private_key.sign(&data, Some(chain_id))?.to_h256())
	}
}

pub async fn query_address_metadata(
	url: String,
	address: Address,
) -> Result<AddressMetadata, RoutingError> {
	let metadata =
		reqwest::get(format!("{}/api/v1/address/{}/metadata", url, address.to_checksummed()))
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e))
			})?
			.json::<AddressMetadata>()
			.await
			.map_err(|e| {
				RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e))
			})?;

	Ok(metadata)
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
	let mut indicies_to_try: Vec<u32> = (0..number_of_addresses.as_u32()).collect();
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
	index_in_service_registry: u32,
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
