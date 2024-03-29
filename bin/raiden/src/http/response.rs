use raiden_primitives::{
	serializers::{
		to_checksum_str,
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
		TokenNetworkRegistryAddress,
	},
};
use raiden_state_machine::{
	storage::{
		types::EventRecord,
		NaiveDateTime,
	},
	types::{
		ChannelState,
		ChannelStatus,
		ErrorPaymentSentFailed,
		Event,
		PaymentReceivedSuccess,
		PaymentSentSuccess,
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
	#[serde(serialize_with = "to_checksum_str")]
	pub token_address: TokenAddress,
	#[serde(serialize_with = "to_checksum_str")]
	pub token_network_address: TokenNetworkAddress,
	#[serde(serialize_with = "u256_to_str")]
	pub channel_identifier: ChannelIdentifier,
	#[serde(serialize_with = "to_checksum_str")]
	pub initiator: Address,
	#[serde(serialize_with = "to_checksum_str")]
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
	#[serde(serialize_with = "to_checksum_str")]
	token_network_address: TokenNetworkAddress,
	#[serde(serialize_with = "to_checksum_str")]
	token_address: TokenAddress,
	#[serde(serialize_with = "to_checksum_str")]
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

#[derive(Serialize)]
pub struct PaymentSuccess {
	#[serde(serialize_with = "to_checksum_str")]
	pub initiator_address: Address,
	#[serde(serialize_with = "to_checksum_str")]
	pub registry_address: TokenNetworkRegistryAddress,
	#[serde(serialize_with = "to_checksum_str")]
	pub token_address: TokenAddress,
	#[serde(serialize_with = "to_checksum_str")]
	pub target_address: Address,
	#[serde(serialize_with = "u256_to_str")]
	pub amount: TokenAmount,
	pub identifier: PaymentIdentifier,
	pub secret: String,
	pub secret_hash: String,
}

#[derive(Serialize)]
pub struct ResponsePaymentSentSuccess {
	pub event: String,
	pub identifier: Option<String>,
	pub log_time: Option<NaiveDateTime>,
	#[serde(serialize_with = "to_checksum_str")]
	pub token_address: Option<TokenAddress>,
	#[serde(serialize_with = "u256_to_str")]
	pub amount: TokenAmount,
	#[serde(serialize_with = "to_checksum_str")]
	pub target: Address,
}

impl From<PaymentSentSuccess> for ResponsePaymentSentSuccess {
	fn from(value: PaymentSentSuccess) -> Self {
		Self {
			event: "EventPaymentSentSuccess".to_owned(),
			identifier: None,
			log_time: None,
			token_address: None,
			amount: value.amount,
			target: value.target,
		}
	}
}

#[derive(Serialize)]
pub struct ResponsePaymentReceivedSuccess {
	pub event: String,
	pub identifier: Option<String>,
	pub log_time: Option<NaiveDateTime>,
	#[serde(serialize_with = "to_checksum_str")]
	pub token_address: Option<TokenAddress>,
	#[serde(serialize_with = "u256_to_str")]
	pub amount: TokenAmount,
	#[serde(serialize_with = "to_checksum_str")]
	pub initiator: Address,
}

impl From<PaymentReceivedSuccess> for ResponsePaymentReceivedSuccess {
	fn from(value: PaymentReceivedSuccess) -> Self {
		Self {
			event: "EventPaymentReceivedSuccess".to_owned(),
			identifier: None,
			log_time: None,
			token_address: None,
			amount: value.amount,
			initiator: value.initiator,
		}
	}
}

#[derive(Serialize)]
pub struct ResponsePaymentSentFailed {
	pub event: String,
	pub identifier: Option<String>,
	pub log_time: Option<NaiveDateTime>,
	#[serde(serialize_with = "to_checksum_str")]
	pub token_address: Option<TokenAddress>,
	pub reason: String,
	#[serde(serialize_with = "to_checksum_str")]
	pub target: Address,
}

impl From<ErrorPaymentSentFailed> for ResponsePaymentSentFailed {
	fn from(value: ErrorPaymentSentFailed) -> Self {
		Self {
			event: "EventPaymentSentFailed".to_owned(),
			identifier: None,
			log_time: None,
			token_address: None,
			reason: value.reason,
			target: value.target,
		}
	}
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ResponsePaymentHistory {
	SentFailed(ResponsePaymentSentFailed),
	SentSuccess(ResponsePaymentSentSuccess),
	ReceivedSuccess(ResponsePaymentReceivedSuccess),
}

#[derive(Serialize)]
pub struct ResponseEvent {
	pub event: Event,
	pub log_time: NaiveDateTime,
}

impl From<EventRecord> for ResponseEvent {
	fn from(value: EventRecord) -> Self {
		Self { event: value.data, log_time: value.timestamp }
	}
}
