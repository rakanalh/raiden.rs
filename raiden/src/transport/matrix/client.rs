use std::collections::{
    BTreeMap,
    HashMap,
};

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
use reqwest::Url;
use web3::{
    signing::Key,
    types::Address,
};

use crate::{
    primitives::{
        signature_to_str,
        AddressMetadata,
        PrivateKey,
    },
    transport::TransportError,
};

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
        let homeserver_url = Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
        let server_name = homeserver_url
            .host_str()
            .expect("homeserver URL has no hostname")
            .to_string();
        let client = Client::new(homeserver_url.clone()).await.unwrap();

        Self {
            client,
            private_key,
            server_name,
        }
    }

    #[allow(unused)]
    async fn init(&self) -> Result<(), TransportError> {
        let username = self.private_key.address().to_string();
        let signed_server_name = self.private_key.sign(self.server_name.as_bytes(), None).unwrap();
        let password = signature_to_str(signed_server_name);
        self.client
            .login(&username, &password, None, Some("RIR"))
            .await
            .map_err(|e| TransportError::Init(format!("{}", e)))?;
        self.client.sync(SyncSettings::new()).await;
        Ok(())
    }

    pub async fn sync_once(&self, settings: SyncSettings<'_>) -> Result<SyncResponse, Error> {
        self.client.sync_once(settings).await
    }

    pub fn address_metadata(&self) -> AddressMetadata {
        let user_id = format!("@{}:{}", self.private_key.address().to_string(), self.server_name);
        let displayname = signature_to_str(self.private_key.sign(user_id.as_bytes(), None).unwrap());
        AddressMetadata {
            user_id,
            displayname,
            capabilities: HashMap::new(),
        }
    }

    pub async fn send(
        &self,
        receiver_address: Address,
        data: String,
        message_type: MessageType,
        receiver_metadata: AddressMetadata,
    ) -> Result<(), TransportError> {
        let transaction_id = TransactionId::new();
        let request = send_event_to_device::v3::Request::new_raw("m.room.message", &transaction_id, BTreeMap::new());
        self.client
            .send(request, Some(RequestConfig::default()))
            .await
            .map_err(TransportError::Send)?;

        Ok(())
    }
}
