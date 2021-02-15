#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate tokio;
extern crate web3;

use slog::Drain;
use std::path::Path;

mod accounts;
mod cli;
mod event_handler;
mod raiden_service;
mod traits;
use traits::{
    ToHTTPEndpoint,
    ToSocketEndpoint,
};

#[tokio::main]
async fn main() {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    let log = slog::Logger::root(drain, o!());

    let cli_app = cli::get_cli_app();
    let matches = cli_app.get_matches();

    let chain_name = matches.value_of("chain-id").unwrap();
    let chain_id = chain_name.parse().unwrap();

    let eth_rpc_http_endpoint = matches.value_of("eth-rpc-endpoint").unwrap();
    let eth_rpc_socket_endpoint = matches.value_of("eth-rpc-socket-endpoint").unwrap();
    let http_endpoint = eth_rpc_http_endpoint.to_http();
    if let Err(e) = http_endpoint {
        crit!(log, "Invalid RPC endpoint: {}", e);
        return;
    }

    let socket_endpoint = eth_rpc_socket_endpoint.to_socket();
    if let Err(e) = socket_endpoint {
        crit!(log, "Invalid RPC endpoint: {}", e);
        return;
    }

    let keystore_path = Path::new(matches.value_of("keystore-path").unwrap());
    let keys = accounts::list_keys(keystore_path).unwrap();

    let selected_key_filename = cli::prompt_key(&keys);
    let our_address = keys[&selected_key_filename].clone();
    let private_key = cli::prompt_password(selected_key_filename, log.clone());

    let config = cli::Config {
        keystore_path,
        private_key,
        eth_http_rpc_endpoint: http_endpoint.unwrap(),
        eth_socket_rpc_endpoint: socket_endpoint.unwrap(),
    };

    let http = web3::transports::Http::new(&config.eth_http_rpc_endpoint).unwrap();
    let web3 = web3::Web3::new(http);

    let service =
        raiden_service::RaidenService::new(web3, chain_id, our_address, config.private_key.clone(), log.clone());

    service.initialize().await;
    service.start(config).await;

    if let Some(_) = matches.subcommand_matches("run") {
        //let server = http::server(log.clone());
        // let _ = eloop.run(server);
    }
}
