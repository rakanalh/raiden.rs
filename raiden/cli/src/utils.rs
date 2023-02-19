use std::{
	collections::HashMap,
	fs::{
		self,
		DirEntry,
		File,
	},
	io,
	io::{
		stdin,
		stdout,
		Write,
	},
	path::{
		Path,
		PathBuf,
	},
};

use ethsign::{
	KeyFile,
	Protected,
	SecretKey,
};
use raiden_blockchain::keys::PrivateKey;
use web3::types::Address;

pub fn get_private_key(keystore_path: PathBuf) -> Result<PrivateKey, String> {
	let keys =
		list_keys(keystore_path.as_path()).map_err(|e| format!("Error listing accounts: {}", e))?;
	let selected_key_filename = prompt_key(&keys);
	let secret_key = prompt_password(selected_key_filename);

	Ok(PrivateKey::new(secret_key))
}

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

pub fn use_key(keystore_file: &String, password: String) -> Option<SecretKey> {
	let file = File::open(&keystore_file).unwrap();
	let key: KeyFile = serde_json::from_reader(file).unwrap();
	let password: Protected = password.into();
	if let Ok(secret) = key.to_secret_key(&password) {
		return Some(secret)
	}
	None
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
				continue
			}
			return keys.keys().nth(chosen_index as usize).unwrap().clone()
		}
	}
}

pub fn prompt_password(key_filename: String) -> SecretKey {
	loop {
		let pass = rpassword::read_password_from_tty(Some("Password: ")).unwrap();
		let unlock = use_key(&key_filename, pass.to_string());
		if let Some(secret_key) = unlock {
			return secret_key
		}
	}
}
