use clap::{
    App,
    Arg,
    SubCommand,
};
use ethsign::SecretKey;
use rpassword;
use slog::Logger;
use std::collections::HashMap;
use std::io::{
    stdin,
    stdout,
    Write,
};
use std::path::Path;
use web3::types::Address;
use crate::accounts;

#[derive(Clone)]
pub struct Config<'a> {
    pub keystore_path: &'a Path,
    pub private_key: SecretKey,

    pub eth_http_rpc_endpoint: String,
    pub eth_socket_rpc_endpoint: String,
}

pub fn get_cli_app<'a, 'b>() -> App<'a, 'b> {
    App::new("Raiden unofficial rust client")
        .arg(
            Arg::with_name("chain-id")
                .short("c")
                .long("chain-id")
                .possible_values(&["ropsten", "kovan", "goerli", "rinkeby", "mainnet"])
                .default_value("mainnet")
                .required(true)
                .takes_value(true)
                .help("Specify the blockchain to run Raiden with"),
        )
        .arg(
            Arg::with_name("eth-rpc-endpoint")
                .long("eth-rpc-endpoint")
                .required(true)
                .takes_value(true)
                .help("Specify the RPC endpoint to interact with"),
        )
        .arg(
            Arg::with_name("eth-rpc-socket-endpoint")
                .long("eth-rpc-socket-endpoint")
                .required(true)
                .takes_value(true)
                .help("Specify the RPC endpoint to interact with"),
        )
        .arg(
            Arg::with_name("keystore-path")
                .short("k")
                .long("keystore-path")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .subcommand(SubCommand::with_name("run").about("Run the raiden client"))
}

pub fn prompt_key(keys: &HashMap<String, Address>) -> String {
    println!("Select key:");
    loop {
        let mut index = 0;
        let mut s = String::new();

        for address in keys.values() {
            println!("[{}]: {}", index, address);
            index += 1;
        }
        print!("Selected key: ");
        let _ = stdout().flush();
        stdin().read_line(&mut s).expect("Did not enter a correct string");
        let selected_value: Result<u32, _> = s.trim().parse();
        if let Ok(chosen_index) = selected_value {
            if (chosen_index as usize) >= keys.len() {
                continue;
            }
            return keys.keys().nth(chosen_index as usize).unwrap().clone();
        }
    }
}

pub fn prompt_password(key_filename: String, log: Logger) -> SecretKey {
    loop {
        let pass = rpassword::read_password_from_tty(Some("Password: ")).unwrap();
        let unlock = accounts::use_key(&key_filename, pass.to_string());
        info!(log, "Key unlocked");
        if let Some(secret_key) = unlock {
            return secret_key;
        }
    }
}
