use std::{
	collections::{
		BTreeMap,
		HashMap,
	},
	fmt::Display,
	time::Duration,
};

use matrix_sdk::{
	config::{
		RequestConfig,
		SyncSettings,
	},
	ruma::{
		api::client::to_device::send_event_to_device,
		events::AnyToDeviceEvent,
		serde::Raw,
		to_device::DeviceIdOrAllDevices,
		OwnedUserId,
		TransactionId,
	},
	Client,
	Error,
};
use raiden_blockchain::{
	keys::PrivateKey,
	proxies::ServiceRegistryProxy,
};
use raiden_network_messages::messages::{
	IncomingMessage,
	OutgoingMessage,
};
use raiden_primitives::{
	traits::Stringify,
	types::{
		Address,
		AddressMetadata,
		BlockNumber,
	},
};
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use tracing::{
	debug,
	error,
};
use web3::{
	signing::Key,
	transports::Http,
};

use crate::TransportError;

/// The type of message handled by the client.
pub enum MessageType {
	Text,
	Notice,
}

impl Display for MessageType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let value = match self {
			MessageType::Text => "m.text".to_owned(),
			MessageType::Notice => "m.notice".to_owned(),
		};
		write!(f, "{}", value)
	}
}

/// The content of the message to be sent out / received.
#[derive(Serialize)]
pub struct MessageContent {
	pub msgtype: String,
	pub body: String,
}

/// Matrix client type.
pub struct MatrixClient {
	client: Client,
	private_key: PrivateKey,
	server_name: String,
	next_sync_token: String,
	services_addresses: HashMap<Address, BlockNumber>,
}

impl MatrixClient {
	/// Create a new instance of `MatrixClient`.
	pub async fn new(homeserver_url: String, private_key: PrivateKey) -> Self {
		let homeserver_url =
			Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
		let mut server_name =
			homeserver_url.host_str().expect("homeserver URL has no hostname").to_string();
		if let Some(port) = homeserver_url.port() {
			server_name = format!("{}:{}", server_name, port);
		}
		let client = Client::new(homeserver_url.clone()).await.unwrap();

		Self {
			client,
			private_key,
			server_name,
			next_sync_token: String::new(),
			services_addresses: HashMap::new(),
		}
	}

	/// Set sync token based on the last sync.
	pub fn set_sync_token(&mut self, sync_token: String) {
		self.next_sync_token = sync_token;
	}

	/// Return the last known sync token.
	pub fn get_sync_token(&self) -> String {
		self.next_sync_token.clone()
	}

	/// Return a copy of the private key.
	pub fn private_key(&self) -> PrivateKey {
		self.private_key.clone()
	}

	/// Initialize the client by logging into the matrix home server.
	pub async fn init(&self) -> Result<(), TransportError> {
		let username = format!("{:#x}", self.private_key.address());
		let signed_server_name =
			self.private_key.sign_message(self.server_name.as_bytes()).map_err(|e| {
				TransportError::Init(format!("Could not generate server password: {}", e))
			})?;

		let password = signed_server_name.as_string();
		let user_info = self
			.client
			.login_username(&username, &password)
			.device_id("RAIDEN")
			.send()
			.await
			.map_err(|e| TransportError::Init(format!("Error fetching matrix user info: {}", e)))?;

		let display_name = self
			.private_key
			.sign_message(user_info.user_id.as_bytes())
			.map_err(|e| TransportError::Init(format!("Error generating displayname: {}", e)))?;

		self.client
			.account()
			.set_display_name(Some(&display_name.as_string()))
			.await
			.map_err(|e| TransportError::Init(format!("Error setting displayname: {}", e)))?;

		Ok(())
	}

	/// Populate services addresses based on on-chain updates to the service registry.
	pub async fn populate_services_addresses(
		&mut self,
		service_registry_proxy: ServiceRegistryProxy<Http>,
	) {
		let services_len = match service_registry_proxy.ever_made_deposits_len(None).await {
			Ok(length) => length.as_u64(),
			Err(e) => {
				error!("Could not populate services addresses: {:?}", e);
				return
			},
		};
		for i in 0..services_len {
			if let Ok(address) = service_registry_proxy.ever_made_deposits(i, None).await {
				if let Ok(has_valid_registration) =
					service_registry_proxy.has_valid_registration(address, None).await
				{
					if !has_valid_registration {
						continue
					}

					if let Ok(validity) =
						service_registry_proxy.service_valid_til(address, None).await
					{
						self.services_addresses.insert(address, validity.as_u64().into());
					}
				}
			}
		}
	}

	/// Sync and retrieve any new messages received on the home server.
	pub async fn get_new_messages(&mut self) -> Result<Vec<IncomingMessage>, Error> {
		let mut sync_settings = SyncSettings::new().timeout(Duration::from_secs(30));
		if !self.next_sync_token.is_empty() {
			sync_settings = sync_settings.token(self.next_sync_token.clone());
		}
		let response = self.client.sync_once(sync_settings).await?;

		let to_device_events = response.to_device.events;
		debug!("Received {} network messages", to_device_events.len());

		let mut messages = vec![];
		for to_device_event in to_device_events.iter() {
			let message = match self.process_event(to_device_event).await {
				Ok(messages) => messages,
				Err(e) => {
					error!("Could not parse message: {:?}", e);
					continue
				},
			};
			messages.extend(message);
		}
		self.next_sync_token = response.next_batch;

		Ok(messages)
	}

	/// Construct the address metadata of the current account.
	pub fn address_metadata(&self) -> AddressMetadata {
		let user_id = self.make_user_id(&self.private_key.address());
		let displayname = self.private_key.sign_message(user_id.as_bytes()).unwrap().as_string();
		AddressMetadata { user_id, displayname, capabilities: "".to_owned() }
	}

	/// Return's the matrix user ID.
	pub fn make_user_id(&self, address: &Address) -> String {
		format!("@0x{}:{}", hex::encode(address), self.server_name)
	}

	/// Send message to the matrix home server.
	pub async fn send(
		&self,
		message: OutgoingMessage,
		receiver_metadata: AddressMetadata,
	) -> Result<(), TransportError> {
		let message_json = match serde_json::to_string(&message) {
			Ok(json) => json,
			Err(e) => {
				error!("Could not serialize message: {:?}", e);
				return Err(TransportError::Other(format!("{}", e)))
			},
		};
		let content = MessageContent { msgtype: MessageType::Text.to_string(), body: message_json };
		let json = match serde_json::to_string(&content) {
			Ok(json) => json,
			Err(e) => {
				error!("Could not serialize message: {:?}", e);
				return Err(TransportError::Other(format!("{}", e)))
			},
		};
		let data = match Raw::from_json_string(json) {
			Ok(d) => d,
			Err(e) => return Err(TransportError::Other(format!("{:?}", e))),
		};
		let user_id: OwnedUserId = receiver_metadata
			.user_id
			.as_str()
			.try_into()
			.map_err(|e| TransportError::Other(format!("{:?}", e)))?;
		let mut messages = BTreeMap::new();
		messages.insert(DeviceIdOrAllDevices::DeviceId("RAIDEN".into()), data);
		let mut destination = BTreeMap::new();
		destination.insert(user_id, messages);

		let transaction_id = TransactionId::new();
		let request = send_event_to_device::v3::Request::new_raw(
			"m.room.message",
			&transaction_id,
			destination,
		);
		self.client
			.send(request, Some(RequestConfig::default()))
			.await
			.map_err(TransportError::Send)?;

		Ok(())
	}

	/// Broadcast message to all known service addresses.
	pub async fn broadcast(
		&self,
		data: String,
		device_id: DeviceIdOrAllDevices,
	) -> Result<(), TransportError> {
		let user_ids: Vec<String> = self
			.services_addresses
			.keys()
			.map(|address| self.make_user_id(address))
			.collect();

		let data = match Raw::from_json_string(data) {
			Ok(d) => d,
			Err(e) => return Err(TransportError::Other(format!("{:?}", e))),
		};

		let mut messages = BTreeMap::new();
		messages.insert(device_id, data);
		for user_id in user_ids {
			let user_id: OwnedUserId = user_id
				.as_str()
				.try_into()
				.map_err(|e| TransportError::Other(format!("{:?}", e)))?;
			let mut destination = BTreeMap::new();
			destination.insert(user_id, messages.clone());

			let transaction_id = TransactionId::new();
			let request = send_event_to_device::v3::Request::new_raw(
				"m.room.message",
				&transaction_id,
				destination,
			);
			self.client
				.send(request, Some(RequestConfig::default()))
				.await
				.map_err(TransportError::Send)?;
		}

		Ok(())
	}

	/// Process sync events received from the matrix home server.
	async fn process_event(
		&self,
		event: &Raw<AnyToDeviceEvent>,
	) -> Result<Vec<IncomingMessage>, String> {
		let event_body = event.json().get();

		let content: Value = serde_json::from_str(event_body)
			.and_then(|map: HashMap<String, Value>| {
				map.get("content")
					.ok_or(serde::de::Error::custom("Could not find message content"))
					.cloned()
			})
			.and_then(|content| {
				content
					.get("body")
					.ok_or(serde::de::Error::custom("Could not find message body"))
					.cloned()
			})
			.map_err(|e| format!("{:?}", e))?;

		let mut messages = vec![];
		let s = content.as_str().unwrap().to_owned();
		for line in s.lines() {
			let message: IncomingMessage = match line.to_string().try_into() {
				Ok(message) => message,
				Err(e) => {
					error!("Could not decode message: {} {}", e, line);
					continue
				},
			};
			messages.push(message);
		}

		Ok(messages)
	}
}
