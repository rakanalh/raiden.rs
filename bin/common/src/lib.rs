use std::{
	collections::HashMap,
	error::Error,
	fs,
	io::{
		stdin,
		stdout,
		Write,
	},
	path::PathBuf,
	str::FromStr,
};

use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::types::Address;
use web3::{
	transports::Http,
	Web3,
};

use crate::cli::{
	list_keys,
	unlock_private_key,
};

pub mod cli;

pub fn parse_address(address: &str) -> Result<Address, Box<dyn Error + Send + Sync + 'static>> {
	Ok(Address::from_str(address)?)
}

pub fn prompt_key(keys: &HashMap<String, Address>) -> String {
	println!("Select key:");
	loop {
		let mut s = String::new();

		for (index, address) in keys.values().enumerate() {
			println!("[{}]: {}", index, address);
		}
		print!("Selected key: ");
		let _ = stdout().flush();
		stdin().read_line(&mut s).expect("Did not enter a correct string");
		let selected_value: Result<u32, _> = s.trim().parse();
		if let Ok(chosen_index) = selected_value {
			if (chosen_index as usize) >= keys.len() {
				continue
			}
			let selected_filename = keys.keys().nth(chosen_index as usize).unwrap();
			return selected_filename.clone()
		}
	}
}

pub async fn init_private_key(
	web3: Web3<Http>,
	keystore_path: PathBuf,
	address: Option<Address>,
	password_file: Option<PathBuf>,
) -> Result<PrivateKey, String> {
	let keys = list_keys(&keystore_path).map_err(|e| format!("Could not list accounts: {}", e))?;

	let key_filename = if let Some(address) = address {
		let inverted_keys: HashMap<Address, String> =
			keys.iter().map(|(k, v)| (*v, k.clone())).collect();
		inverted_keys.get(&address).unwrap().clone()
	} else {
		prompt_key(&keys)
	};

	let password = if let Some(password_file) = password_file {
		fs::read_to_string(password_file)
			.map_err(|e| format!("Error reading password file: {:?}", e))?
			.trim()
			.to_owned()
	} else {
		rpassword::read_password_from_tty(Some("Password: "))
			.map_err(|e| format!("Could not read password: {:?}", e))?
	};

	unlock_private_key(web3, key_filename, password).await
}
