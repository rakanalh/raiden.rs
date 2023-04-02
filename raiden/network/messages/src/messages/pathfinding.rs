use chrono::{
	NaiveDateTime,
	Utc,
};
use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	serializers::u256_to_str,
	traits::ToBytes,
	types::{
		Address,
		CanonicalIdentifier,
		Nonce,
		RevealTimeout,
		Signature,
		TokenAmount,
	},
};
use raiden_state_machine::{
	types::{
		ChannelState,
		FeeScheduleState,
	},
	views,
};
use serde::{
	Deserialize,
	Serialize,
};
use serde_rlp::ser::to_bytes as rlp_to_bytes;
use web3::signing::SigningError;

use super::SignedMessage;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct PFSCapacityUpdate {
	pub canonical_identifier: CanonicalIdentifier,
	pub updating_participant: Address,
	pub other_participant: Address,
	#[serde(serialize_with = "u256_to_str")]
	pub updating_nonce: Nonce,
	#[serde(serialize_with = "u256_to_str")]
	pub other_nonce: Nonce,
	#[serde(serialize_with = "u256_to_str")]
	pub updating_capacity: TokenAmount,
	#[serde(serialize_with = "u256_to_str")]
	pub other_capacity: TokenAmount,
	pub reveal_timeout: RevealTimeout,
	pub signature: Signature,
}

impl From<ChannelState> for PFSCapacityUpdate {
	fn from(channel: ChannelState) -> Self {
		Self {
			canonical_identifier: channel.canonical_identifier,
			updating_participant: channel.our_state.address,
			other_participant: channel.partner_state.address,
			updating_nonce: channel.our_state.nonce,
			other_nonce: channel.partner_state.nonce,
			updating_capacity: views::channel_distributable(
				&channel.our_state,
				&channel.partner_state,
			),
			other_capacity: views::channel_distributable(
				&channel.partner_state,
				&channel.our_state,
			),
			reveal_timeout: channel.reveal_timeout,
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for PFSCapacityUpdate {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let chain_id: Vec<u8> = self.canonical_identifier.chain_identifier.into();

		let mut channel_identifier = [0u8; 32];
		self.canonical_identifier
			.channel_identifier
			.to_big_endian(&mut channel_identifier);

		let mut updating_nonce = [0u8; 32];
		self.updating_nonce.to_big_endian(&mut updating_nonce);

		let mut other_nonce = [0u8; 32];
		self.other_nonce.to_big_endian(&mut other_nonce);

		let mut updating_capacity = [0u8; 32];
		self.updating_capacity.to_big_endian(&mut updating_capacity);

		let mut other_capacity = [0u8; 32];
		self.other_capacity.to_big_endian(&mut other_capacity);

		let mut bytes = vec![];
		bytes.extend_from_slice(&chain_id);
		bytes.extend_from_slice(self.canonical_identifier.token_network_address.as_bytes());
		bytes.extend_from_slice(&channel_identifier);
		bytes.extend_from_slice(self.updating_participant.as_bytes());
		bytes.extend_from_slice(self.other_participant.as_bytes());
		bytes.extend_from_slice(&updating_nonce);
		bytes.extend_from_slice(&other_nonce);
		bytes.extend_from_slice(&updating_capacity);
		bytes.extend_from_slice(&other_capacity);
		bytes.extend_from_slice(&self.reveal_timeout.as_bytes());
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct PFSFeeUpdate {
	pub canonical_identifier: CanonicalIdentifier,
	pub updating_participant: Address,
	pub fee_schedule: FeeScheduleState,
	pub timestamp: NaiveDateTime,
	pub signature: Signature,
}

impl From<ChannelState> for PFSFeeUpdate {
	fn from(channel: ChannelState) -> Self {
		Self {
			canonical_identifier: channel.canonical_identifier,
			updating_participant: channel.our_state.address,
			fee_schedule: channel.fee_schedule,
			timestamp: Utc::now().naive_local(),
			signature: Signature::default(),
		}
	}
}

impl SignedMessage for PFSFeeUpdate {
	fn bytes_to_sign(&self) -> Vec<u8> {
		let chain_id: Vec<u8> = self.canonical_identifier.chain_identifier.into();

		let mut channel_identifier = [0u8; 32];
		self.canonical_identifier
			.channel_identifier
			.to_big_endian(&mut channel_identifier);

		let imbalance_penalty =
			if let Some(imbalance_penalty) = &self.fee_schedule.imbalance_penalty {
				let imbalance_penalty: Vec<_> =
					imbalance_penalty.iter().map(|(a, b)| (a.as_u128(), b.as_u128())).collect();
				rlp_to_bytes(&imbalance_penalty).expect("Should be able to serialize")
			} else {
				rlp_to_bytes(&0u64).unwrap()
			};

		let mut bytes = vec![];
		bytes.extend_from_slice(&chain_id);
		bytes.extend_from_slice(self.canonical_identifier.token_network_address.as_bytes());
		bytes.extend_from_slice(&channel_identifier);
		bytes.extend_from_slice(self.updating_participant.as_bytes());
		bytes.push(self.fee_schedule.cap_fees as u8);
		bytes.extend_from_slice(&self.fee_schedule.flat.to_bytes());
		bytes.extend_from_slice(&self.fee_schedule.proportional.to_bytes());
		bytes.extend_from_slice(&imbalance_penalty);
		bytes.extend_from_slice(
			&serde_json::to_string(&self.timestamp)
				.expect("Serialize date/time should work")
				.into_bytes(),
		);
		bytes
	}

	fn sign(&mut self, key: PrivateKey) -> Result<(), SigningError> {
		self.signature = self.sign_message(key)?.to_bytes().into();
		Ok(())
	}
}
