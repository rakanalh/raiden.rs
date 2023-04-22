use ethsign::SecretKey;
use raiden_primitives::types::{
	Bytes,
	Secret,
};
use rand::{
	distributions::Alphanumeric,
	thread_rng,
	Rng,
	RngCore,
};

use crate::constants::SECRET_LENGTH;

pub struct Generator;

impl Generator {
	pub fn random_key() -> SecretKey {
		let secret_bytes = Self::random_bytes();
		SecretKey::from_raw(&secret_bytes).expect("SecretKey should be generated")
	}

	pub fn random_secret() -> Secret {
		Bytes(
			thread_rng()
				.sample_iter(&Alphanumeric)
				.take(SECRET_LENGTH as usize)
				.collect::<Vec<u8>>(),
		)
	}

	pub fn random_bytes() -> [u8; 32] {
		let mut secret = [0u8; 32];
		thread_rng().fill_bytes(&mut secret);
		secret
	}
}
