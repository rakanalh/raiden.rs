use std::{
	collections::BTreeMap,
	fmt::Display,
};

use matrix_sdk::{
	config::{
		RequestConfig,
		SyncSettings,
	},
	deserialized_responses::SyncResponse,
	ruma::{
		api::client::to_device::send_event_to_device,
		serde::Raw,
		to_device::DeviceIdOrAllDevices,
		OwnedUserId,
		TransactionId,
	},
	Client,
	Error,
};
use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	traits::Stringify,
	types::AddressMetadata,
};
use reqwest::Url;
use serde::Serialize;
use web3::signing::Key;

use crate::TransportError;

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

#[derive(Serialize)]
pub struct MessageContent {
	pub msgtype: String,
	pub body: String,
}

pub struct MatrixClient {
	client: Client,
	private_key: PrivateKey,
	server_name: String,
}

impl MatrixClient {
	pub async fn new(homeserver_url: String, private_key: PrivateKey) -> Self {
		let homeserver_url =
			Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
		let mut server_name =
			homeserver_url.host_str().expect("homeserver URL has no hostname").to_string();
		if let Some(port) = homeserver_url.port() {
			server_name = format!("{}:{}", server_name, port);
		}
		let client = Client::new(homeserver_url.clone()).await.unwrap();

		Self { client, private_key, server_name }
	}

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

	pub async fn sync_once(&self, settings: SyncSettings<'_>) -> Result<SyncResponse, Error> {
		self.client.sync_once(settings).await
	}

	pub fn address_metadata(&self) -> AddressMetadata {
		let user_id =
			format!("@0x{}:{}", hex::encode(self.private_key.address()), self.server_name);
		let displayname = self.private_key.sign_message(user_id.as_bytes()).unwrap().as_string();
		AddressMetadata { user_id, displayname, capabilities: "".to_owned() }
	}

	pub async fn send(
		&self,
		data: String,
		receiver_metadata: AddressMetadata,
	) -> Result<(), TransportError> {
		let data = match Raw::from_json_string(data) {
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
}
