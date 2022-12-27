use std::{collections::HashMap, sync::Arc};

use hyper::{header, Body, Error as HttpError, Request, Response, StatusCode};
use raiden::{
    blockchain::contracts::{self, ContractsManager},
    primitives::TokenAddress,
    state_machine::views,
};
use routerify::ext::RequestExt;
use web3::types::Address;

use super::{
    error::Error,
    request::InitiatePaymentParams,
    utils::{api, body_to_params, contracts_manager, state_manager},
};
use crate::{
    http::request::ChannelOpenParams,
    http::{request::ChannelPatchParams, response, utils::account},
    json_response, unwrap,
};

pub async fn address(req: Request<Body>) -> Result<Response<Body>, HttpError> {
    let state_manager = state_manager(&req);
    let our_address = state_manager.read().current_state.our_address;

    let response = response::AddressResponse { our_address };

    json_response!(response)
}

pub async fn channels(req: Request<Body>) -> Result<Response<Body>, HttpError> {
    let state_manager = state_manager(&req);

    let channels: Vec<response::ChannelResponse> = views::get_channels(&state_manager.read().current_state)
        .iter()
        .map(|c| c.clone().into())
        .collect();

    json_response!(&channels)
}

pub async fn create_channel(req: Request<Body>) -> Result<Response<Body>, HttpError> {
    let api = api(&req);
    let account = account(&req);
    let state_manager = state_manager(&req);
    let current_state = state_manager.read().current_state.clone();
    let _our_address = current_state.our_address;

    let params: ChannelOpenParams = unwrap!(body_to_params(req).await);

    let channel_identifier = unwrap!(
        api.create_channel(
            account.clone(),
            params.registry_address,
            params.token_address,
            params.partner_address,
            params.settle_timeout,
            params.reveal_timeout,
            None,
        )
        .await
    );

    if params.total_deposit.is_some() {
        unwrap!(
            api.update_channel(
                account,
                params.registry_address,
                params.token_address,
                params.partner_address,
                None,
                params.total_deposit,
                None,
                None,
                None,
            )
            .await
        );
    }

    let mut data = HashMap::new();
    data.insert("channel_identifier".to_owned(), channel_identifier);
    json_response!(data)
}

pub async fn channel_update(req: Request<Body>) -> Result<Response<Body>, HttpError> {
    let api = api(&req);
    let account = account(&req);
    let state_manager = state_manager(&req);
    let current_state = state_manager.read().current_state.clone();
    let _our_address = current_state.our_address;

    let params: ChannelPatchParams = unwrap!(body_to_params(req).await);

    let channel_state = unwrap!(
        api.update_channel(
            account,
            params.registry_address,
            params.token_address,
            params.partner_address,
            params.reveal_timeout,
            params.total_withdraw,
            params.total_deposit,
            params.state,
            None,
        )
        .await
    );

    json_response!(unwrap!(serde_json::to_string(&channel_state)))
}

pub async fn payments(req: Request<Body>) -> Result<Response<Body>, HttpError> {
    let _token_address = req.param("token_address");
    let _partner_address = req.param("partner_address");

    Ok(Response::default())
}

pub async fn initiate_payment(req: Request<Body>) -> Result<Response<Body>, HttpError> {
    let api = api(&req);
    let account = account(&req);
    let contracts_manager = contracts_manager(&req);
    // let state_manager = state_manager(&req);

    let token_address = unwrap!(req.param("token_address").ok_or(Error::Uri("Missing token address")));
    let partner_address = unwrap!(req
        .param("partner_address")
        .ok_or(Error::Uri("Missing partner address")));

    let token_address: TokenAddress = Address::from_slice(token_address.as_bytes());
    let partner_address: Address = Address::from_slice(partner_address.as_bytes());

    let params: InitiatePaymentParams = unwrap!(body_to_params(req).await);

    let default_token_network_registry = unwrap!(get_default_token_network_registry(contracts_manager.clone()));
    let default_secret_registry = unwrap!(get_default_secret_registry(contracts_manager.clone()));

    let payment = unwrap!(
        api.initiate_payment(
            account,
            default_token_network_registry,
            default_secret_registry,
            token_address,
            partner_address,
            params.amount,
            params.payment_identifier,
            params.secret,
            params.secret_hash,
            params.lock_timeout,
        )
        .await
    );

    json_response!(unwrap!(serde_json::to_string(&payment)))
}

fn get_default_token_network_registry(contracts_manager: Arc<ContractsManager>) -> Result<Address, Error> {
    let token_network_registry_deployed_contract =
        match contracts_manager.get_deployed(contracts::ContractIdentifier::SecretRegistry) {
            Ok(contract) => contract,
            Err(e) => {
                return Err(Error::Other(format!(
                    "Could not find token network registry deployment info: {:?}",
                    e
                )));
            }
        };
    Ok(token_network_registry_deployed_contract.address)
}

fn get_default_secret_registry(contracts_manager: Arc<ContractsManager>) -> Result<Address, Error> {
    let secret_registry_deployed_contract =
        match contracts_manager.get_deployed(contracts::ContractIdentifier::SecretRegistry) {
            Ok(contract) => contract,
            Err(e) => {
                return Err(Error::Other(format!(
                    "Could not find secret registry deployment info: {:?}",
                    e
                )));
            }
        };
    Ok(secret_registry_deployed_contract.address)
}
