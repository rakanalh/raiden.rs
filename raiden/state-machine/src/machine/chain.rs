use raiden_primitives::types::{
	BlockNumber,
	SecretHash,
	TokenNetworkAddress,
	H256,
	U64,
};

use super::{
	initiator_manager,
	mediator,
	target,
};
use crate::{
	constants::CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
	errors::StateTransitionError,
	machine::{
		channel,
		token_network,
	},
	types::{
		ActionCancelPayment,
		ActionInitChain,
		ActionInitInitiator,
		ActionInitMediator,
		ActionInitTarget,
		ActionTransferReroute,
		Block,
		CanonicalIdentifier,
		ChainState,
		ContractReceiveChannelClosed,
		ContractReceiveTokenNetworkCreated,
		ContractReceiveTokenNetworkRegistry,
		ContractSendEvent,
		Event,
		InitiatorTask,
		MediatorTask,
		QueueIdentifier,
		ReceiveDelivered,
		ReceiveLockExpired,
		ReceiveProcessed,
		ReceiveSecretRequest,
		ReceiveSecretReveal,
		ReceiveTransferCancelRoute,
		ReceiveTransferRefund,
		ReceiveUnlock,
		ReceiveWithdrawConfirmation,
		ReceiveWithdrawExpired,
		ReceiveWithdrawRequest,
		SendMessageEvent,
		StateChange,
		TargetTask,
		TokenNetworkState,
		TransferRole,
		TransferTask,
		UpdateServicesAddresses,
		UpdatedServicesAddresses,
	},
	views,
};

type TransitionResult = std::result::Result<ChainTransition, StateTransitionError>;

pub struct ChainTransition {
	pub new_state: ChainState,
	pub events: Vec<Event>,
}

/// Check if the message exists in queue with ID `queueid` and exclude if found.
fn inplace_delete_message(queue: &mut Vec<SendMessageEvent>, state_change: &StateChange) {
	for (index, message) in queue.clone().iter().enumerate() {
		// A withdraw request is only confirmed by a withdraw confirmation.
		// This is done because Processed is not an indicator that the partner has
		// processed and **accepted** our withdraw request. Receiving
		// `Processed` here would cause the withdraw request to be removed
		// from the queue although the confirmation may have not been sent.
		// This is avoided by waiting for the confirmation before removing
		// the withdraw request.
		if let SendMessageEvent::SendWithdrawRequest(_) = message {
			if !matches!(state_change, StateChange::ReceiveWithdrawConfirmation(_)) {
				continue
			}
		}

		let message_type_identifier = match message {
			SendMessageEvent::SendLockExpired(message_inner) => message_inner.message_identifier,
			SendMessageEvent::SendLockedTransfer(message_inner) => message_inner.message_identifier,
			SendMessageEvent::SendSecretReveal(message_inner) => message_inner.message_identifier,
			SendMessageEvent::SendSecretRequest(message_inner) => message_inner.message_identifier,
			SendMessageEvent::SendUnlock(message_inner) => message_inner.message_identifier,
			SendMessageEvent::SendWithdrawRequest(message_inner) =>
				message_inner.message_identifier,
			SendMessageEvent::SendWithdrawConfirmation(message_inner) =>
				message_inner.message_identifier,
			SendMessageEvent::SendWithdrawExpired(message_inner) =>
				message_inner.message_identifier,
			SendMessageEvent::SendProcessed(message_inner) => message_inner.message_identifier,
		};
		let state_change_message_identifier = match state_change {
			StateChange::ReceiveDelivered(inner) => inner.message_identifier,
			StateChange::ReceiveProcessed(inner) => inner.message_identifier,
			StateChange::ReceiveTransferCancelRoute(inner) => inner.transfer.message_identifier,
			StateChange::ReceiveTransferRefund(inner) => inner.transfer.message_identifier,
			StateChange::ReceiveLockExpired(inner) => inner.message_identifier,
			StateChange::ReceiveUnlock(inner) => inner.message_identifier,
			StateChange::ReceiveWithdrawRequest(inner) => inner.message_identifier,
			StateChange::ReceiveWithdrawConfirmation(inner) => inner.message_identifier,
			StateChange::ReceiveWithdrawExpired(inner) => inner.message_identifier,
			_ => 0,
		};
		if message_type_identifier == state_change_message_identifier {
			queue.remove(index);
		}
	}
}

fn inplace_delete_message_queue(
	chain_state: &mut ChainState,
	state_change: &StateChange,
	queue_id: &QueueIdentifier,
) {
	let mut queue = match chain_state.queueids_to_queues.get(&queue_id) {
		Some(queue) => queue.clone(),
		None => return,
	};

	if queue.is_empty() {
		chain_state.queueids_to_queues.remove(&queue_id);
	}

	inplace_delete_message(&mut queue, state_change);

	if queue.is_empty() {
		chain_state.queueids_to_queues.remove(&queue_id);
		return
	}
	chain_state.queueids_to_queues.insert(queue_id.clone(), queue);
}

fn subdispatch_by_canonical_id(
	chain_state: &mut ChainState,
	state_change: StateChange,
	canonical_identifier: CanonicalIdentifier,
) -> TransitionResult {
	let token_network_registries = &mut chain_state.identifiers_to_tokennetworkregistries;
	let token_network = match token_network_registries
		.values_mut()
		.map(|tnr| tnr.tokennetworkaddresses_to_tokennetworks.values_mut())
		.flatten()
		.find(|tn| tn.address == canonical_identifier.token_network_address)
	{
		Some(tn) => tn,
		None => return Ok(ChainTransition { new_state: chain_state.clone(), events: vec![] }),
	};

	let transition = token_network::state_transition(
		token_network.clone(),
		state_change,
		chain_state.block_number,
		chain_state.block_hash,
		&mut chain_state.pseudo_random_number_generator,
	)?;

	*token_network = transition.new_state;
	let events = transition.events;

	Ok(ChainTransition { new_state: chain_state.clone(), events })
}

fn subdispatch_to_all_channels(
	mut chain_state: ChainState,
	state_change: StateChange,
	block_number: U64,
	block_hash: H256,
) -> TransitionResult {
	let mut events = vec![];

	for (_, token_network_registry) in chain_state.identifiers_to_tokennetworkregistries.iter_mut()
	{
		for (_, token_network) in
			token_network_registry.tokennetworkaddresses_to_tokennetworks.iter_mut()
		{
			for (_, channel_state) in token_network.channelidentifiers_to_channels.iter_mut() {
				let result = channel::state_transition(
					channel_state.clone(),
					state_change.clone(),
					block_number,
					block_hash,
					&mut chain_state.pseudo_random_number_generator,
				)?;

				if let Some(new_state) = result.new_state {
					*channel_state = new_state;
				}
				events.extend(result.events);
			}
		}
	}

	Ok(ChainTransition { new_state: chain_state, events })
}

fn subdispatch_to_payment_task(
	mut chain_state: ChainState,
	state_change: StateChange,
	secrethash: SecretHash,
) -> TransitionResult {
	let mut events = vec![];

	if let Some(sub_task) =
		chain_state.payment_mapping.secrethashes_to_task.get(&secrethash).cloned()
	{
		match sub_task {
			TransferTask::Initiator(mut initiator) => {
				let sub_iteration = initiator_manager::state_transition(
					chain_state,
					Some(initiator.manager_state.clone()),
					state_change,
				)?;
				chain_state = sub_iteration.chain_state;
				if let Some(new_state) = sub_iteration.new_state {
					initiator.manager_state = new_state;
					chain_state
						.payment_mapping
						.secrethashes_to_task
						.insert(secrethash, TransferTask::Initiator(initiator));
				} else {
					chain_state.payment_mapping.secrethashes_to_task.remove(&secrethash);
				}
				events.extend(sub_iteration.events);
			},
			TransferTask::Mediator(mut mediator) => {
				let sub_iteration = mediator::state_transition(
					chain_state,
					Some(mediator.mediator_state.clone()),
					state_change,
				)?;
				chain_state = sub_iteration.chain_state;
				if let Some(new_state) = sub_iteration.new_state {
					mediator.mediator_state = new_state;
					chain_state
						.payment_mapping
						.secrethashes_to_task
						.insert(secrethash, TransferTask::Mediator(mediator));
				} else {
					chain_state.payment_mapping.secrethashes_to_task.remove(&secrethash);
				}
				events.extend(sub_iteration.events);
			},
			TransferTask::Target(mut target) => {
				let sub_iteration =
					target::state_transition(chain_state, Some(target.target_state), state_change)?;
				chain_state = sub_iteration.chain_state;
				if let Some(new_state) = sub_iteration.new_state {
					target.target_state = new_state;
					chain_state
						.payment_mapping
						.secrethashes_to_task
						.insert(secrethash, TransferTask::Target(target));
				} else {
					chain_state.payment_mapping.secrethashes_to_task.remove(&secrethash);
				}
				events.extend(sub_iteration.events);
			},
		}
	}

	Ok(ChainTransition { new_state: chain_state, events })
}

fn subdispatch_to_all_lockedtransfers(
	mut chain_state: ChainState,
	state_change: StateChange,
) -> TransitionResult {
	let mut events = vec![];

	let payment_mapping = chain_state.payment_mapping.clone();
	for secrethash in payment_mapping.secrethashes_to_task.keys() {
		let result =
			subdispatch_to_payment_task(chain_state.clone(), state_change.clone(), *secrethash)?;
		chain_state = result.new_state;
		events.extend(result.events);
	}

	Ok(ChainTransition { new_state: chain_state, events })
}

fn subdispatch_initiator_task(
	mut chain_state: ChainState,
	state_change: ActionInitInitiator,
) -> TransitionResult {
	let token_network_state = match views::get_token_network_by_address(
		&chain_state,
		state_change.transfer.token_network_address,
	) {
		Some(tn) => tn.clone(),
		None => return Ok(ChainTransition { new_state: chain_state, events: vec![] }),
	};

	let manager_state = match chain_state
		.payment_mapping
		.secrethashes_to_task
		.get(&state_change.transfer.secrethash)
	{
		Some(sub_task) => {
			let initiator = match sub_task {
				TransferTask::Initiator(initiator)
					if token_network_state.address == initiator.token_network_address =>
					initiator,
				_ => return Ok(ChainTransition { new_state: chain_state, events: vec![] }),
			};
			Some(initiator.manager_state.clone())
		},
		None => None,
	};

	if manager_state.is_some() {
		return Ok(ChainTransition { new_state: chain_state, events: vec![] })
	}

	let initiator_state = initiator_manager::state_transition(
		chain_state.clone(),
		manager_state,
		state_change.clone().into(),
	)?;

	match initiator_state.new_state {
		Some(initiator_state) => {
			chain_state.payment_mapping.secrethashes_to_task.insert(
				state_change.transfer.secrethash,
				TransferTask::Initiator(InitiatorTask {
					role: TransferRole::Initiator,
					token_network_address: token_network_state.address.clone(),
					manager_state: initiator_state,
				}),
			);
		},
		None => {
			chain_state
				.payment_mapping
				.secrethashes_to_task
				.remove(&state_change.transfer.secrethash);
		},
	}

	Ok(ChainTransition { new_state: initiator_state.chain_state, events: initiator_state.events })
}

fn subdispatch_mediator_task(
	chain_state: ChainState,
	state_change: ActionInitMediator,
	token_network_address: TokenNetworkAddress,
	secrethash: SecretHash,
) -> TransitionResult {
	let mediator_state = match chain_state.payment_mapping.secrethashes_to_task.get(&secrethash) {
		Some(sub_task) => match sub_task {
			TransferTask::Mediator(mediator_task) => Some(mediator_task.mediator_state.clone()),
			_ => return Ok(ChainTransition { new_state: chain_state, events: vec![] }),
		},
		None => None,
	};

	let mut events = vec![];
	let iteration = mediator::state_transition(chain_state, mediator_state, state_change.into())?;
	events.extend(iteration.events);

	let mut chain_state = iteration.chain_state;

	if let Some(new_state) = iteration.new_state {
		let mediator_task = MediatorTask {
			role: TransferRole::Mediator,
			token_network_address,
			mediator_state: new_state,
		};
		chain_state
			.payment_mapping
			.secrethashes_to_task
			.insert(secrethash, TransferTask::Mediator(mediator_task));
	} else if chain_state.payment_mapping.secrethashes_to_task.contains_key(&secrethash) {
		chain_state.payment_mapping.secrethashes_to_task.remove(&secrethash);
	}

	Ok(ChainTransition { new_state: chain_state, events })
}

fn subdispatch_target_task(
	chain_state: ChainState,
	state_change: ActionInitTarget,
	token_network_address: TokenNetworkAddress,
	secrethash: SecretHash,
) -> TransitionResult {
	let target_state = match chain_state.payment_mapping.secrethashes_to_task.get(&secrethash) {
		Some(sub_task) => match sub_task {
			TransferTask::Target(target_task) => Some(target_task.target_state.clone()),
			_ => return Ok(ChainTransition { new_state: chain_state, events: vec![] }),
		},
		None => None,
	};

	let mut events = vec![];

	let iteration = target::state_transition(chain_state, target_state, state_change.into())?;
	events.extend(iteration.events);

	let mut chain_state = iteration.chain_state;

	if let Some(new_state) = iteration.new_state {
		let target_task = TargetTask {
			role: TransferRole::Target,
			token_network_address,
			target_state: new_state,
		};
		chain_state
			.payment_mapping
			.secrethashes_to_task
			.insert(secrethash, TransferTask::Target(target_task));
	} else if chain_state.payment_mapping.secrethashes_to_task.contains_key(&secrethash) {
		chain_state.payment_mapping.secrethashes_to_task.remove(&secrethash);
	}

	Ok(ChainTransition { new_state: chain_state, events })
}

fn handle_action_init_chain(state_change: ActionInitChain) -> TransitionResult {
	Ok(ChainTransition {
		new_state: ChainState::new(
			state_change.chain_id,
			state_change.block_number,
			state_change.block_hash,
			state_change.our_address,
		),
		events: vec![],
	})
}

fn handle_action_init_intiator(
	chain_state: ChainState,
	state_change: ActionInitInitiator,
) -> TransitionResult {
	subdispatch_initiator_task(chain_state, state_change)
}

fn handle_action_init_mediator(
	chain_state: ChainState,
	state_change: ActionInitMediator,
) -> TransitionResult {
	let transfer = &state_change.from_transfer;
	let secrethash = transfer.lock.secrethash;
	let token_network_address = transfer.balance_proof.canonical_identifier.token_network_address;

	subdispatch_mediator_task(chain_state, state_change, token_network_address, secrethash)
}

fn handle_action_init_target(
	chain_state: ChainState,
	state_change: ActionInitTarget,
) -> TransitionResult {
	let transfer = &state_change.transfer;
	let secrethash = transfer.lock.secrethash;
	let token_network_address = transfer.balance_proof.canonical_identifier.token_network_address;

	subdispatch_target_task(chain_state, state_change, token_network_address, secrethash)
}

fn handle_action_transfer_reroute(
	mut chain_state: ChainState,
	state_change: ActionTransferReroute,
) -> TransitionResult {
	let new_secrethash = state_change.secrethash;

	if let Some(current_payment_task) = chain_state
		.payment_mapping
		.secrethashes_to_task
		.get(&state_change.transfer.lock.secrethash)
		.cloned()
	{
		chain_state
			.payment_mapping
			.secrethashes_to_task
			.insert(new_secrethash, current_payment_task.clone());
	}

	subdispatch_to_payment_task(chain_state, state_change.into(), new_secrethash)
}

fn handle_action_cancel_payment(
	chain_state: ChainState,
	_state_change: ActionCancelPayment,
) -> TransitionResult {
	Ok(ChainTransition { new_state: chain_state, events: vec![] })
}

fn handle_new_block(mut chain_state: ChainState, state_change: Block) -> TransitionResult {
	chain_state.block_number = state_change.block_number;
	chain_state.block_hash = state_change.block_hash;

	let channels_result = subdispatch_to_all_channels(
		chain_state.clone(),
		state_change.clone().into(),
		chain_state.block_number,
		chain_state.block_hash,
	)?;

	let mut events = channels_result.events;

	chain_state = channels_result.new_state;

	let transfers_result = subdispatch_to_all_lockedtransfers(chain_state, state_change.into())?;
	events.extend(transfers_result.events);

	chain_state = transfers_result.new_state;

	Ok(ChainTransition { new_state: chain_state, events })
}

fn handle_contract_receive_token_network_registry(
	mut chain_state: ChainState,
	state_change: ContractReceiveTokenNetworkRegistry,
) -> TransitionResult {
	chain_state
		.identifiers_to_tokennetworkregistries
		.entry(state_change.token_network_registry.address)
		.or_insert(state_change.token_network_registry);

	Ok(ChainTransition { new_state: chain_state, events: vec![] })
}

fn handle_contract_receive_token_network_created(
	mut chain_state: ChainState,
	state_change: ContractReceiveTokenNetworkCreated,
) -> TransitionResult {
	let token_network_registries = &mut chain_state.identifiers_to_tokennetworkregistries;
	let token_network_registry =
		match token_network_registries.get_mut(&state_change.token_network_registry_address) {
			Some(token_network_registry) => token_network_registry,
			None =>
				return Err(StateTransitionError {
					msg: format!(
						"Token network registry {} was not found",
						state_change.token_network_registry_address
					),
				}),
		};

	token_network_registry
		.tokennetworkaddresses_to_tokennetworks
		.insert(state_change.token_network.address, state_change.token_network.clone());
	token_network_registry.tokenaddresses_to_tokennetworkaddresses.insert(
		state_change.token_network.token_address.clone(),
		state_change.token_network.address.clone(),
	);

	Ok(ChainTransition { new_state: chain_state, events: vec![] })
}

fn handle_token_network_state_change(
	mut chain_state: ChainState,
	token_network_address: TokenNetworkAddress,
	state_change: StateChange,
	block_number: U64,
	block_hash: H256,
) -> TransitionResult {
	let token_network_state = match views::get_token_network(&chain_state, &token_network_address) {
		Some(token_network_state) => token_network_state,
		None =>
			return Err(StateTransitionError {
				msg: format!("Token network {} was not found", token_network_address,),
			}),
	};

	let transition = token_network::state_transition(
		token_network_state.clone(),
		state_change,
		block_number,
		block_hash,
		&mut chain_state.pseudo_random_number_generator,
	)?;

	let new_state: TokenNetworkState = transition.new_state;
	let registry_address =
		views::get_token_network_registry_by_token_network_address(&chain_state, new_state.address)
			.unwrap()
			.address;
	let registry = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&registry_address)
		.unwrap();
	registry
		.tokennetworkaddresses_to_tokennetworks
		.insert(new_state.address, new_state);

	Ok(ChainTransition { new_state: chain_state, events: transition.events })
}

fn handle_contract_receive_channel_closed(
	mut chain_state: ChainState,
	state_change: ContractReceiveChannelClosed,
	block_number: U64,
	block_hash: H256,
) -> TransitionResult {
	let token_network_address = state_change.canonical_identifier.token_network_address;
	if let Some(channel_state) = views::get_channel_by_canonical_identifier(
		&chain_state,
		state_change.canonical_identifier.clone(),
	) {
		let queue_identifier = QueueIdentifier {
			recipient: channel_state.partner_state.address,
			canonical_identifier: state_change.canonical_identifier.clone(),
		};
		chain_state.queueids_to_queues.remove(&queue_identifier);
	}
	handle_token_network_state_change(
		chain_state,
		token_network_address,
		state_change.into(),
		block_number,
		block_hash,
	)
}

fn handle_receive_transfer_cancel_route(
	chain_state: ChainState,
	state_change: ReceiveTransferCancelRoute,
) -> TransitionResult {
	let secrethash = state_change.transfer.lock.secrethash;
	subdispatch_to_payment_task(chain_state, state_change.into(), secrethash)
}

fn handle_receive_secret_reveal(
	chain_state: ChainState,
	state_change: ReceiveSecretReveal,
) -> TransitionResult {
	let secrethash = state_change.secrethash;
	subdispatch_to_payment_task(chain_state, state_change.into(), secrethash)
}

fn handle_receive_secret_request(
	chain_state: ChainState,
	state_change: ReceiveSecretRequest,
) -> TransitionResult {
	let secrethash = state_change.secrethash;
	subdispatch_to_payment_task(chain_state, state_change.into(), secrethash)
}

fn handle_receive_lock_expired(
	chain_state: ChainState,
	state_change: ReceiveLockExpired,
) -> TransitionResult {
	let secrethash = state_change.secrethash;
	subdispatch_to_payment_task(chain_state, state_change.into(), secrethash)
}

fn handle_receive_transfer_refund(
	chain_state: ChainState,
	state_change: ReceiveTransferRefund,
) -> TransitionResult {
	let secrethash = state_change.transfer.lock.secrethash;
	subdispatch_to_payment_task(chain_state, state_change.into(), secrethash)
}

fn handle_receive_unlock(chain_state: ChainState, state_change: ReceiveUnlock) -> TransitionResult {
	let secrethash = state_change.secrethash;
	subdispatch_to_payment_task(chain_state, state_change.into(), secrethash)
}

fn handle_receive_withdraw_request(
	mut chain_state: ChainState,
	state_change: ReceiveWithdrawRequest,
) -> TransitionResult {
	let canonical_identifier = state_change.canonical_identifier.clone();
	subdispatch_by_canonical_id(&mut chain_state, state_change.into(), canonical_identifier)
}

fn handle_receive_withdraw_confirmation(
	mut chain_state: ChainState,
	state_change: ReceiveWithdrawConfirmation,
) -> TransitionResult {
	let canonical_identifier = state_change.canonical_identifier.clone();
	let iteration = subdispatch_by_canonical_id(
		&mut chain_state,
		state_change.clone().into(),
		canonical_identifier,
	)?;

	let mut chain_state = iteration.new_state;
	for queue_id in chain_state.queueids_to_queues.clone().keys() {
		inplace_delete_message_queue(&mut chain_state, &state_change.clone().into(), queue_id);
	}

	Ok(ChainTransition { new_state: chain_state, events: iteration.events })
}

fn handle_receive_withdraw_expired(
	mut chain_state: ChainState,
	state_change: ReceiveWithdrawExpired,
) -> TransitionResult {
	let canonical_identifier = state_change.canonical_identifier.clone();
	subdispatch_by_canonical_id(&mut chain_state, state_change.into(), canonical_identifier)
}

fn handle_receive_delivered(
	mut chain_state: ChainState,
	state_change: ReceiveDelivered,
) -> TransitionResult {
	let queue_id = QueueIdentifier {
		recipient: state_change.sender,
		canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
	};
	inplace_delete_message_queue(&mut chain_state, &state_change.into(), &queue_id);
	Ok(ChainTransition { new_state: chain_state, events: vec![] })
}

fn handle_receive_processed(
	mut chain_state: ChainState,
	state_change: ReceiveProcessed,
) -> TransitionResult {
	for queue_id in chain_state.queueids_to_queues.clone().keys() {
		inplace_delete_message_queue(&mut chain_state, &state_change.clone().into(), queue_id);
	}

	Ok(ChainTransition { new_state: chain_state, events: vec![] })
}

fn handle_update_services_addresses(
	chain_state: ChainState,
	state_change: UpdateServicesAddresses,
) -> TransitionResult {
	let event = UpdatedServicesAddresses {
		service_address: state_change.service,
		validity: state_change.valid_til,
	};
	Ok(ChainTransition { new_state: chain_state, events: vec![event.into()] })
}

/// True if the side-effect of `transaction` is satisfied by
/// `state_change`.
///
/// This predicate is used to clear the transaction queue. This should only be
/// done once the expected side effect of a transaction is achieved. This
/// doesn't necessarily mean that the transaction sent by *this* node was
/// mined, but only that *some* transaction which achieves the same side-effect
/// was successfully executed and mined. This distinction is important for
/// restarts and to reduce the number of state changes.
///
/// On restarts: The state of the on-chain channel could have changed while the
/// node was offline. Once the node learns about the change (e.g. the channel
/// was settled), new transactions can be dispatched by Raiden as a side effect for the
/// on-chain *event* (e.g. do the batch unlock with the latest pending locks),
/// but the dispatched transaction could have been completed by another agent (e.g.
/// the partner node). For these cases, the transaction from a different
/// address which achieves the same side-effect is sufficient, otherwise
/// unnecessary transactions would be sent by the node.
///
/// NOTE: The above is not important for transactions sent as a side-effect for
/// a new *block*. On restart the node first synchronizes its state by querying
/// for new events, only after the off-chain state is up-to-date, a Block state
/// change is dispatched. At this point some transactions are not required
/// anymore and therefore are not dispatched.
///
/// On the number of state changes: Accepting a transaction from another
/// address removes the need for clearing state changes, e.g. when our
/// node's close transaction fails but its partner's close transaction
/// succeeds.
fn is_transaction_effect_satisfied(
	chain_state: &ChainState,
	transaction: &ContractSendEvent,
	state_change: &StateChange,
) -> bool {
	// These transactions are not made atomic through the WAL. They are sent
	// exclusively through the external APIs.
	//
	//  - ContractReceiveChannelNew
	//  - ContractReceiveChannelDeposit
	//  - ContractReceiveNewTokenNetworkRegistry
	//  - ContractReceiveNewTokenNetwork
	//  - ContractReceiveRouteNew
	//
	// Note: Deposits and Withdraws must consider a transaction with a higher
	// value as sufficient, because the values are monotonically increasing and
	// the transaction with a lower value will never be executed.

	// Transactions are used to change the on-chain state of a channel. It
	// doesn't matter if the sender of the transaction is the local node or
	// another node authorized to perform the operation. So, for the following
	// transactions, as long as the side-effects are the same, the local
	// transaction can be removed from the queue.
	//
	// - An update transfer can be done by a trusted third party (i.e. monitoring service)
	// - A close transaction can be sent by our partner
	// - A settle transaction can be sent by anyone
	// - A secret reveal can be done by anyone

	// - A lower nonce is not a valid replacement, since that is an older balance proof
	// - A larger raiden state change nonce is impossible. That would require the partner node to
	//   produce an invalid balance proof, and this node to accept the invalid balance proof and
	//   sign it

	if let StateChange::ContractReceiveUpdateTransfer(update_transfer_state_change) = state_change {
		if let ContractSendEvent::ContractSendChannelUpdateTransfer(update_transfer_event) =
			transaction
		{
			if update_transfer_state_change.canonical_identifier ==
				update_transfer_event.balance_proof.canonical_identifier &&
				update_transfer_state_change.nonce == update_transfer_event.balance_proof.nonce
			{
				return true
			}
		}
	}

	if let StateChange::ContractReceiveChannelClosed(channel_closed_state_change) = state_change {
		if let ContractSendEvent::ContractSendChannelClose(channel_close_event) = transaction {
			if channel_closed_state_change.canonical_identifier ==
				channel_close_event.canonical_identifier
			{
				return true
			}
		}
	}

	if let StateChange::ContractReceiveChannelSettled(channel_settled_state_change) = state_change {
		if let ContractSendEvent::ContractSendChannelSettle(channel_settle_event) = transaction {
			if channel_settled_state_change.canonical_identifier ==
				channel_settle_event.canonical_identifier
			{
				return true
			}
		}
	}

	if let StateChange::ContractReceiveSecretReveal(secret_reveal_state_change) = state_change {
		if let ContractSendEvent::ContractSendSecretReveal(secret_reveal_event) = transaction {
			if secret_reveal_state_change.secret == secret_reveal_event.secret {
				return true
			}
		}
	}

	if let StateChange::ContractReceiveChannelBatchUnlock(batch_unlock_state_change) = state_change
	{
		if let ContractSendEvent::ContractSendChannelBatchUnlock(_) = transaction {
			let our_address = chain_state.our_address;
			let mut partner_address = None;
			if batch_unlock_state_change.receiver == our_address {
				partner_address = Some(batch_unlock_state_change.sender);
			} else if batch_unlock_state_change.sender == our_address {
				partner_address = Some(batch_unlock_state_change.receiver);
			}

			if let Some(partner_address) = partner_address {
				let channel_state = views::get_channel_by_token_network_and_partner(
					chain_state,
					batch_unlock_state_change.canonical_identifier.token_network_address,
					partner_address,
				);
				if channel_state.is_none() {
					return true
				}
			}
		}
	}

	false
}

fn is_transaction_invalidated(transaction: &ContractSendEvent, state_change: &StateChange) -> bool {
	if let StateChange::ContractReceiveChannelSettled(channel_settled) = state_change {
		if let ContractSendEvent::ContractSendChannelUpdateTransfer(update_transfer) = transaction {
			if channel_settled.canonical_identifier ==
				update_transfer.balance_proof.canonical_identifier
			{
				return true
			}
		}
	}

	if let StateChange::ContractReceiveChannelClosed(channel_closed) = state_change {
		if let ContractSendEvent::ContractSendChannelWithdraw(channel_withdraw) = transaction {
			if channel_closed.canonical_identifier == channel_withdraw.canonical_identifier {
				return true
			}
		}
	}

	false
}

fn is_transaction_expired(transaction: &ContractSendEvent, block_number: BlockNumber) -> bool {
	if let ContractSendEvent::ContractSendChannelUpdateTransfer(update_transfer) = transaction {
		if update_transfer.expiration < block_number {
			return true
		}
	}

	if let ContractSendEvent::ContractSendSecretReveal(secret_reveal) = transaction {
		if secret_reveal.expiration < block_number {
			return true
		}
	}

	false
}

fn is_transaction_pending(
	chain_state: &ChainState,
	transaction: &ContractSendEvent,
	state_change: &StateChange,
) -> bool {
	!(is_transaction_effect_satisfied(chain_state, transaction, state_change) ||
		is_transaction_invalidated(transaction, state_change) ||
		is_transaction_expired(transaction, chain_state.block_number))
}

fn update_queues(iteration: &mut ChainTransition, state_change: StateChange) {
	let chain_state = &mut iteration.new_state;
	match state_change {
		StateChange::ContractReceiveChannelOpened(_) |
		StateChange::ContractReceiveChannelClosed(_) |
		StateChange::ContractReceiveChannelSettled(_) |
		StateChange::ContractReceiveChannelDeposit(_) |
		StateChange::ContractReceiveChannelWithdraw(_) |
		StateChange::ContractReceiveChannelBatchUnlock(_) |
		StateChange::ContractReceiveSecretReveal(_) |
		StateChange::ContractReceiveRouteNew(_) |
		StateChange::ContractReceiveUpdateTransfer(_) => {
			let mut pending_transactions = chain_state.pending_transactions.clone();
			pending_transactions.retain(|transaction| {
				is_transaction_pending(chain_state, transaction, &state_change)
			});
			chain_state.pending_transactions = pending_transactions;
		},
		_ => {},
	};

	for event in &iteration.events {
		match event {
			Event::ContractSendChannelClose(_) |
			Event::ContractSendChannelWithdraw(_) |
			Event::ContractSendChannelSettle(_) |
			Event::ContractSendChannelUpdateTransfer(_) |
			Event::ContractSendChannelBatchUnlock(_) |
			Event::ContractSendSecretReveal(_) => {
				chain_state
					.pending_transactions
					.push(event.clone().try_into().expect("Should work"));
			},
			_ => {},
		}

		let queue_identifier = match event {
			Event::SendWithdrawExpired(inner) => inner.inner.queue_identifier(),
			Event::SendWithdrawRequest(inner) => inner.inner.queue_identifier(),
			Event::SendLockedTransfer(inner) => inner.inner.queue_identifier(),
			Event::SendLockExpired(inner) => inner.inner.queue_identifier(),
			Event::SendSecretRequest(inner) => inner.inner.queue_identifier(),
			Event::SendSecretReveal(inner) => inner.inner.queue_identifier(),
			Event::SendUnlock(inner) => inner.inner.queue_identifier(),
			Event::SendProcessed(inner) => inner.inner.queue_identifier(),
			_ => continue,
		};
		let queue =
			chain_state.queueids_to_queues.entry(queue_identifier).or_insert_with(|| vec![]);
		queue.push(event.clone().try_into().expect("Should work"));
	}
}

pub fn state_transition(
	mut chain_state: ChainState,
	state_change: StateChange,
) -> TransitionResult {
	let update_queues_state_change = state_change.clone();
	let mut iteration = match state_change {
		StateChange::ActionInitChain(inner) => handle_action_init_chain(inner),
		StateChange::ActionInitInitiator(inner) => handle_action_init_intiator(chain_state, inner),
		StateChange::ActionInitMediator(inner) => handle_action_init_mediator(chain_state, inner),
		StateChange::ActionInitTarget(inner) => handle_action_init_target(chain_state, inner),
		StateChange::ActionChannelWithdraw(ref inner) => subdispatch_by_canonical_id(
			&mut chain_state,
			state_change.clone(),
			inner.canonical_identifier.clone(),
		),
		StateChange::ActionChannelSetRevealTimeout(ref inner) => subdispatch_by_canonical_id(
			&mut chain_state,
			state_change.clone(),
			inner.canonical_identifier.clone(),
		),
		StateChange::ActionTransferReroute(inner) =>
			handle_action_transfer_reroute(chain_state, inner),
		StateChange::ActionCancelPayment(inner) => handle_action_cancel_payment(chain_state, inner),
		StateChange::ActionChannelClose(ref inner) => {
			let token_network_address = inner.canonical_identifier.token_network_address;
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			handle_token_network_state_change(
				chain_state,
				token_network_address,
				state_change,
				block_number,
				block_hash,
			)
		},
		StateChange::ActionChannelCoopSettle(ref inner) => {
			let canonical_identifier = inner.canonical_identifier.clone();
			subdispatch_by_canonical_id(&mut chain_state, state_change, canonical_identifier)
		},
		StateChange::Block(inner) => handle_new_block(chain_state, inner),
		StateChange::ContractReceiveTokenNetworkRegistry(inner) =>
			handle_contract_receive_token_network_registry(chain_state, inner),
		StateChange::ContractReceiveTokenNetworkCreated(inner) =>
			handle_contract_receive_token_network_created(chain_state, inner),
		StateChange::ContractReceiveChannelOpened(ref inner) => {
			let token_network_address =
				inner.channel_state.canonical_identifier.token_network_address;
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			handle_token_network_state_change(
				chain_state,
				token_network_address,
				state_change,
				block_number,
				block_hash,
			)
		},
		StateChange::ContractReceiveChannelClosed(inner) => {
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			handle_contract_receive_channel_closed(chain_state, inner, block_number, block_hash)
		},
		StateChange::ContractReceiveChannelSettled(ref inner) => {
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			let token_network_address = inner.canonical_identifier.token_network_address;
			handle_token_network_state_change(
				chain_state.clone(),
				token_network_address,
				state_change,
				block_number,
				block_hash,
			)
		},
		StateChange::ContractReceiveChannelDeposit(ref inner) => {
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			let token_network_address = inner.canonical_identifier.token_network_address;
			handle_token_network_state_change(
				chain_state.clone(),
				token_network_address,
				state_change,
				block_number,
				block_hash,
			)
		},
		StateChange::ContractReceiveChannelWithdraw(ref inner) => {
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			let token_network_address = inner.canonical_identifier.token_network_address;
			handle_token_network_state_change(
				chain_state.clone(),
				token_network_address,
				state_change,
				block_number,
				block_hash,
			)
		},
		StateChange::ContractReceiveChannelBatchUnlock(ref inner) => {
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			let token_network_address = inner.canonical_identifier.token_network_address;
			handle_token_network_state_change(
				chain_state.clone(),
				token_network_address,
				state_change,
				block_number,
				block_hash,
			)
		},
		StateChange::ContractReceiveUpdateTransfer(ref inner) => {
			let block_number = chain_state.block_number;
			let block_hash = chain_state.block_hash;
			let token_network_address = inner.canonical_identifier.token_network_address;
			handle_token_network_state_change(
				chain_state,
				token_network_address,
				state_change,
				block_number,
				block_hash,
			)
		},
		StateChange::ContractReceiveSecretReveal(ref inner) =>
			subdispatch_to_payment_task(chain_state, state_change.clone(), inner.secrethash),
		StateChange::ContractReceiveRouteNew(_) =>
			Ok(ChainTransition { new_state: chain_state, events: vec![] }),
		StateChange::ReceiveTransferCancelRoute(inner) =>
			handle_receive_transfer_cancel_route(chain_state, inner),
		StateChange::ReceiveSecretReveal(inner) => handle_receive_secret_reveal(chain_state, inner),
		StateChange::ReceiveSecretRequest(inner) =>
			handle_receive_secret_request(chain_state, inner),
		StateChange::ReceiveLockExpired(inner) => handle_receive_lock_expired(chain_state, inner),
		StateChange::ReceiveTransferRefund(inner) =>
			handle_receive_transfer_refund(chain_state, inner),
		StateChange::ReceiveUnlock(inner) => handle_receive_unlock(chain_state, inner),
		StateChange::ReceiveWithdrawRequest(inner) =>
			handle_receive_withdraw_request(chain_state, inner),
		StateChange::ReceiveWithdrawConfirmation(inner) =>
			handle_receive_withdraw_confirmation(chain_state, inner),
		StateChange::ReceiveWithdrawExpired(inner) =>
			handle_receive_withdraw_expired(chain_state, inner),
		StateChange::ReceiveDelivered(inner) => handle_receive_delivered(chain_state, inner),
		StateChange::ReceiveProcessed(inner) => handle_receive_processed(chain_state, inner),
		StateChange::UpdateServicesAddresses(inner) =>
			handle_update_services_addresses(chain_state, inner),
	}?;

	update_queues(&mut iteration, update_queues_state_change);

	Ok(iteration)
}
