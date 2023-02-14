use raiden_state_machine::{
	constants::SECRET_LENGTH,
	types::PaymentIdentifier,
};
use rand::{
	distributions::Alphanumeric,
	thread_rng,
	Rng,
};

pub fn random_identifier() -> PaymentIdentifier {
	let mut rng = rand::thread_rng();
	rng.gen_range(1..std::u64::MAX).into()
}

pub fn random_secret() -> String {
	thread_rng()
		.sample_iter(&Alphanumeric)
		.take(SECRET_LENGTH as usize)
		.map(char::from)
		.collect()
}
