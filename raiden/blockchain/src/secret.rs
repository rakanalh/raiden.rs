use raiden_primitives::{
	signing::recover_pub_key,
	types::{
		AddressMetadata,
		Bytes,
		PaymentIdentifier,
		Secret,
		TokenAmount,
	},
};
use raiden_state_machine::types::DecryptedSecret;

use crate::keys::{
	self,
	PrivateKey,
};

pub fn encrypt_secret(
	secret: Secret,
	target_metadata: AddressMetadata,
	amount: TokenAmount,
	payment_identifier: PaymentIdentifier,
) -> Result<Bytes, String> {
	let message = target_metadata.user_id;
	let signature = hex::decode(target_metadata.displayname.trim_start_matches("0x"))
		.map_err(|e| format!("Could not decode signature: {:?}", e))?;
	let public_key = recover_pub_key(&message.as_bytes(), &signature)
		.map_err(|e| format!("Could not recover public key: {:?}", e))?;

	let data = DecryptedSecret { secret, amount, payment_identifier };

	let json = serde_json::to_string(&data)
		.map_err(|e| format!("Could not serialize encrypted secret: {}", e))?;

	Ok(Bytes(
		keys::encrypt(&public_key.0, json.as_bytes())
			.map_err(|e| format!("Could not encrypt secret: {:?}", e))?,
	))
}

pub fn decrypt_secret(
	encrypted_secret: Vec<u8>,
	private_key: &PrivateKey,
) -> Result<DecryptedSecret, String> {
	let decrypted_secret = keys::decrypt(&private_key, &encrypted_secret)
		.map_err(|e| format!("Could not decrypt secret: {:?}", e))?;
	let json = std::str::from_utf8(&decrypted_secret)
		.map_err(|e| format!("Invalid UTF-8 sequence: {}", e))?;
	serde_json::from_str(json).map_err(|e| format!("Could not deserialize secret: {:?}", e))
}
