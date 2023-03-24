use once_cell::sync::Lazy;
use secp256k1::{
	ecdsa::{
		RecoverableSignature,
		RecoveryId,
	},
	All,
	Message,
	Secp256k1,
};
use web3::{
	ethabi::Address,
	signing::RecoveryError,
	types::Bytes,
};

static CONTEXT: Lazy<Secp256k1<All>> = Lazy::new(Secp256k1::new);

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

pub fn recover_pub_key(data: &[u8], signature: &[u8]) -> Result<Bytes, RecoveryError> {
	let data_hash = hash_data(data);
	let recovery_id = signature[64] as i32 - 27;
	let message = Message::from_slice(&data_hash).map_err(|_| RecoveryError::InvalidMessage)?;
	let recovery_id =
		RecoveryId::from_i32(recovery_id).map_err(|_| RecoveryError::InvalidSignature)?;
	let signature = RecoverableSignature::from_compact(&signature[..64], recovery_id)
		.map_err(|_| RecoveryError::InvalidSignature)?;
	let public_key = CONTEXT
		.recover_ecdsa(&message, &signature)
		.map_err(|_| RecoveryError::InvalidSignature)?;

	let public_key: [u8; 65] = public_key.serialize_uncompressed();
	Ok(Bytes(public_key[1..].to_vec()))
}
