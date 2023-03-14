use lazy_static::lazy_static;
use web3::signing::keccak256;

use crate::types::{
	Bytes,
	Locksroot,
};

lazy_static! {
	pub static ref EMPTY_SIGNATURE: Bytes = Bytes(vec![0; 65]);
	pub static ref LOCKSROOT_OF_NO_LOCKS: Locksroot = Locksroot::from_slice(&keccak256(&[]));
}
