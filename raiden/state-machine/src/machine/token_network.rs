use raiden_primitives::types::{
	H256,
	U256,
	U64,
};

use super::channel;
use crate::{
	errors::StateTransitionError,
	types::{
		ContractReceiveChannelOpened,
		Event,
		Random,
		StateChange,
		TokenNetworkState,
	},
};

type TransitionResult = std::result::Result<TokenNetworkTransition, StateTransitionError>;

#[derive(Debug)]
pub struct TokenNetworkTransition {
	pub new_state: TokenNetworkState,
	pub events: Vec<Event>,
}

fn subdispatch_to_channel_by_id(
	mut token_network_state: TokenNetworkState,
	channel_identifier: U256,
	state_change: StateChange,
	block_number: U64,
	block_hash: H256,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	let channel_state =
		match token_network_state.channelidentifiers_to_channels.get(&channel_identifier) {
			Some(channel_state) => channel_state.clone(),
			None =>
				return Ok(TokenNetworkTransition { new_state: token_network_state, events: vec![] }),
		};

	let result = channel::state_transition(
		channel_state.clone(),
		state_change,
		block_number,
		block_hash,
		pseudo_random_number_generator,
	)?;
	match result.new_state {
		Some(channel_state) => {
			token_network_state
				.channelidentifiers_to_channels
				.insert(channel_identifier, channel_state);
		},
		None => {
			token_network_state.channelidentifiers_to_channels.remove(&channel_identifier);

			token_network_state
				.partneraddresses_to_channelidentifiers
				.remove(&channel_state.partner_state.address);
		},
	}

	Ok(TokenNetworkTransition { new_state: token_network_state, events: result.events })
}

fn handle_contract_receive_channel_opened(
	mut token_network_state: TokenNetworkState,
	state_change: ContractReceiveChannelOpened,
) -> TransitionResult {
	let channel_state = state_change.channel_state;
	let canonical_identifier = channel_state.canonical_identifier.clone();

	token_network_state
		.partneraddresses_to_channelidentifiers
		.entry(channel_state.partner_state.address)
		.or_insert(vec![]);
	if let Some(entry) = token_network_state
		.partneraddresses_to_channelidentifiers
		.get_mut(&channel_state.partner_state.address)
	{
		entry.push(canonical_identifier.channel_identifier);
	}

	token_network_state
		.channelidentifiers_to_channels
		.entry(canonical_identifier.channel_identifier)
		.or_insert(channel_state);

	Ok(TokenNetworkTransition { new_state: token_network_state, events: vec![] })
}

pub fn state_transition(
	token_network_state: TokenNetworkState,
	state_change: StateChange,
	block_number: U64,
	block_hash: H256,
	pseudo_random_number_generator: &mut Random,
) -> TransitionResult {
	match state_change {
		StateChange::ActionChannelClose(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ActionChannelWithdraw(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ActionChannelCoopSettle(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ActionChannelSetRevealTimeout(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ContractReceiveChannelOpened(inner) =>
			handle_contract_receive_channel_opened(token_network_state, inner),
		StateChange::ContractReceiveChannelClosed(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ContractReceiveChannelDeposit(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ContractReceiveChannelWithdraw(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ContractReceiveChannelSettled(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ContractReceiveUpdateTransfer(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ContractReceiveChannelBatchUnlock(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ReceiveWithdrawRequest(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ReceiveWithdrawConfirmation(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		StateChange::ReceiveWithdrawExpired(ref inner) => {
			let channel_identifier = inner.canonical_identifier.channel_identifier;
			subdispatch_to_channel_by_id(
				token_network_state,
				channel_identifier,
				state_change,
				block_number,
				block_hash,
				pseudo_random_number_generator,
			)
		},
		_ => Err(StateTransitionError { msg: String::from("Could not transition token network") }),
	}
}
