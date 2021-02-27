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
use url::form_urlencoded;

use super::utils::{
    contracts_registry,
    state_manager,
};
use crate::json_response;

pub async fn address(req: Request<Body>) -> Result<Response<Body>, Error> {
    let state_manager = state_manager(&req);
    let our_address = state_manager.read().current_state.our_address;

    let mut data = HashMap::new();
    data.insert("our_address", our_address.to_string());

    json_response!(data)
}

pub async fn channels(req: Request<Body>) -> Result<Response<Body>, Error> {
    let state_manager = state_manager(&req);

    let channels = views::get_channels(&state_manager.read().current_state);

    let res = match serde_json::to_string(&channels) {
        Ok(json) => Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Internal Server Error"))
            .unwrap(),
    };

    Ok(res)
}
    let res = match serde_json::to_string(&data) {
        Ok(json) => Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Internal Server Error"))
            .unwrap(),
    };

    Ok(res)
}
