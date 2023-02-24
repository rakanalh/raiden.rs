use raiden_blockchain::{
	keys::PrivateKey,
	signature::SignatureUtils,
};
use raiden_primitives::types::{
	Address,
	BlockExpiration,
	ChainID,
	OneToNAddress,
	TokenAmount,
	H256,
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::signing::{
	self,
	Key,
};

const IOU_MESSAGE_TYPE_ID: u8 = 5;

#[derive(Clone, Serialize, Deserialize)]
pub struct IOU {
	pub sender: Address,
	pub receiver: Address,
	pub one_to_n_address: OneToNAddress,
	pub amount: TokenAmount,
	pub expiration_block: BlockExpiration,
	pub chain_id: ChainID,
	pub signature: Option<H256>,
}

impl IOU {
	pub fn sign(&mut self, private_key: PrivateKey) -> Result<(), signing::SigningError> {
		let mut amount = [];
		self.amount.to_big_endian(&mut amount);
		let mut expiration_block = [];
		self.expiration_block.to_big_endian(&mut expiration_block);

		let chain_id: u64 = self.chain_id.clone().into();
		let chain_id_bytes: Vec<u8> = self.chain_id.clone().into();

		let mut message = vec![];
		message.extend_from_slice(self.one_to_n_address.as_bytes());
		message.extend(chain_id_bytes);
		message.push(IOU_MESSAGE_TYPE_ID);
		message.extend_from_slice(self.sender.as_bytes());
		message.extend_from_slice(self.receiver.as_bytes());
		message.extend(amount);
		message.extend(expiration_block);
		let signature = private_key.sign(&message, Some(chain_id))?;
		self.signature = Some(signature.to_h256());
		Ok(())
	}
}
