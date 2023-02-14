use raiden_blockchain::{
	keys::PrivateKey,
	signature::SignatureUtils,
};
use raiden_state_machine::types::{
	BlockExpiration,
	ChainID,
	OneToNAddress,
	TokenAmount,
};
use serde::{
	Deserialize,
	Serialize,
};
use web3::{
	signing::{
		self,
		Key,
	},
	types::{
		Address,
		H256,
	},
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

		let mut message = vec![];
		message.extend_from_slice(self.one_to_n_address.as_bytes());
		message.push(self.chain_id.clone() as u8);
		message.push(IOU_MESSAGE_TYPE_ID);
		message.extend_from_slice(self.sender.as_bytes());
		message.extend_from_slice(self.receiver.as_bytes());
		message.extend(amount);
		message.extend(expiration_block);
		let signature = private_key.sign(&message, Some(self.chain_id.clone() as u64))?;
		self.signature = Some(signature.to_h256());
		Ok(())
	}
}
