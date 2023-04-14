use raiden_primitives::{
	serializers::{
		to_checksummed_str,
		u256_to_str,
	},
	types::{
		ChannelIdentifier,
		PaymentIdentifier,
		RevealTimeout,
		SettleTimeout,
		TokenAddress,
		TokenAmount,
		TokenNetworkAddress,
	},
};
use raiden_state_machine::{
	types::{
		ChannelState,
		ChannelStatus,
	},
	views,
};
use serde::Serialize;
use web3::types::{
	Address,
	U256,
};

#[derive(Serialize)]
pub struct AddressResponse {
	pub our_address: Address,
}

#[derive(Serialize)]
pub struct VersionResponse {
	pub version: Option<&'static str>,
}

#[derive(Serialize)]
pub struct SettingsResponse {
	pub pathfinding_service_address: String,
}

#[derive(Serialize)]
pub struct ConnectionManager {
	#[serde(serialize_with = "u256_to_str")]
	pub sum_deposits: TokenAmount,
	pub channels: u32,
}

#[derive(Serialize)]
pub struct TransferView {
	pub payment_identifier: PaymentIdentifier,
	#[serde(serialize_with = "to_checksummed_str")]
	pub token_address: TokenAddress,
	#[serde(serialize_with = "to_checksummed_str")]
	pub token_network_address: TokenNetworkAddress,
	#[serde(serialize_with = "u256_to_str")]
	pub channel_identifier: ChannelIdentifier,
	#[serde(serialize_with = "to_checksummed_str")]
	pub initiator: Address,
	#[serde(serialize_with = "to_checksummed_str")]
	pub target: Address,
	#[serde(serialize_with = "u256_to_str")]
	pub transferred_amount: TokenAmount,
	#[serde(serialize_with = "u256_to_str")]
	pub locked_amount: TokenAmount,
	pub role: String,
}

#[derive(Serialize)]
pub struct ChannelResponse {
	#[serde(serialize_with = "u256_to_str")]
	channel_identifier: U256,
	#[serde(serialize_with = "to_checksummed_str")]
	token_network_address: TokenNetworkAddress,
	#[serde(serialize_with = "to_checksummed_str")]
	token_address: TokenAddress,
	#[serde(serialize_with = "to_checksummed_str")]
	partner_address: Address,
	settle_timeout: SettleTimeout,
	reveal_timeout: RevealTimeout,
	#[serde(serialize_with = "u256_to_str")]
	balance: TokenAmount,
	state: ChannelStatus,
	#[serde(serialize_with = "u256_to_str")]
	total_deposit: TokenAmount,
	#[serde(serialize_with = "u256_to_str")]
	total_withdraw: TokenAmount,
}

#[derive(Serialize)]
pub struct CreateChannelResponse {
	token_address: TokenAddress,
	partner_address: Address,
	reveal_timeout: RevealTimeout,
	settle_timeout: SettleTimeout,
	total_deposit: TokenAmount,
}

impl From<ChannelState> for ChannelResponse {
	fn from(channel: ChannelState) -> Self {
		ChannelResponse {
			channel_identifier: channel.canonical_identifier.channel_identifier,
			token_network_address: channel.canonical_identifier.token_network_address,
			token_address: channel.token_address,
			partner_address: channel.partner_state.address,
			settle_timeout: channel.settle_timeout,
			reveal_timeout: channel.reveal_timeout,
			total_deposit: channel.our_state.contract_balance,
			total_withdraw: channel.our_state.total_withdraw(),
			state: channel.status(),
			balance: views::channel_balance(&channel.our_state, &channel.partner_state),
		}
	}
}
