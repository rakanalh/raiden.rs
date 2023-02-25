use std::collections::BTreeMap;

use matrix_sdk::{
	config::{
		RequestConfig,
		SyncSettings,
	},
	deserialized_responses::SyncResponse,
	ruma::{
		api::client::to_device::send_event_to_device,
		TransactionId,
	},
	Client,
	Error,
};
use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	traits::ToString,
	types::Address,
};
use raiden_state_machine::types::AddressMetadata;
use reqwest::Url;
use web3::signing::Key;

use crate::TransportError;

pub enum MessageType {
	Text,
	Notice,
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

		let password = signed_server_name.to_string();
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
			.set_display_name(Some(&display_name.to_string()))
			.await
			.map_err(|e| TransportError::Init(format!("Error setting displayname: {}", e)))?;

		Ok(())
	}

	pub async fn sync_once(&self, settings: SyncSettings<'_>) -> Result<SyncResponse, Error> {
		self.client.sync_once(settings).await
	}

	pub fn address_metadata(&self) -> AddressMetadata {
		let user_id = format!("@{}:{}", self.private_key.address().to_string(), self.server_name);
		let displayname = self.private_key.sign(user_id.as_bytes(), None).unwrap().to_string();
		AddressMetadata { user_id, displayname, capabilities: "".to_owned() }
	}

	pub async fn send(
		&self,
		_receiver_address: Address,
		_data: String,
		_message_type: MessageType,
		_receiver_metadata: AddressMetadata,
	) -> Result<(), TransportError> {
		let transaction_id = TransactionId::new();
		let request = send_event_to_device::v3::Request::new_raw(
			"m.room.message",
			&transaction_id,
			BTreeMap::new(),
		);
		self.client
			.send(request, Some(RequestConfig::default()))
			.await
			.map_err(TransportError::Send)?;

		Ok(())
	}
}
