use std::{
	path::PathBuf,
	process,
	str::FromStr,
	sync::Arc,
};

use raiden_bin_common::{
	init_private_key,
	parse_address,
};
use raiden_blockchain::{
	contracts::ContractsManager,
	proxies::{
		Account,
		ProxyManager,
	},
};
use raiden_primitives::types::ChainID;
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

	#[structopt(short("a"), long, parse(try_from_str = parse_address), takes_value = true)]
	pub address: Option<Address>,

	#[structopt(long, parse(from_os_str), takes_value = true)]
	pub password_file: Option<PathBuf>,

	#[structopt(subcommand)]
	cmd: Command,
}

#[derive(StructOpt, Debug)]
enum Command {
	Mint { amount: u64 },
	Approve { spender: String, amount: u64 },
	UserDeposit { contract_address: String, amount: u64 },
}

#[tokio::main]
async fn main() {
	let cli = Opt::from_args();

	let transport = match web3::transports::Http::new(&cli.eth_rpc_endpoint) {
		Ok(transport) => transport,
		Err(e) => {
			eprintln!("Could not connect to ETH's RPC endpoint: {}", e);
			process::exit(1);
		},
	};

	let web3 = web3::Web3::new(transport);

	let private_key = match init_private_key(
		web3.clone(),
		cli.keystore_path.clone(),
		cli.address,
		cli.password_file,
	)
	.await
	{
		Ok(result) => result,
		Err(e) => {
			eprintln!("{}", e);
			process::exit(1);
		},
	};

	let nonce = match web3
		.eth()
		.transaction_count(private_key.address(), Some(web3::types::BlockNumber::Pending))
		.await
	{
		Ok(nonce) => nonce - 1,
		Err(e) => {
			eprintln!("Failed to fetch nonce: {}", e);
			process::exit(1);
		},
	};

	let contracts_manager = match ContractsManager::new(ChainID::Private(4321.into())) {
		Ok(contracts_manager) => Arc::new(contracts_manager),
		Err(e) => {
			eprintln!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};
	let account = Account::new(web3.clone(), private_key.clone(), nonce);
	let proxy_manager = match ProxyManager::new(web3.clone(), contracts_manager) {
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

	match token_proxy.balance_of(private_key.address(), None).await {
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
		Command::Approve { spender, amount } => {
			let password =
				rpassword::prompt_password_stderr("Password:").expect("Failed to get password");

			if let Err(e) =
				web3.personal().unlock_account(private_key.address(), &password, None).await
			{
				eprintln!("Could not unlock account: {}", e);
				process::exit(1);
			}

			let spender = Address::from_str(&spender).expect("Could not parse spender");
			match token_proxy.approve(account, spender, amount.into()).await {
				Ok(hash) => {
					println!("Transaction sent: {}", hash);
				},
				Err(e) => {
					eprintln!("Error approving token: {}", e);
					process::exit(1);
				},
			}
		},
		Command::UserDeposit { contract_address, amount } => {
			let user_deposit_address =
				Address::from_str(&contract_address).expect("Could not parse user deposit address");

			match token_proxy.allowance(private_key.address(), user_deposit_address, None).await {
				Ok(approvals) => println!("Approvals: {:?}", approvals),
				Err(e) => eprintln!("Could not get balance {}", e),
			};

			let user_deposit = match proxy_manager.user_deposit(user_deposit_address).await {
				Ok(user_deposit_proxy) => user_deposit_proxy,
				Err(e) => {
					eprintln!("Error creating user_deposit contract: {}", e);
					process::exit(1);
				},
			};

			let total_deposit = user_deposit
				.total_deposit(private_key.address(), None)
				.await
				.expect("Could not fetch user deposit total_deposit");
			let new_total_deposit = total_deposit.saturating_add(amount.into());

			println!("New user deposit: {:?}", new_total_deposit);

			let password =
				rpassword::prompt_password_stderr("Password:").expect("Failed to get password");

			if let Err(e) =
				web3.personal().unlock_account(private_key.address(), &password, None).await
			{
				eprintln!("Could not unlock account: {}", e);
				process::exit(1);
			}

			match user_deposit
				.deposit(account, private_key.address(), new_total_deposit.into())
				.await
			{
				Ok(hash) => {
					println!("Transaction sent: {}", hash);
				},
				Err(e) => {
					eprintln!("Error approving token: {}", e);
					process::exit(1);
				},
			}
		},
	}
}
