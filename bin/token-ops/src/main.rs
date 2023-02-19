use std::{
	path::PathBuf,
	process,
	str::FromStr,
	sync::Arc,
};

use raiden_blockchain::{
	contracts::{
		ChainID,
		ContractsManager,
	},
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_cli::utils::get_private_key;
use structopt::StructOpt;
use web3::{
	signing::Key,
	types::Address,
};

#[derive(StructOpt, Debug)]
#[structopt(name = "Token Ops")]
pub struct Opt {
	/// Specify the RPC endpoint to interact with.
	#[structopt(long, required = true, takes_value = true)]
	pub eth_rpc_endpoint: String,

	#[structopt(short("k"), long, parse(from_os_str), required = true, takes_value = true)]
	pub keystore_path: PathBuf,

	#[structopt(long, required = true, takes_value = true)]
	pub token_address: String,

	#[structopt(subcommand)]
	cmd: Command,
}

#[derive(StructOpt, Debug)]
enum Command {
	Mint { amount: u64 },
}

#[tokio::main]
async fn main() {
	let cli = Opt::from_args();

	let private_key = match get_private_key(cli.keystore_path.clone()) {
		Ok(result) => result,
		Err(e) => {
			eprintln!("{}", e);
			process::exit(1);
		},
	};

	let transport = match web3::transports::Http::new(&cli.eth_rpc_endpoint) {
		Ok(transport) => transport,
		Err(e) => {
			eprintln!("Could not connect to ETH's RPC endpoint: {}", e);
			process::exit(1);
		},
	};

	let web3 = web3::Web3::new(transport);

	let nonce = match web3.eth().transaction_count(private_key.address(), None).await {
		Ok(nonce) => nonce,
		Err(e) => {
			eprintln!("Failed to fetch nonce: {}", e);
			process::exit(1);
		},
	};

	let contracts_manager = match ContractsManager::new(ChainID::Private) {
		Ok(contracts_manager) => Arc::new(contracts_manager),
		Err(e) => {
			eprintln!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};
	let account = Account::new(web3.clone(), private_key, nonce);
	let proxy_manager = match ProxyManager::new(web3, contracts_manager) {
		Ok(proxy_manager) => proxy_manager,
		Err(e) => {
			eprintln!("Error creating contracts proxy: {}", e);
			process::exit(1);
		},
	};
	let token_address = Address::from_str(&cli.token_address).expect("Invalid token address");
	let token_proxy = match proxy_manager.token(token_address).await {
		Ok(token_proxy) => token_proxy,
		Err(e) => {
			eprintln!("Error creating token contract: {}", e);
			process::exit(1);
		},
	};

	match token_proxy
		.balance_of(
			Address::from_str("0x1B74935E78F33695962c9ac278127335A4089882").expect("123"),
			None,
		)
		.await
	{
		Ok(balance) => println!("Balance: {}", balance),
		Err(e) => eprintln!("Could not get balance {}", e),
	};

	match cli.cmd {
		Command::Mint { amount } => match token_proxy.mint(account, amount.into()).await {
			Ok(hash) => {
				println!("Transaction sent: {}", hash);
			},
			Err(e) => {
				eprintln!("Error minting token: {}", e);
				process::exit(1);
			},
		},
	}
}
