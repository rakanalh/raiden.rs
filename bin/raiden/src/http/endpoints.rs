use std::collections::HashMap;

use hyper::{
    header,
    Body,
    Error,
    Request,
    Response,
    StatusCode,
};
use raiden::state_machine::views;

use super::utils::{
    api,
    body_to_params,
    state_manager,
};
use crate::{
    error,
    http::request::ChannelOpenParams,
    http::{
        request::ChannelPatchParams,
        response,
    },
    json_response,
    unwrap,
};

pub async fn address(req: Request<Body>) -> Result<Response<Body>, Error> {
    let state_manager = state_manager(&req);
    let our_address = state_manager.read().current_state.our_address;

    let response = response::AddressResponse { our_address };

    json_response!(response)
}

pub async fn channels(req: Request<Body>) -> Result<Response<Body>, Error> {
    let state_manager = state_manager(&req);

    let channels: Vec<response::ChannelResponse> = views::get_channels(&state_manager.read().current_state)
        .iter()
        .map(|c| c.clone().into())
        .collect();

    json_response!(&channels)
}

pub async fn create_channel(req: Request<Body>) -> Result<Response<Body>, Error> {
    let api = api(&req);
    let state_manager = state_manager(&req);
    let current_state = state_manager.read().current_state.clone();
    let _our_address = current_state.our_address;

    let params: ChannelOpenParams = match body_to_params(req).await {
        Ok(p) => p,
        Err(super::error::Error::Http(e)) => return Err(e),
        Err(super::error::Error::Serialization(e)) => {
            error!(e);
        }
    };

    let channel_identifier = unwrap!(
        api.create_channel(
            params.registry_address,
            params.token_address,
            params.partner_address,
            params.settle_timeout,
            params.reveal_timeout,
            None,
        )
        .await
    );

    let mut data = HashMap::new();
    data.insert("channel_identifier".to_owned(), channel_identifier);
    json_response!(data)
}

pub async fn channel_update(req: Request<Body>) -> Result<Response<Body>, Error> {
    let api = api(&req);
    let state_manager = state_manager(&req);
    let current_state = state_manager.read().current_state.clone();
    let _our_address = current_state.our_address;

    let params: ChannelPatchParams = match body_to_params(req).await {
        Ok(p) => p,
        Err(super::error::Error::Http(e)) => return Err(e),
        Err(super::error::Error::Serialization(e)) => {
            error!(e);
        }
    };

    let channel_state = unwrap!(
        api.update_channel(
            params.registry_address,
            params.token_address,
            params.partner_address,
            params.reveal_timeout,
            params.total_withdraw,
            params.total_deposit,
            params.state,
        )
        .await
    );

    json_response!(unwrap!(serde_json::to_string(&channel_state)))
}
