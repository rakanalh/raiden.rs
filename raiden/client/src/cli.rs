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
use raiden_primitives::types::Address;

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
