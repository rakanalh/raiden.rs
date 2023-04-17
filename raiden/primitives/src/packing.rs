#![warn(clippy::missing_docs_in_private_items)]

use web3::{
	ethabi::{
		encode,
		Token,
	},
	types::U256,
};

use crate::types::{
	Address,
	BalanceHash,
	BlockExpiration,
	Bytes,
	CanonicalIdentifier,
	ChainID,
	MessageHash,
	MessageTypeId,
	Nonce,
	OneToNAddress,
	Signature,
	TokenAmount,
	TokenNetworkAddress,
};

pub fn pack_balance_proof(
	nonce: Nonce,
	balance_hash: BalanceHash,
	additional_hash: MessageHash,
	canonical_identifier: CanonicalIdentifier,
	msg_type: MessageTypeId,
) -> Bytes {
	let mut b = vec![];

	b.extend(canonical_identifier.token_network_address.as_bytes());
	b.extend(encode(&[Token::Uint(canonical_identifier.chain_identifier.into())]));
	b.extend(encode(&[Token::Uint(U256::from(msg_type as u8))]));
	b.extend(encode(&[Token::Uint(canonical_identifier.channel_identifier)]));
	b.extend(balance_hash.as_bytes());
	b.extend(encode(&[Token::Uint(nonce)]));
	b.extend(additional_hash.as_bytes());

	Bytes(b)
}

pub fn pack_balance_proof_message(
	nonce: Nonce,
	balance_hash: BalanceHash,
	additional_hash: MessageHash,
	canonical_identifier: CanonicalIdentifier,
	msg_type: MessageTypeId,
	partner_signature: Signature,
) -> Bytes {
	let mut b =
		pack_balance_proof(nonce, balance_hash, additional_hash, canonical_identifier, msg_type);

	b.0.extend(&partner_signature.0);

	b
}

pub fn pack_withdraw(
	canonical_identifier: CanonicalIdentifier,
	participant: Address,
	total_withdraw: TokenAmount,
	expiration_block: BlockExpiration,
) -> Bytes {
	let mut b = vec![];

	b.extend(canonical_identifier.token_network_address.as_bytes());
	b.extend(encode(&[Token::Uint(canonical_identifier.chain_identifier.into())]));
	b.extend(encode(&[Token::Uint(U256::from(MessageTypeId::Withdraw as u8))]));
	b.extend(encode(&[Token::Uint(canonical_identifier.channel_identifier)]));
	b.extend(participant.as_bytes());
	b.extend(encode(&[Token::Uint(total_withdraw)]));
	b.extend(encode(&[Token::Uint(expiration_block.into())]));

	Bytes(b)
}

pub fn pack_reward_proof(
	monitoring_service_contract_address: Address,
	chain_id: ChainID,
	token_network_address: TokenNetworkAddress,
	non_closing_participant: Address,
	non_closing_signature: Signature,
	reward_amount: TokenAmount,
) -> Bytes {
	let mut b = vec![];

	b.extend(monitoring_service_contract_address.as_bytes());
	b.extend(encode(&[Token::Uint(chain_id.into())]));
	b.extend(encode(&[Token::Uint(U256::from(MessageTypeId::MSReward as u8))]));
	b.extend(token_network_address.as_bytes());
	b.extend(non_closing_participant.as_bytes());
	b.extend(non_closing_signature.0);
	b.extend(encode(&[Token::Uint(reward_amount)]));

	Bytes(b)
}

pub fn pack_one_to_n_iou(
	one_to_n_address: OneToNAddress,
	sender: Address,
	receiver: Address,
	amount: TokenAmount,
	expiration_block: BlockExpiration,
	chain_id: ChainID,
) -> Bytes {
	let mut b = one_to_n_address.as_bytes().to_vec();
	b.extend(encode(&[Token::Uint(chain_id.into())]));
	b.extend(encode(&[Token::Uint(U256::from(MessageTypeId::IOU as u8))]));
	b.extend(sender.as_bytes());
	b.extend(receiver.as_bytes());
	b.extend(encode(&[Token::Uint(amount)]));
	b.extend(encode(&[Token::Uint(expiration_block.into())]));

	Bytes(b)
}
