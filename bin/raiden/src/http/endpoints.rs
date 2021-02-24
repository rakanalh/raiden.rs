use std::{
    collections::HashMap,
    convert::Infallible,
};

use hyper::{
    header,
    Body,
    Request,
    Response,
    StatusCode,
};

use super::utils::state_manager;

pub async fn address(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let state_manager = state_manager(req);
    let our_address = state_manager.read().current_state.our_address;

    let mut data = HashMap::new();
    data.insert("our_address", our_address.to_string());
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
