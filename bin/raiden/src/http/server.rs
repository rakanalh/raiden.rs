use hyper::{
    Body,
    Request,
    Response,
    Server,
    StatusCode,
};
use parking_lot::RwLock;
use raiden::state_manager::StateManager;
use routerify::{
    Middleware,
    RequestInfo,
    Router,
    RouterService,
};
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
};

use super::endpoints;

// A middleware which logs an http request.
async fn logger(req: Request<Body>) -> Result<Request<Body>, Infallible> {
    println!("{} {}", req.method(), req.uri().path());
    Ok(req)
}

async fn error_handler(err: routerify::Error, _: RequestInfo) -> Response<Body> {
    eprintln!("{}", err);
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(format!("Something went wrong: {}", err)))
        .unwrap()
}

// Create a `Router<Body, Infallible>` for response body type `hyper::Body`
// and for handler error type `Infallible`.
fn router(state_manager: Arc<RwLock<StateManager>>) -> Router<Body, Infallible> {
    // Create a router and specify the logger middleware and the handlers.
    // Here, "Middleware::pre" means we're adding a pre middleware which will be executed
    // before any route handlers.
    Router::builder()
        // Specify the state data which will be available to every route handlers,
        // error handler and middlewares.
        .middleware(Middleware::pre(logger))
        .data(state_manager)
        .get("/api/v1/address", endpoints::address)
        .err_handler_with_info(error_handler)
        .build()
        .unwrap()
}

pub async fn start_server(state_manager: Arc<RwLock<StateManager>>) {
    let router = router(state_manager);

    // Create a Service from the router above to handle incoming requests.
    let service = RouterService::new(router).unwrap();

    // The address on which the server will be listening.
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // Create a server by passing the created service to `.serve` method.
    let server = Server::bind(&addr).serve(service);

    if let Err(err) = server.await {
        eprintln!("Server error: {}", err);
        return;
    }
}
