use chrono::{
    DateTime,
    Utc,
};
use derive_more::Display;
use reqwest::Url;
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    collections::HashMap,
    str::FromStr,
};
use thiserror::Error;
use tokio::sync::Mutex;
use web3::{
    signing::{
        Key,
        SigningError,
    },
    types::{
        Address,
        H256,
    },
};

use crate::primitives::{
    signature::SignatureUtils,
    AddressMetadata,
    BlockExpiration,
    BlockNumber,
    ChainID,
    PFSConfig,
    PFSInfo,
    PrivateKey,
    TokenAmount,
    IOU,
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
}

#[derive(Clone, Serialize)]
pub struct PFSRequest {
    from: Address,
    to: Address,
    value: TokenAmount,
    max_paths: usize,
    iou: Option<IOU>,
}

#[derive(Deserialize)]
pub struct PFSNetworkInfo {
    chain_id: ChainID,
    token_network_registry_address: Address,
    user_deposit_address: Address,
    confirmed_block_number: BlockNumber,
}

#[derive(Deserialize)]
pub struct PFSInfoResponse {
    price_info: TokenAmount,
    network_info: PFSNetworkInfo,
    payment_address: Address,
    message: String,
    operator: String,
    version: String,
    matrix_server: String,
}

#[derive(Deserialize)]
pub struct PFSPath {
    pub nodes: Vec<Address>,
    pub address_metadata: AddressMetadata,
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
    our_address_metadata: AddressMetadata,
    iou_creation: Mutex<()>,
}

impl PFS {
    pub fn new(
        chain_id: ChainID,
        pfs_config: PFSConfig,
        private_key: PrivateKey,
        our_address_metadata: AddressMetadata,
    ) -> Self {
        Self {
            chain_id,
            pfs_config,
            private_key,
            our_address_metadata,
            iou_creation: Mutex::new(()),
        }
    }

    pub async fn query_paths(
        &self,
        our_address: Address,
        token_network_address: Address,
        one_to_n_address: Address,
        current_block_number: BlockNumber,
        route_from: Address,
        route_to: Address,
        value: TokenAmount,
        pfs_wait_for_block: BlockNumber,
    ) -> Result<(Vec<PFSPath>, String), RoutingError> {
        let mut payload = PFSRequest {
            from: route_from,
            to: route_to,
            max_paths: self.pfs_config.max_paths,
            iou: None,
            value,
        };
        let offered_fee = self.pfs_config.info.price;

        let mut current_info = self.get_pfs_info().await?;
        while current_info.confirmed_block_number < pfs_wait_for_block {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
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

            return Ok((response.result, response.feedback_token));
        }

        Ok((vec![], String::new()))
    }

    pub async fn get_pfs_info(&self) -> Result<PFSInfo, RoutingError> {
        let infos: PFSInfoResponse = reqwest::get(format!("{}/api/v1/info", &self.pfs_config.info.url))
            .await
            .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e)))?
            .json()
            .await
            .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e)))?;

        let matrix_server = Url::parse(&infos.matrix_server)
            .map_err(|_| RoutingError::MalformedMatrixUrl)?
            .host()
            .ok_or(RoutingError::MalformedMatrixUrl)?
            .to_string();

        Ok(PFSInfo {
            url: self.pfs_config.info.url.clone(),
            price: infos.price_info,
            chain_id: infos.network_info.chain_id,
            token_network_registry_address: infos.network_info.token_network_registry_address,
            user_deposit_address: infos.network_info.user_deposit_address,
            payment_address: infos.payment_address,
            message: infos.message,
            operator: infos.operator,
            version: infos.version,
            confirmed_block_number: infos.network_info.confirmed_block_number,
            matrix_server,
        })
    }

    pub async fn post_pfs_paths(
        &self,
        token_network_address: Address,
        payload: PFSRequest,
    ) -> Result<PFSPathsResponse, RoutingError> {
        let client = reqwest::Client::new();
        let response: PFSPathsResponse = client
            .post(format!(
                "{}/api/v1/{}/paths",
                &self.pfs_config.info.url, token_network_address
            ))
            .json(&payload)
            .send()
            .await
            .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e)))?
            .json()
            .await
            .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e)))?;

        Ok(response)
    }

    pub async fn create_current_iou(
        &self,
        token_network_address: Address,
        one_to_n_address: Address,
        our_address: Address,
        block_number: BlockNumber,
        offered_fee: TokenAmount,
        scrap_existing_iou: bool,
    ) -> Result<IOU, RoutingError> {
        if scrap_existing_iou {
            return self
                .make_iou(our_address, one_to_n_address, block_number, offered_fee)
                .await;
        }

        let latest_iou = self.get_last_iou(token_network_address, our_address).await?;

        self.update_iou(latest_iou, offered_fee, None).await
    }

    pub async fn get_last_iou(&self, token_network_address: Address, sender: Address) -> Result<IOU, RoutingError> {
        let timestamp = Utc::now();
        let signature = self
            .iou_signature_data(sender, self.pfs_config.info.payment_address, timestamp)
            .map_err(RoutingError::Signing)?;

        let client = reqwest::Client::new();
        let response = client
            .request(
                reqwest::Method::GET,
                format!(
                    "{}/api/v1/{}/payment/iou",
                    self.pfs_config.info.url, token_network_address
                ),
            )
            .query(&[
                ("sender", sender.to_string()),
                ("receiver", self.pfs_config.info.payment_address.to_string()),
                ("timestamp", timestamp.to_string()),
                ("signature", signature.to_string()),
            ])
            .send()
            .await
            .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e)))?
            .json::<HashMap<String, String>>()
            .await
            .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e)))?;

        let sender = Address::from_slice(
            response
                .get("sender")
                .ok_or(RoutingError::PFServiceInvalidResponse)?
                .as_bytes(),
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
        let amount = TokenAmount::from_dec_str(&response.get("amount").ok_or(RoutingError::PFServiceInvalidResponse)?)
            .map_err(|_| RoutingError::PFServiceInvalidResponse)?;
        let expiration_block = BlockNumber::from_str(
            response
                .get("expiration_block")
                .ok_or(RoutingError::PFServiceInvalidResponse)?
                .as_str(),
        )
        .map_err(|_| RoutingError::PFServiceInvalidResponse)?;
        let chain_id = ChainID::from_str(
            response
                .get("chain_id")
                .ok_or(RoutingError::PFServiceInvalidResponse)?
                .as_str(),
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
        one_to_n_address: Address,
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
        let mut data = vec![];
        data.extend_from_slice(sender.as_bytes());
        data.extend_from_slice(receiver.as_bytes());
        data.extend_from_slice(timestamp.as_bytes());
        Ok(self
            .private_key
            .sign(&data, Some(self.chain_id.clone() as u64))?
            .to_h256())
    }
}

pub async fn query_address_metadata(url: String, address: Address) -> Result<AddressMetadata, RoutingError> {
    let metadata = reqwest::get(format!("{}/api/v1/address/{}/metadata", url, address))
        .await
        .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Could not connect to {}", e)))?
        .json::<AddressMetadata>()
        .await
        .map_err(|e| RoutingError::PFServiceRequestFailed(format!("Malformed json in response: {}", e)))?;

    Ok(metadata)
}
