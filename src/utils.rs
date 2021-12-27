use rand::distributions::Alphanumeric;
use rand::{
    thread_rng,
    Rng,
};

use crate::constants::SECRET_LENGTH;
use crate::primitives::PaymentIdentifier;

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
