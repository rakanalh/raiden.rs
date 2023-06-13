use ethsign::{
	PublicKey,
	SecretKey,
};
use raiden_primitives::types::Address;

pub const ALICE: &str = "ALICE";
pub const BOB: &str = "BOB";
pub const CHARLIE: &str = "CHARLIE";

pub enum Keyring {
	Alice,
	Bob,
	Charlie,
}

impl Keyring {
	pub fn private_key(&self) -> SecretKey {
		let mut secret: [u8; 32] = [0; 32];
		let s = match self {
			Self::Alice => ALICE.as_bytes(),
			Self::Bob => BOB.as_bytes(),
			Self::Charlie => CHARLIE.as_bytes(),
		};
		secret[..s.len()].copy_from_slice(s);
		SecretKey::from_raw(&secret).expect("Private key generation should not fail")
	}

	pub fn public_key(&self) -> PublicKey {
		self.private_key().public()
	}

	pub fn address(&self) -> Address {
		Address::from_slice(self.public_key().address())
	}
}
