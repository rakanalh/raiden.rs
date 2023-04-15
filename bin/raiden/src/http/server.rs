use std::{
	net::SocketAddr,
	sync::Arc,
};

use hyper::{
	server::conn::AddrIncoming,
	Body,
	Error,
	Request,
	Response,
	Server,
	StatusCode,
};
use raiden_api::{
	api::Api,
	raiden::Raiden,
};
use routerify::{
	Middleware,
	RequestInfo,
	Router,
	RouterService,
};
use tracing::debug;

use super::endpoints;

pub struct HttpServer {
	inner: Server<AddrIncoming, RouterService<Body, Error>>,
}

impl HttpServer {
	pub fn new(socket: SocketAddr, raiden: Arc<Raiden>, api: Arc<Api>) -> Self {
		let router = router(raiden, api);

		// Create a Service from the router above to handle incoming requests.
		let service = RouterService::new(router).unwrap();

		// Create a server by passing the created service to `.serve` method.
		let server = Server::bind(&socket).serve(service);

		Self { inner: server }
	}

	pub async fn start(self) {
		if let Err(err) = self.inner.await {
			eprintln!("Server error: {}", err);
			return
		}
	}
}

async fn log_request(req: Request<Body>) -> Result<Request<Body>, Error> {
	debug!("{} {}", req.method(), req.uri().path());
	Ok(req)
}

async fn error_handler(err: routerify::RouteError, _: RequestInfo) -> Response<Body> {
	eprintln!("{}", err);
	Response::builder()
		.status(StatusCode::INTERNAL_SERVER_ERROR)
		.body(Body::from(format!("Something went wrong: {}", err)))
		.unwrap()
}

fn router(raiden: Arc<Raiden>, api: Arc<Api>) -> Router<Body, Error> {
	Router::builder()
		// Specify the state data which will be available to every route handlers,
		// error handler and middlewares.
		.middleware(Middleware::pre(log_request))
		.data(api)
		.data(raiden.config.account.clone())
		.data(raiden.state_manager.clone())
		.data(raiden.contracts_manager.clone())
		.data(raiden.proxy_manager.clone())
		.get("/api/v1/address", endpoints::address)
		.get("/api/v1/contracts", endpoints::contracts)
		.get("/api/v1/channels", endpoints::channels)
		.put("/api/v1/channels", endpoints::create_channel)
		.patch("/api/v1/channels", endpoints::channel_update)
		.get("/api/v1/channels/:token_address", endpoints::channels)
		.get("/api/v1/channels/:token_address/:partner_address", endpoints::channels)
		.get("/api/v1/connections", endpoints::connections_info)
		.get("/api/v1/connections/:token_address", endpoints::connections_leave)
		.get("/api/v1/notifications", endpoints::notifications)
		.get("/api/v1/payments", endpoints::payments)
		.get("/api/v1/payments/:token_address", endpoints::payments)
		.get("/api/v1/payments/:token_address/:partner_address", endpoints::payments)
		.post("/api/v1/payments/:token_address/:partner_address", endpoints::initiate_payment)
		.get("/api/v1/pending_transfers", endpoints::pending_transfers)
		.get("/api/v1/pending_transfers/:token_address", endpoints::pending_transfers)
		.get(
			"/api/v1/pending_transfers/:token_address/:partner_address",
			endpoints::pending_transfers,
		)
		.get("/api/v1/settings", endpoints::settings)
		.get("/api/v1/tokens", endpoints::tokens)
		.put("/api/v1/tokens/:token_address", endpoints::register_token)
		.get("/api/v1/tokens/:token_address/partners", endpoints::partners_by_token_address)
		.post("/api/v1/user_deposit", endpoints::user_deposit)
		.get("/api/v1/status", endpoints::status)
		.get("/api/v1/version", endpoints::version)
		.err_handler_with_info(error_handler)
		.build()
		.unwrap()
}
