use web3::{
	ethabi::Address,
	signing::RecoveryError,
};

pub fn hash_data(data: &[u8]) -> [u8; 32] {
	let prefix_msg = "\x19Ethereum Signed Message:\n";
	let len_str = data.len().to_string();
	let mut res: Vec<u8> = Vec::new();
	res.append(&mut prefix_msg.as_bytes().to_vec());
	res.append(&mut len_str.as_bytes().to_vec());
	res.append(&mut data.to_vec());

	web3::signing::keccak256(&res)
}

pub fn recover(data: &[u8], signature: &[u8]) -> Result<Address, RecoveryError> {
	let data_hash = hash_data(data);
	let recovery_id = signature[64] as i32 - 27;
	web3::signing::recover(&data_hash, &signature[..64], recovery_id)
}
