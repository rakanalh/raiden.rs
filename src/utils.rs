use rand::distributions::Alphanumeric;
use rand::{
    thread_rng,
    Rng,
};

use crate::constants::SECRET_LENGTH;

pub fn random_identifier() -> u64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(1..std::u64::MAX)
}

pub fn random_secret() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(SECRET_LENGTH as usize)
        .map(char::from)
        .collect()
}
