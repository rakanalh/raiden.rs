use std::{
	collections::HashMap,
	fs::{
		self,
		DirEntry,
	},
	io,
	path::Path,
};

use ethsign::KeyFile;
use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::types::Address;
use web3::{
	signing::Key,
	transports::Http,
	Web3,
};

pub fn list_keys(keystore: &Path) -> io::Result<HashMap<String, Address>> {
	let mut keys: HashMap<String, Address> = HashMap::new();
	for entry in fs::read_dir(keystore)? {
		let entry: DirEntry = entry?;
		let file_name: String = String::from(entry.path().to_str().unwrap());
		let file = std::fs::File::open(&file_name).unwrap();
		let key: KeyFile = serde_json::from_reader(file).unwrap();
		let address: Address = Address::from_slice(&key.address.unwrap().0);
		keys.insert(file_name, address);
	}
	Ok(keys)
}

pub async fn unlock_private_key(
	web3: Web3<Http>,
	key_filename: String,
	password: String,
) -> Result<PrivateKey, String> {
	let private_key = PrivateKey::new(key_filename.clone(), password.clone())
		.map_err(|e| format!("Could not unlock private key: {:?}", e))?;

	if !password.is_empty() {
		web3.personal()
			.unlock_account(private_key.address(), &password, None)
			.await
			.map_err(|e| format!("Could not unlock account on ethereum node: {:?}", e))?;
	}

	Ok(private_key)
}
