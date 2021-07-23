use hyper::{
    server::conn::AddrIncoming,
    Body,
    Error,
    Request,
    Response,
    Server,
    StatusCode,
};
use parking_lot::RwLock;
use raiden::{
    api::Api,
    blockchain::{
        contracts::ContractsManager,
        proxies::ProxyManager,
    },
    state_manager::StateManager,
};
use routerify::{
    ext::RequestExt,
    Middleware,
    RequestInfo,
    Router,
    RouterService,
};
use slog::Logger;
use std::{
    net::SocketAddr,
    sync::Arc,
};

use super::endpoints;

pub struct HttpServer {
    inner: Server<AddrIncoming, RouterService<Body, Error>>,
}

impl HttpServer {
    pub fn new(
        api: Arc<Api>,
        state_manager: Arc<RwLock<StateManager>>,
        contracts_manager: Arc<ContractsManager>,
        proxy_manager: Arc<ProxyManager>,
        logger: Logger,
    ) -> Self {
        let router = router(api, state_manager, contracts_manager, proxy_manager, logger.clone());

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

async fn log_request(req: Request<Body>) -> Result<Request<Body>, Error> {
    let logger = req.data::<Logger>().unwrap().clone();
    debug!(logger, "{} {}", req.method(), req.uri().path());
    Ok(req)
}

async fn error_handler(err: routerify::RouteError, _: RequestInfo) -> Response<Body> {
    eprintln!("{}", err);
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(format!("Something went wrong: {}", err)))
        .unwrap()
}

fn router(
    api: Arc<Api>,
    state_manager: Arc<RwLock<StateManager>>,
    contracts_manager: Arc<ContractsManager>,
    proxy_manager: Arc<ProxyManager>,
    logger: Logger,
) -> Router<Body, Error> {
    Router::builder()
        // Specify the state data which will be available to every route handlers,
        // error handler and middlewares.
        .middleware(Middleware::pre(log_request))
        .data(api)
        .data(state_manager)
        .data(contracts_manager)
        .data(proxy_manager)
        .data(logger)
        .get("/api/v1/address", endpoints::address)
        .get("/api/v1/channels", endpoints::channels)
        .put("/api/v1/channels", endpoints::create_channel)
        .patch("api/v1/channels", endpoints::channel_update)
        .err_handler_with_info(error_handler)
        .build()
        .unwrap()
}
