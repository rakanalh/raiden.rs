use std::time::Duration;

use matrix_sdk::{
    reqwest::Url,
    Client,
    LoopCtrl,
    SyncSettings,
};
use web3::signing::Key;

use crate::blockchain::key::{
    signature_to_str,
    PrivateKey,
};

use super::Transport;

pub struct MatrixTransport {
    client: Client,
    private_key: PrivateKey,
    server_name: String,
}

impl MatrixTransport {
    pub fn new(homeserver_url: String, private_key: PrivateKey) -> Self {
        let homeserver_url = Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
        let client = Client::new(homeserver_url.clone()).unwrap();
        Self {
            client,
            private_key,
            server_name: homeserver_url
                .host_str()
                .expect("homeserver URL has no hostname")
                .to_string(),
        }
    }

    async fn login(&self) -> Result<(), matrix_sdk::Error> {
        let username = self.private_key.address().to_string();
        let signed_server_name = self.private_key.sign(self.server_name.as_bytes(), None).unwrap();
        let password = signature_to_str(signed_server_name);
        self.client.login(&username, &password, None, Some("rust-sdk")).await?;
        self.client.sync(SyncSettings::new()).await;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for MatrixTransport {
    async fn init(&self) {
        let _ = self.login().await;
    }

    async fn sync(&self) {
        let sync_settings = SyncSettings::new().timeout(Duration::from_secs(30));
        self.client
            .sync_with_callback(sync_settings, |_response| async move { LoopCtrl::Continue })
            .await;
    }
}
