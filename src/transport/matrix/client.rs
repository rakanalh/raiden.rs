use std::collections::HashMap;

use matrix_sdk::{
    deserialized_responses::SyncResponse,
    Client,
    Error,
    SyncSettings,
};
use reqwest::Url;
use web3::signing::Key;

use crate::{
    primitives::{
        signature_to_str,
        AddressMetadata,
        PrivateKey,
    },
    transport::TransportError,
};

pub struct MatrixClient {
    client: Client,
    private_key: PrivateKey,
    server_name: String,
}

impl MatrixClient {
    pub fn new(homeserver_url: String, private_key: PrivateKey) -> Self {
        let homeserver_url = Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
        let server_name = homeserver_url
            .host_str()
            .expect("homeserver URL has no hostname")
            .to_string();
        let client = Client::new(homeserver_url.clone()).unwrap();

        Self {
            client,
            private_key,
            server_name,
        }
    }

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
}
