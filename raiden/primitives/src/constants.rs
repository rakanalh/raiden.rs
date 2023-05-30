#![warn(clippy::missing_docs_in_private_items)]

use std::ops::Mul;

use lazy_static::lazy_static;
use web3::signing::keccak256;

use crate::types::{
	Address,
	BlockTimeout,
	Bytes,
	CanonicalIdentifier,
	ChainID,
	ChannelIdentifier,
	Locksroot,
	TokenAmount,
	U256,
};

pub const CANONICAL_IDENTIFIER_UNORDERED_QUEUE: CanonicalIdentifier = CanonicalIdentifier {
	chain_identifier: ChainID::Private(U256::zero()),
	token_network_address: Address::zero(),
	channel_identifier: ChannelIdentifier::zero(),
};

lazy_static! {
	pub static ref EMPTY_SIGNATURE: Bytes = Bytes(vec![0; 65]);
	pub static ref LOCKSROOT_OF_NO_LOCKS: Locksroot = Locksroot::from_slice(&keccak256(&[]));
	pub static ref MONITORING_REWARD: TokenAmount =
		TokenAmount::from(10).pow(TokenAmount::from(18)) * TokenAmount::from(80);
	pub static ref PFS_DEFAULT_MAX_PATHS: usize = 3;
	pub static ref PFS_DEFAULT_MAX_FEE: TokenAmount =
		TokenAmount::from(10).pow(TokenAmount::from(16)).mul(TokenAmount::from(5));
	pub static ref PFS_DEFAULT_IOU_TIMEOUT: BlockTimeout =
		Into::<BlockTimeout>::into(BlockTimeout::from(10).pow(BlockTimeout::from(5).into()))
			.mul(BlockTimeout::from(2));
}
