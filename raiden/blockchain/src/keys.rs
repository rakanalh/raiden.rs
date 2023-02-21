use ethsign::SecretKey;
use tiny_keccak::{
	Hasher,
	Keccak,
};
use web3::{
	signing::{
		self,
		Key,
		Signature,
	},
	types::{
		Address,
		H256,
	},
};

#[derive(Clone)]
pub struct PrivateKey {
	inner: SecretKey,
}

impl PrivateKey {
	pub fn new(inner: SecretKey) -> Self {
		Self { inner }
	}
}

impl Key for PrivateKey {
	fn sign(
		&self,
		message: &[u8],
		chain_id: Option<u64>,
	) -> Result<signing::Signature, signing::SigningError> {
		let signature =
			self.inner.sign(message).map_err(|_| signing::SigningError::InvalidMessage)?;

		let standard_v = signature.v as u64;
		let v = if let Some(chain_id) = chain_id {
			standard_v + 35 + chain_id * 2
		} else {
			standard_v + 27
		};
		Ok(signing::Signature { r: H256::from(signature.r), s: H256::from(signature.s), v })
	}

	fn sign_message(&self, message: &[u8]) -> Result<signing::Signature, signing::SigningError> {
		let prefix_msg = "\x19Ethereum Signed Message:\n";
		let len_str = message.len().to_string();
		let mut res: Vec<u8> = Vec::new();
		res.append(&mut prefix_msg.as_bytes().to_vec());
		res.append(&mut len_str.as_bytes().to_vec());
		res.append(&mut message.to_vec());

		let mut keccak = Keccak::v256();
		let mut result = [0u8; 32];
		keccak.update(&res);
		keccak.finalize(&mut result);

		let signature =
			self.inner.sign(&result).map_err(|_| signing::SigningError::InvalidMessage)?;

		Ok(signing::Signature {
			r: H256::from(signature.r),
			s: H256::from(signature.s),
			v: signature.v as u64 + 27,
		})
	}

	fn address(&self) -> web3::types::Address {
		Address::from(self.inner.public().address())
	}
}

pub fn signature_to_bytes(s: Signature) -> Vec<u8> {
	let rb = s.r.to_fixed_bytes();
	let sb = s.s.to_fixed_bytes();
	let sv = s.v.to_be_bytes();

	let mut b = vec![];
	b.extend(&rb);
	b.extend(&sb);
	b.push(sv[sv.len() - 1]);

	b
}

pub fn signature_to_str(s: Signature) -> String {
	let bytes = signature_to_bytes(s);
	let bytes = bytes.as_slice();
	format!("0x{}", hex::encode(bytes))
}
