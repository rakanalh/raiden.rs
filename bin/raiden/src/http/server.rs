use hyper::{
    server::conn::AddrIncoming,
    Body,
    Request,
    Response,
    Server,
    StatusCode,
};
use parking_lot::RwLock;
use raiden::state_manager::StateManager;
use routerify::{
    ext::RequestExt,
    Middleware,
    RequestInfo,
    Router,
    RouterService,
};
use slog::Logger;
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
};

use super::endpoints;

pub struct HttpServer {
    inner: Server<AddrIncoming, RouterService<Body, Infallible>>,
}

impl HttpServer {
    pub fn new(state_manager: Arc<RwLock<StateManager>>, logger: Logger) -> Self {
        let router = router(state_manager, logger.clone());

        // Create a Service from the router above to handle incoming requests.
        let service = RouterService::new(router).unwrap();

        // The address on which the server will be listening.
        let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

        // Create a server by passing the created service to `.serve` method.
        let server = Server::bind(&addr).serve(service);

        Self { inner: server }
    }

    pub async fn start(self) {
        if let Err(err) = self.inner.await {
            eprintln!("Server error: {}", err);
            return;
        }
    }
}

async fn log_request(req: Request<Body>) -> Result<Request<Body>, Infallible> {
    let logger = req.data::<Logger>().unwrap().clone();
    debug!(logger, "{} {}", req.method(), req.uri().path());
    Ok(req)
}

async fn error_handler(err: routerify::Error, _: RequestInfo) -> Response<Body> {
    eprintln!("{}", err);
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(format!("Something went wrong: {}", err)))
        .unwrap()
}

fn router(state_manager: Arc<RwLock<StateManager>>, logger: Logger) -> Router<Body, Infallible> {
    Router::builder()
        // Specify the state data which will be available to every route handlers,
        // error handler and middlewares.
        .middleware(Middleware::pre(log_request))
        .data(state_manager)
        .data(logger)
        .get("/api/v1/address", endpoints::address)
        .err_handler_with_info(error_handler)
        .build()
        .unwrap()
}
