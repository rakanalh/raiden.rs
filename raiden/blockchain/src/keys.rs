use std::fs::File;

pub use ecies::SecpError;
use ethsign::{
	KeyFile,
	Protected,
	SecretKey,
};
use raiden_primitives::{
	signing::hash_data,
	types::{
		Address,
		H256,
	},
};
use web3::signing::{
	Key,
	Signature,
	SigningError,
};

/// Encrypt bytes with the receiver's public key.
pub fn encrypt(receiver_pub: &[u8], data: &[u8]) -> Result<Vec<u8>, SecpError> {
	ecies::encrypt(receiver_pub, data)
}

/// Decrypts bytes using the current account's private key.
pub fn decrypt(private_key: &PrivateKey, data: &[u8]) -> Result<Vec<u8>, SecpError> {
	ecies::decrypt(private_key.plain.as_ref(), data)
}

/// A wrapper of `SecretKey` to use for signing.
#[derive(Clone)]
pub struct PrivateKey {
	plain: Protected,
	inner: SecretKey,
}

impl PrivateKey {
	/// Creates a new instance of `PrivateKey`.
	pub fn new(filename: String, password: String) -> Result<Self, String> {
		let file = File::open(&filename)
			.map_err(|e| format!("Could not open file {}: {}", filename, e))?;

		let key: KeyFile = serde_json::from_reader(file)
			.map_err(|e| format!("Could not read file {}: {}", filename, e))?;

		let plain = key
			.crypto
			.decrypt(&password.into())
			.map_err(|e| format!("Could not decrypt private key file {}: {}", filename, e))?;

		let inner = SecretKey::from_raw(&plain)
			.map_err(|e| format!("Could not generate secret key from file {}: {}", filename, e))?;

		Ok(Self { plain: plain.into(), inner })
	}
}

impl Key for PrivateKey {
	/// Signs bytes using the inner `SecretKey` with chain ID.
	fn sign(&self, message: &[u8], chain_id: Option<u64>) -> Result<Signature, SigningError> {
		let signature = self.inner.sign(message).map_err(|_| SigningError::InvalidMessage)?;

		let standard_v = signature.v as u64;
		let v = if let Some(chain_id) = chain_id {
			standard_v + 35 + chain_id * 2
		} else {
			standard_v + 27
		};
		Ok(Signature { r: H256::from(signature.r), s: H256::from(signature.s), v })
	}

	/// Signs message bytes using the inner `SecretKey`.
	fn sign_message(&self, message: &[u8]) -> Result<Signature, SigningError> {
		let data_hash = hash_data(message);
		let signature = self.inner.sign(&data_hash).map_err(|_| SigningError::InvalidMessage)?;

		Ok(Signature {
			r: H256::from(signature.r),
			s: H256::from(signature.s),
			v: signature.v as u64 + 27,
		})
	}

	fn address(&self) -> Address {
		Address::from(self.inner.public().address())
	}
}
