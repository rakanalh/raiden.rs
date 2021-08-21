#[macro_use]
extern crate slog;
use cli::RaidenApp;
use raiden::{
    blockchain::{
        key::PrivateKey,
        proxies::Account,
    },
    primitives::{
        MatrixTransportConfig,
        MediationFeeConfig,
        RaidenConfig,
    },
    transport::matrix::{
        constants::MATRIX_AUTO_SELECT_SERVER,
        utils::{
            get_default_matrix_servers,
            select_best_server,
        },
    },
};
use slog::Drain;
use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    process,
};
use structopt::StructOpt;
use web3::{
    signing::Key,
    transports::Http,
    types::Address,
    Web3,
};

use crate::{
    cli::Opt,
    traits::{
        ToHTTPEndpoint,
        ToSocketEndpoint,
    },
};

mod accounts;
mod cli;
mod event_handler;
mod http;
mod services;
mod traits;

#[tokio::main]
async fn main() {
    let cli = Opt::from_args();

    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let logger = slog::Logger::root(drain, o!());

    match setup_data_directory(cli.datadir.clone()) {
        Err(e) => {
            eprintln!("Error initializing data directory: {}", e);
            process::exit(1);
        }
        _ => {}
    };

    let private_key = match prompt_key(cli.keystore_path.clone()) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    info!(logger, "Welcome to Raiden");
    info!(logger, "Initializing");

    let (config, web3) = match raiden_config(cli, private_key.clone()).await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    let raiden_app = match RaidenApp::new(config, web3, logger.clone()) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Error initializing app: {}", e);
            process::exit(1);
        }
    };

    info!(logger, "Raiden is starting");
    raiden_app.run().await;
}

fn setup_data_directory(path: PathBuf) -> Result<PathBuf, String> {
    let path =
        expanduser::expanduser(path.to_string_lossy()).map_err(|_| "Failed to expand data directory".to_owned())?;

    if !path.is_dir() {
        return Err("Datadir has to be a directory".to_owned());
    }

    if !path.exists() {
        match fs::create_dir(path.clone()) {
            Err(e) => return Err(format!("Could not create directory: {:?} because {}", path.clone(), e)),
            _ => {}
        }
    }
    Ok(path.to_path_buf())
}

fn prompt_key(keystore_path: PathBuf) -> Result<PrivateKey, String> {
    let keys = accounts::list_keys(keystore_path.as_path()).map_err(|e| format!("Error listing accounts: {}", e))?;
    let selected_key_filename = crate::cli::prompt_key(&keys);
    let secret_key = crate::cli::prompt_password(selected_key_filename);

    Ok(PrivateKey::new(secret_key))
}

async fn raiden_config(cli: Opt, private_key: PrivateKey) -> Result<(RaidenConfig, Web3<Http>), String> {
    let chain_id = cli.chain_id.into();
    let eth_rpc_http_endpoint = cli
        .eth_rpc_endpoint
        .to_http()
        .map_err(|e| format!("Invalid RPC endpoint: {}", e))?;
    let eth_rpc_socket_endpoint = cli
        .eth_rpc_socket_endpoint
        .to_socket()
        .map_err(|e| format!("Invalid RPC endpoint {}", e))?;

    let keystore_path = Path::new(&cli.keystore_path);
    let datadir = expanduser::expanduser(cli.datadir.to_string_lossy())
        .map_err(|e| format!("Error expanding data directory: {}", e))?;

    let mediation_config = MediationFeeConfig {
        token_to_flat_fee: cli
            .mediation_fees
            .flat_fee
            .into_iter()
            .map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
            .collect(),
        token_to_proportional_fee: cli
            .mediation_fees
            .proportional_fee
            .into_iter()
            .map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
            .collect(),
        token_to_proportional_imbalance_fee: cli
            .mediation_fees
            .proportional_imbalance_fee
            .into_iter()
            .map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
            .collect(),
        cap_meditation_fees: cli.mediation_fees.cap_mediation_fees,
    };

    let http = web3::transports::Http::new(&eth_rpc_http_endpoint).unwrap();
    let web3 = web3::Web3::new(http);

    let nonce = match web3.eth().transaction_count(private_key.address(), None).await {
        Ok(nonce) => nonce,
        Err(e) => return Err(format!("Failed to fetch nonce: {}", e)),
    };

    let account = Account::new(web3.clone(), private_key, nonce);

    let homeserver_url = if cli.matrix_transport_config.matrix_server == MATRIX_AUTO_SELECT_SERVER {
        let servers = get_default_matrix_servers(cli.environment_type.into())
            .await
            .map_err(|_| format!("Could not fetch default matrix servers"))?;
        select_best_server(servers)
    } else {
        cli.matrix_transport_config.matrix_server
    };

    let transport_config = MatrixTransportConfig {
        homeserver_url
    };

    let config = RaidenConfig {
        chain_id,
        account,
        datadir,
        mediation_config,
        transport_config,
        keystore_path: keystore_path.to_path_buf(),
        eth_http_rpc_endpoint: eth_rpc_http_endpoint,
        eth_socket_rpc_endpoint: eth_rpc_socket_endpoint,
    };

    Ok((config, web3))
}
