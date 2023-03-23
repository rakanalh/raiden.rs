use raiden_primitives::{
	deserializers::{
		u256_from_optional_str,
		u256_from_str,
	},
	types::{
		Address,
		BlockTimeout,
		PaymentIdentifier,
		RevealTimeout,
		SecretHash,
		SettleTimeout,
		TokenAddress,
		TokenAmount,
	},
};
use raiden_state_machine::types::ChannelStatus;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ChannelOpenParams {
	pub registry_address: Address,
	pub partner_address: Address,
	pub token_address: TokenAddress,
	pub settle_timeout: Option<SettleTimeout>,
	pub reveal_timeout: Option<RevealTimeout>,
	#[serde(deserialize_with = "u256_from_optional_str")]
	pub total_deposit: Option<TokenAmount>,
}

#[derive(Deserialize)]
pub struct ChannelPatchParams {
	pub registry_address: Address,
	pub token_address: TokenAddress,
	pub partner_address: Address,
	#[serde(default)]
	#[serde(deserialize_with = "u256_from_optional_str")]
	pub total_deposit: Option<TokenAmount>,
	#[serde(default)]
	#[serde(deserialize_with = "u256_from_optional_str")]
	pub total_withdraw: Option<TokenAmount>,
	pub reveal_timeout: Option<RevealTimeout>,
	pub state: Option<ChannelStatus>,
}

#[derive(Deserialize)]
pub struct InitiatePaymentParams {
	#[serde(deserialize_with = "u256_from_str")]
	pub amount: TokenAmount,
	pub payment_identifier: Option<PaymentIdentifier>,
	pub secret: Option<String>,
	pub secret_hash: Option<SecretHash>,
	pub lock_timeout: Option<BlockTimeout>,
}
