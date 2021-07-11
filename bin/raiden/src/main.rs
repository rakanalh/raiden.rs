#[macro_use]
extern crate slog;
// extern crate slog_term;
// extern crate tokio;
// extern crate web3;

use clap::Clap;
use cli::RaidenApp;
use raiden::blockchain::key::PrivateKey;
use slog::Drain;
use std::{
    convert::TryInto,
    fs,
    path::PathBuf,
    process,
};
use web3::types::Address;

use crate::cli::Opt;

mod accounts;
mod cli;
mod event_handler;
mod http;
mod services;
mod traits;

#[tokio::main]
async fn main() {
    let cli = Opt::parse();

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

    let (node_address, secret_key) = prompt_key(cli.keystore_path.clone());

    info!(logger, "Welcome to Raiden");
    info!(logger, "Initializing");

    let configs = match cli.try_into() {
        Ok(configs) => configs,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    let raiden_app = match RaidenApp::new(configs, node_address, secret_key, logger.clone()) {
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

fn prompt_key(keystore_path: PathBuf) -> (Address, PrivateKey) {
    let keys = accounts::list_keys(keystore_path.as_path()).unwrap();
    let selected_key_filename = crate::cli::prompt_key(&keys);
    let our_address = keys[&selected_key_filename].clone();
    let secret_key = crate::cli::prompt_password(selected_key_filename);

    (our_address, PrivateKey::new(secret_key))
}
