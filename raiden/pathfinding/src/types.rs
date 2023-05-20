use raiden_blockchain::keys::PrivateKey;
use raiden_primitives::{
	deserializers::u256_from_str,
	packing::pack_one_to_n_iou,
	serializers::{
		to_checksum_str,
		u256_to_str,
	},
	traits::{
		Checksum,
		ToBytes,
	},
	types::{
		Address,
		BlockExpiration,
		Bytes,
		ChainID,
		OneToNAddress,
		TokenAmount,
	},
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::{
	self,
	Key,
};

#[derive(Copy, Clone, PartialEq)]
pub enum RoutingMode {
	PFS,
	Private,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IOU {
	#[serde(serialize_with = "to_checksum_str")]
	pub sender: Address,
	#[serde(serialize_with = "to_checksum_str")]
	pub receiver: Address,
	#[serde(serialize_with = "to_checksum_str")]
	pub one_to_n_address: OneToNAddress,
	#[serde(serialize_with = "u256_to_str", deserialize_with = "u256_from_str")]
	pub amount: TokenAmount,
	pub expiration_block: BlockExpiration,
	pub chain_id: ChainID,
	pub signature: Option<Bytes>,
}

impl IOU {
	pub fn sign(&mut self, private_key: PrivateKey) -> Result<(), signing::SigningError> {
		let data = pack_one_to_n_iou(
			self.one_to_n_address,
			self.sender,
			self.receiver,
			self.amount,
			self.expiration_block,
			self.chain_id,
		);
		let signature = private_key.sign_message(&data.0)?;
		self.signature = Some(Bytes(signature.to_bytes()));
		Ok(())
	}
}

impl ToString for IOU {
	fn to_string(&self) -> String {
		format!(
			"IOU (sender = {}, receiver = {}, amount = {}, expiration = {})",
			self.sender.checksum(),
			self.receiver.checksum(),
			self.amount.to_string(),
			self.expiration_block.to_string()
		)
	}
}
