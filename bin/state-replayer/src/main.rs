use std::{
	collections::HashMap,
	path::PathBuf,
	process,
};

use colored::Colorize;
use raiden_bin_common::parse_address;
use raiden_blockchain::contracts;
use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockNumber,
	ChainID,
	H256,
	U256,
};
use raiden_state_machine::{
	machine::chain,
	storage::{
		types::StorageID,
		StateStorage,
	},
	types::{
		ChainState,
		ContractReceiveTokenNetworkRegistry,
		PaymentMappingState,
		Random,
		TokenNetworkRegistryState,
	},
};
use rusqlite::Connection;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "State Replayer")]
pub struct Opt {
	#[structopt(short("d"), long, parse(from_os_str), required = true, takes_value = true)]
	pub dbpath: PathBuf,
	#[structopt(short("a"), long, parse(try_from_str = parse_address), takes_value = true)]
	pub address: Address,
}

fn main() {
	let cli = Opt::from_args();

	let dbpath = match expanduser::expanduser(cli.dbpath.to_string_lossy()) {
		Ok(p) => p,
		Err(e) => {
			eprintln!("Error expanding db path: {}", e);
			process::exit(1);
		},
	};

	let conn = match Connection::open(dbpath) {
		Ok(conn) => conn,
		Err(e) => {
			eprintln!("Could not connect to database: {}", e);
			process::exit(1);
		},
	};

	let chain_id = ChainID::Private(U256::zero());

	let contracts_manager = match contracts::ContractsManager::new(chain_id) {
		Ok(contracts_manager) => contracts_manager,
		Err(e) => {
			eprintln!("Error creating contracts manager: {}", e);
			process::exit(1);
		},
	};
	let default_addresses = match contracts_manager.deployed_addresses() {
		Ok(addresses) => addresses,
		Err(e) => {
			eprintln!("Failed to construct default deployed addresses: {:?}", e);
			process::exit(1);
		},
	};

	let storage = StateStorage::new(conn);
	let state_change_records =
		match storage.get_state_changes_in_range(StorageID::zero(), StorageID::max()) {
			Ok(state_changes) => state_changes,
			Err(e) => {
				eprintln!("Could not fetch state changes: {}", e);
				process::exit(1);
			},
		};

	let mut chain_state = ChainState {
		chain_id,
		block_number: BlockNumber::from(0),
		block_hash: BlockHash::random(),
		our_address: cli.address,
		identifiers_to_tokennetworkregistries: HashMap::new(),
		payment_mapping: PaymentMappingState { secrethashes_to_task: HashMap::new() },
		pending_transactions: vec![],
		pseudo_random_number_generator: Random::new(),
	};

	let token_network_registry_state_change = ContractReceiveTokenNetworkRegistry {
		transaction_hash: Some(H256::zero()),
		token_network_registry: TokenNetworkRegistryState {
			address: default_addresses.token_network_registry,
			tokennetworkaddresses_to_tokennetworks: HashMap::new(),
			tokenaddresses_to_tokennetworkaddresses: HashMap::new(),
		},
		block_number: BlockNumber::from(1),
		block_hash: H256::zero(),
	};
	let result = match chain::state_transition(
		chain_state.clone(),
		token_network_registry_state_change.into(),
	) {
		Ok(transition) => transition,
		Err(e) => {
			eprintln!("\tError: {:?}", e.msg);
			process::exit(1);
		},
	};
	chain_state = result.new_state;

	for state_change_record in state_change_records {
		let state_change = state_change_record.data;
		println!();
		print!("{}", "StateChange ->".red().bold());
		println!(" {:#?}", state_change);
		let result = match chain::state_transition(chain_state.clone(), state_change) {
			Ok(transition) => transition,
			Err(e) => {
				eprintln!("\tError: {:?}", e.msg);
				continue
			},
		};
		for event in result.events {
			let event_str = format!("{:#?}", event).replace('\n', "\n\t");
			print!("\t{}", "<- Event ".yellow().bold());
			println!("{}", event_str);
			continue
		}
		println!();
		chain_state = result.new_state;
	}

	println!("{}", "FINAL STATE:".green().on_white().bold());
	println!("{:#?}", chain_state);
}
