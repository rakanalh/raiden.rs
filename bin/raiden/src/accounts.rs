use ethsign::{
    KeyFile,
    Protected,
    SecretKey,
};
use serde_json;
use std::collections::HashMap;
use std::fs::{
    self,
    DirEntry,
    File,
};
use std::io;
use std::path::Path;
use web3::types::Address;

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
        return Some(secret);
    }
    None
}
