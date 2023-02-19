use std::ops::{
	Div,
	Sub,
};

use web3::types::{
	Address,
	Bytes,
	H256,
	U256,
};

use crate::{
	constants::DEFAULT_REVEAL_TIMEOUT,
	machine::chain,
	tests::factories::{
		chain_state_with_token_network,
		channel_state,
	},
	types::{
		ActionChannelSetRevealTimeout,
		ActionChannelWithdraw,
		BalanceProofState,
		Block,
		CanonicalIdentifier,
		ContractReceiveChannelBatchUnlock,
		ContractReceiveChannelClosed,
		ContractReceiveChannelDeposit,
		ContractReceiveChannelSettled,
		ContractReceiveChannelWithdraw,
		ContractSendChannelBatchUnlock,
		ContractSendChannelSettle,
		ContractSendChannelUpdateTransfer,
		ContractSendEventInner,
		ErrorInvalidActionSetRevealTimeout,
		ErrorInvalidActionWithdraw,
		MediationFeeConfig,
		PendingWithdrawState,
		SendMessageEventInner,
		SendWithdrawExpired,
		SendWithdrawRequest,
		TransactionChannelDeposit,
		TransactionExecutionStatus,
		TransactionHash,
		TransactionResult,
		U64,
	},
	views,
};

#[test]
fn test_open_channel_new_block_with_expired_withdraws() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);
	let channel_identifier = U256::from(1u64);

	let mut chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier =
		CanonicalIdentifier { chain_identifier, token_network_address, channel_identifier };

	let token_network_registry_state = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&channel_identifier)
		.expect("Channel should exist");

	channel_state.our_state.withdraws_pending.insert(
		U256::from(100u64),
		PendingWithdrawState {
			total_withdraw: U256::from(100u64),
			expiration: U64::from(50u64),
			nonce: U256::from(1u64),
			recipient_metadata: None,
		},
	);

	let expected_event = SendWithdrawExpired {
		inner: SendMessageEventInner {
			recipient: channel_state.partner_state.address.clone(),
			recipient_metadata: None,
			canonical_identifier: canonical_identifier.clone(),
			message_identifier: 1,
		},
		participant: channel_state.our_state.address.clone(),
		nonce: U256::from(1u64),
		expiration: U64::from(50u64),
		total_withdraw: U256::from(100u64),
	};
	let state_change = Block {
		block_number: U64::from(511u64),
		block_hash: H256::random(),
		gas_limit: U256::zero(),
	};
	let result = chain::state_transition(chain_state.clone(), state_change.into())
		.expect("Block should succeed");

	assert!(!result.events.is_empty());
	assert_eq!(result.events[0], expected_event.into())
}

#[test]
fn test_closed_channel_new_block() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);
	let channel_identifier = U256::from(1u64);
	let mut chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier =
		CanonicalIdentifier { chain_identifier, token_network_address, channel_identifier };

	let token_network_registry_state = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&token_network_address)
		.expect("token network should exist");
	let mut channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&channel_identifier)
		.expect("Channel should exist");

	channel_state.close_transaction = Some(TransactionExecutionStatus {
		started_block_number: Some(U64::from(10u64)),
		finished_block_number: Some(U64::from(10u64)),
		result: Some(TransactionResult::Success),
	});

	let block_hash = H256::random();
	let state_change =
		Block { block_number: U64::from(511u64), block_hash, gas_limit: U256::zero() };
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Block should succeed");

	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ContractSendChannelSettle {
			inner: ContractSendEventInner { triggered_by_blockhash: block_hash },
			canonical_identifier: canonical_identifier.clone(),
		}
		.into()
	);
}

#[test]
fn test_channel_opened() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let chain_identifier = chain_state.chain_id.clone();
	let channel_state = views::get_channel_by_canonical_identifier(
		&chain_state,
		CanonicalIdentifier { chain_identifier, token_network_address, channel_identifier },
	);

	assert!(channel_state.is_some());
}

#[test]
fn test_channel_closed() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let state_change = ContractReceiveChannelClosed {
		transaction_hash: Some(H256::random()),
		block_number: U64::from(10u64),
		block_hash: H256::random(),
		transaction_from: Address::random(),
		canonical_identifier: canonical_identifier.clone(),
	};

	let result = chain::state_transition(chain_state.clone(), state_change.clone().into())
		.expect("Should close channel");
	assert!(result.events.is_empty());

	let channel_identifier = U256::from(2u64);
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let mut chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let balance_proof_state = BalanceProofState {
		nonce: U256::from(1u64),
		transferred_amount: U256::zero(),
		locked_amount: U256::zero(),
		locksroot: Bytes::default(),
		canonical_identifier: canonical_identifier.clone(),
		balance_hash: H256::zero(),
		message_hash: Some(H256::zero()),
		signature: None,
		sender: Some(Address::zero()),
	};

	let token_network_registry_state = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&token_network_address)
		.expect("token network should exist");
	let mut channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&channel_identifier)
		.expect("Channel should exist");
	channel_state.partner_state.balance_proof = Some(balance_proof_state.clone());

	let state_change = ContractReceiveChannelClosed {
		transaction_hash: Some(H256::random()),
		block_number: U64::from(10u64),
		block_hash: H256::zero(),
		transaction_from: Address::random(),
		canonical_identifier: canonical_identifier.clone(),
	};

	let result = chain::state_transition(chain_state.clone(), state_change.clone().into())
		.expect("Should close channel");

	let event = ContractSendChannelUpdateTransfer {
		inner: ContractSendEventInner { triggered_by_blockhash: H256::zero() },
		expiration: U64::from(510u64),
		balance_proof: balance_proof_state,
	};
	assert!(!result.events.is_empty());
	assert_eq!(result.events[0], event.into());
}

#[test]
fn test_channel_withdraw() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel should exist");

	assert_eq!(channel_state.our_state.contract_balance, U256::zero());

	let state_change = ContractReceiveChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		participant: chain_state.our_address.clone(),
		total_withdraw: U256::from(100u64),
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: U64::from(1u64),
		block_hash: H256::zero(),
	};
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Withdraw should succeed");
	let chain_state = result.new_state;
	let channel_state = views::get_channel_by_canonical_identifier(
		&chain_state.clone(),
		canonical_identifier.clone(),
	)
	.expect("Channel should exist")
	.clone();
	assert_eq!(channel_state.our_state.onchain_total_withdraw, U256::from(100u64));

	let state_change = ContractReceiveChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		participant: channel_state.partner_state.address,
		total_withdraw: U256::from(99u64),
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: U64::from(1u64),
		block_hash: H256::zero(),
	};
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Withdraw should succeed");
	let chain_state = result.new_state;
	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel should exist");
	assert_eq!(channel_state.partner_state.onchain_total_withdraw, U256::from(99u64));
}

#[test]
fn test_channel_deposit() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel should exist");

	assert_eq!(channel_state.our_state.contract_balance, U256::zero());

	let state_change = ContractReceiveChannelDeposit {
		canonical_identifier: canonical_identifier.clone(),
		deposit_transaction: TransactionChannelDeposit {
			participant_address: chain_state.our_address.clone(),
			contract_balance: U256::from(100u64),
			deposit_block_number: U64::from(10u64),
		},
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: U64::from(1u64),
		block_hash: H256::zero(),
	};
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Deposit should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone())
			.expect("Channel should exist");
	assert_eq!(channel_state.our_state.contract_balance, U256::from(100u64));

	let chain_state = result.new_state;
	let state_change = ContractReceiveChannelDeposit {
		canonical_identifier: canonical_identifier.clone(),
		deposit_transaction: TransactionChannelDeposit {
			participant_address: chain_state.our_address.clone(),
			contract_balance: U256::from(99u64), // Less than the deposit before
			deposit_block_number: U64::from(10u64),
		},
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: U64::from(1u64),
		block_hash: H256::zero(),
	};
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Deposit should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier)
			.expect("Channel should exist");
	assert_eq!(channel_state.our_state.contract_balance, U256::from(100u64));
}

#[test]
fn test_channel_settled() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let block_hash = H256::random();
	let our_locksroot = Bytes(vec![1u8; 32]);

	let state_change = ContractReceiveChannelSettled {
		transaction_hash: Some(H256::random()),
		block_number: U64::from(1u64),
		block_hash,
		canonical_identifier: canonical_identifier.clone(),
		our_onchain_locksroot: Bytes::default(),
		partner_onchain_locksroot: Bytes::default(),
	};
	let result = chain::state_transition(chain_state.clone(), state_change.into())
		.expect("Channel settled should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone());
	assert_eq!(channel_state, None);

	let state_change = ContractReceiveChannelSettled {
		transaction_hash: Some(H256::random()),
		block_number: U64::from(1u64),
		block_hash,
		canonical_identifier: canonical_identifier.clone(),
		our_onchain_locksroot: our_locksroot.clone(),
		partner_onchain_locksroot: Bytes::default(),
	};
	let result = chain::state_transition(chain_state.clone(), state_change.into())
		.expect("Channel settled should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone())
			.expect("channel should exist");

	assert!(!result.events.is_empty());
	assert_eq!(channel_state.our_state.onchain_locksroot, our_locksroot);
	assert_eq!(
		result.events[0],
		ContractSendChannelBatchUnlock {
			inner: ContractSendEventInner { triggered_by_blockhash: block_hash },
			canonical_identifier: canonical_identifier.clone(),
			sender: channel_state.partner_state.address,
		}
		.into()
	)
}

#[test]
fn test_channel_batch_unlock() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let mut chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let token_network_registry_state = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&token_network_address)
		.expect("token network should exist");
	let mut channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&channel_identifier)
		.expect("Channel should exist");

	channel_state.settle_transaction = Some(TransactionExecutionStatus {
		started_block_number: Some(U64::from(1u64)),
		finished_block_number: Some(U64::from(2u64)),
		result: Some(TransactionResult::Success),
	});

	let state_change = ContractReceiveChannelBatchUnlock {
		canonical_identifier: canonical_identifier.clone(),
		receiver: channel_state.our_state.address,
		sender: channel_state.partner_state.address,
		locksroot: Bytes::default(),
		unlocked_amount: U256::from(100u64),
		returned_tokens: U256::zero(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: U64::from(1u64),
		block_hash: H256::zero(),
	};
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Should succeeed");

	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone());
	assert_eq!(channel_state, None);
}

#[test]
fn test_channel_action_withdraw() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let mut chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	// Withdraw with insufficient onchain balance
	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: U256::from(100u64),
		recipient_metadata: None,
	};
	let result = chain::state_transition(chain_state.clone(), state_change.into())
		.expect("action withdraw should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionWithdraw {
			attemped_withdraw: U256::from(100u64),
			reason: format!("Insufficient balance: 0. Requested 100 for withdraw"),
		}
		.into()
	);
	// Withdraw a zero amount
	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: U256::zero(),
		recipient_metadata: None,
	};
	let result = chain::state_transition(chain_state.clone(), state_change.into())
		.expect("action withdraw should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionWithdraw {
			attemped_withdraw: U256::zero(),
			reason: format!("Total withdraw 0 did not increase"),
		}
		.into()
	);

	let token_network_registry_state = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&token_network_address)
		.expect("token network should exist");
	let mut channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&channel_identifier)
		.expect("Channel should exist");

	channel_state.close_transaction = Some(TransactionExecutionStatus {
		started_block_number: Some(U64::from(1u64)),
		finished_block_number: Some(U64::from(1u64)),
		result: Some(TransactionResult::Success),
	});
	// Withdraw on a closed channel
	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: U256::from(100u64),
		recipient_metadata: None,
	};
	let result = chain::state_transition(chain_state, state_change.into())
		.expect("action withdraw should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionWithdraw {
			attemped_withdraw: U256::from(100u64),
			reason: format!("Invalid withdraw, the channel is not opened"),
		}
		.into()
	);

	let mut chain_state = result.new_state;
	let token_network_registry_state = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&token_network_address)
		.expect("token network should exist");
	let mut channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&channel_identifier)
		.expect("Channel should exist");

	// Successful withdraw
	channel_state.close_transaction = None;
	channel_state.our_state.contract_balance = U256::from(200u64);

	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: U256::from(100u64),
		recipient_metadata: None,
	};
	let result = chain::state_transition(chain_state, state_change.into())
		.expect("action withdraw should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone())
			.expect("Channel should exist");

	assert!(!channel_state.our_state.withdraws_pending.is_empty());
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		SendWithdrawRequest {
			inner: SendMessageEventInner {
				recipient: channel_state.partner_state.address,
				recipient_metadata: None,
				canonical_identifier: canonical_identifier.clone(),
				message_identifier: 1, // Doesn't matter
			},
			participant: channel_state.our_state.address,
			nonce: channel_state.our_state.nonce,
			expiration: U64::from(101u64),
			coop_settle: false,
		}
		.into()
	)
}

#[test]
fn test_channel_set_reveal_timeout() {
	let token_network_registry_address = Address::random();
	let token_address = Address::random();
	let token_network_address = Address::random();

	let chain_state = chain_state_with_token_network(
		token_network_registry_address,
		token_address,
		token_network_address,
	);

	let channel_identifier = U256::from(1u64);
	let chain_identifier = chain_state.chain_id.clone();
	let canonical_identifier = CanonicalIdentifier {
		chain_identifier: chain_identifier.clone(),
		token_network_address,
		channel_identifier,
	};
	let chain_state = channel_state(
		chain_state,
		token_network_registry_address,
		token_network_address,
		token_address,
		channel_identifier,
	);

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel state should exist");

	assert_eq!(channel_state.reveal_timeout, U64::from(DEFAULT_REVEAL_TIMEOUT));

	let state_change = ActionChannelSetRevealTimeout {
		canonical_identifier: canonical_identifier.clone(),
		reveal_timeout: U64::from(6u64),
	};
	let result = chain::state_transition(chain_state.clone(), state_change.into())
		.expect("Set reveal timeout should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionSetRevealTimeout {
			reveal_timeout: U64::from(6u64),
			reason: format!("Settle timeout should be at least twice as large as reveal timeout"),
		}
		.into()
	);

	let reveal_timeout = channel_state.settle_timeout.div(2).sub(1).as_u64();
	let state_change = ActionChannelSetRevealTimeout {
		canonical_identifier: canonical_identifier.clone(),
		reveal_timeout: U64::from(reveal_timeout),
	};
	let result = chain::state_transition(chain_state, state_change.into())
		.expect("Set reveal timeout should succeed");
	assert!(result.events.is_empty());
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone())
			.expect("Channel state should exist");

	assert_eq!(channel_state.reveal_timeout, U64::from(reveal_timeout));
}
