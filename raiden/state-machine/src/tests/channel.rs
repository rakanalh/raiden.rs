use std::ops::{
	Div,
	Sub,
};

use raiden_primitives::{
	constants::LOCKSROOT_OF_NO_LOCKS,
	types::{
		Address,
		BalanceHash,
		BlockExpiration,
		BlockHash,
		BlockNumber,
		GasLimit,
		Locksroot,
		MessageHash,
		Nonce,
		RevealTimeout,
		TokenAmount,
		TransactionHash,
	},
};

use crate::{
	constants::DEFAULT_REVEAL_TIMEOUT,
	machine::{
		chain,
		channel::utils::compute_locksroot,
		utils::update_channel,
	},
	tests::factories::{
		ChainStateBuilder,
		Keyring,
	},
	types::{
		ActionChannelSetRevealTimeout,
		ActionChannelWithdraw,
		BalanceProofState,
		Block,
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
		PendingLocksState,
		PendingWithdrawState,
		SendMessageEventInner,
		SendWithdrawExpired,
		SendWithdrawRequest,
		TransactionChannelDeposit,
		TransactionExecutionStatus,
		TransactionResult,
	},
	views,
};

#[test]
fn test_open_channel_new_block_with_expired_withdraws() {
	let mut chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let token_network_registry_state = chain_info
		.chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&chain_info.token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&chain_info.token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&canonical_identifier.channel_identifier)
		.expect("Channel should exist");

	channel_state.our_state.contract_balance = TokenAmount::from(1000);
	channel_state.our_state.withdraws_pending.insert(
		TokenAmount::from(100u64),
		PendingWithdrawState {
			total_withdraw: TokenAmount::from(100u64),
			expiration: BlockExpiration::from(50u64),
			nonce: Nonce::from(1u64),
			recipient_metadata: None,
		},
	);

	let expected_event = SendWithdrawExpired {
		inner: SendMessageEventInner {
			recipient: channel_state.partner_state.address,
			recipient_metadata: None,
			canonical_identifier: canonical_identifier.clone(),
			message_identifier: 1,
		},
		participant: channel_state.our_state.address,
		nonce: Nonce::from(1u64),
		expiration: BlockExpiration::from(50u64),
		total_withdraw: TokenAmount::from(100u64),
	};
	let state_change = Block {
		block_number: BlockNumber::from(511u64),
		block_hash: BlockHash::random(),
		gas_limit: GasLimit::zero(),
	};
	let result = chain::state_transition(chain_info.chain_state.clone(), state_change.into())
		.expect("Block should succeed");

	assert!(!result.events.is_empty());
	assert_eq!(result.events[0], expected_event.into())
}

#[test]
fn test_closed_channel_new_block() {
	let mut chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let token_network_registry_state = chain_info
		.chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&chain_info.token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&chain_info.token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&canonical_identifier.channel_identifier)
		.expect("Channel should exist");

	channel_state.close_transaction = Some(TransactionExecutionStatus {
		started_block_number: Some(BlockNumber::from(10u64)),
		finished_block_number: Some(BlockNumber::from(10u64)),
		result: Some(TransactionResult::Success),
	});

	let block_hash = BlockHash::random();
	let state_change =
		Block { block_number: BlockNumber::from(511u64), block_hash, gas_limit: GasLimit::zero() };
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("Block should succeed");

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
	let chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();
	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_info.chain_state, canonical_identifier);

	assert!(channel_state.is_some());
}

#[test]
fn test_channel_closed() {
	let mut chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![
			(
				(Keyring::Alice.address(), TokenAmount::zero()),
				(Keyring::Bob.address(), TokenAmount::zero()),
			),
			(
				(Keyring::Bob.address(), TokenAmount::zero()),
				(Keyring::Charlie.address(), TokenAmount::zero()),
			),
		])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let state_change = ContractReceiveChannelClosed {
		transaction_hash: Some(TransactionHash::random()),
		block_number: BlockNumber::from(10u64),
		block_hash: BlockHash::random(),
		transaction_from: Address::random(),
		canonical_identifier,
	};

	let mut result = chain::state_transition(chain_info.chain_state.clone(), state_change.into())
		.expect("Should close channel");
	assert!(result.events.is_empty());

	let canonical_identifier = chain_info.canonical_identifiers[1].clone();

	let balance_proof_state = BalanceProofState {
		nonce: Nonce::from(1u64),
		transferred_amount: TokenAmount::zero(),
		locked_amount: TokenAmount::zero(),
		locksroot: compute_locksroot(&PendingLocksState { locks: vec![] }),
		canonical_identifier: canonical_identifier.clone(),
		balance_hash: BalanceHash::zero(),
		message_hash: Some(MessageHash::zero()),
		signature: None,
		sender: Some(Address::zero()),
	};

	let token_network_registry_state = chain_info
		.chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&chain_info.token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&chain_info.token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&canonical_identifier.channel_identifier)
		.expect("Channel should exist");
	channel_state.partner_state.balance_proof = Some(balance_proof_state.clone());
	let _ = update_channel(&mut result.new_state, channel_state.clone());

	let state_change = ContractReceiveChannelClosed {
		transaction_hash: Some(TransactionHash::random()),
		block_number: BlockNumber::from(10u64),
		block_hash: BlockHash::zero(),
		transaction_from: Address::random(),
		canonical_identifier: canonical_identifier.clone(),
	};

	let result = chain::state_transition(result.new_state.clone(), state_change.into())
		.expect("Should close channel");

	let event = ContractSendChannelUpdateTransfer {
		inner: ContractSendEventInner { triggered_by_blockhash: BlockHash::zero() },
		expiration: BlockExpiration::from(510u64),
		balance_proof: balance_proof_state,
	};
	assert!(!result.events.is_empty());
	assert_eq!(result.events[0], event.into());
}

#[test]
fn test_channel_withdraw() {
	let mut chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let token_network_registry_state = chain_info
		.chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&chain_info.token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&chain_info.token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&canonical_identifier.channel_identifier)
		.expect("Channel should exist");

	assert_eq!(channel_state.our_state.contract_balance, TokenAmount::zero());

	channel_state.our_state.contract_balance = TokenAmount::from(1000);
	channel_state.partner_state.contract_balance = TokenAmount::from(1000);

	let state_change = ContractReceiveChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		participant: Keyring::Alice.address(),
		total_withdraw: TokenAmount::from(100u64),
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: BlockNumber::from(1u64),
		block_hash: BlockHash::zero(),
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("Withdraw should succeed");
	let chain_state = result.new_state;
	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel should exist")
			.clone();
	assert_eq!(channel_state.our_state.onchain_total_withdraw, TokenAmount::from(100u64));

	let state_change = ContractReceiveChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		participant: channel_state.partner_state.address,
		total_withdraw: TokenAmount::from(99u64),
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: BlockNumber::from(1u64),
		block_hash: BlockHash::zero(),
	};
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Withdraw should succeed");
	let chain_state = result.new_state;
	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel should exist");
	assert_eq!(channel_state.partner_state.onchain_total_withdraw, TokenAmount::from(99u64));
}

#[test]
fn test_channel_deposit() {
	let chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let channel_state = views::get_channel_by_canonical_identifier(
		&chain_info.chain_state,
		canonical_identifier.clone(),
	)
	.expect("Channel should exist");

	assert_eq!(channel_state.our_state.contract_balance, TokenAmount::zero());

	let state_change = ContractReceiveChannelDeposit {
		canonical_identifier: canonical_identifier.clone(),
		deposit_transaction: TransactionChannelDeposit {
			participant_address: Keyring::Alice.address(),
			contract_balance: TokenAmount::from(100u64),
			deposit_block_number: BlockNumber::from(10u64),
		},
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: BlockNumber::from(1u64),
		block_hash: BlockHash::zero(),
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("Deposit should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone())
			.expect("Channel should exist");
	assert_eq!(channel_state.our_state.contract_balance, TokenAmount::from(100u64));

	let chain_state = result.new_state;
	let state_change = ContractReceiveChannelDeposit {
		canonical_identifier: canonical_identifier.clone(),
		deposit_transaction: TransactionChannelDeposit {
			participant_address: chain_state.our_address,
			contract_balance: TokenAmount::from(99u64), // Less than the deposit before
			deposit_block_number: BlockNumber::from(10u64),
		},
		fee_config: MediationFeeConfig::default(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: BlockNumber::from(1u64),
		block_hash: BlockHash::zero(),
	};
	let result =
		chain::state_transition(chain_state, state_change.into()).expect("Deposit should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier)
			.expect("Channel should exist");
	assert_eq!(channel_state.our_state.contract_balance, TokenAmount::from(100u64));
}

#[test]
fn test_channel_settled() {
	let chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let block_hash = BlockHash::random();
	let our_locksroot = Locksroot::from_slice(&[1u8; 32]);

	let state_change = ContractReceiveChannelSettled {
		transaction_hash: Some(TransactionHash::random()),
		block_number: BlockNumber::from(1u64),
		block_hash,
		canonical_identifier: canonical_identifier.clone(),
		our_onchain_locksroot: *LOCKSROOT_OF_NO_LOCKS,
		partner_onchain_locksroot: *LOCKSROOT_OF_NO_LOCKS,
		our_transferred_amount: TokenAmount::zero(),
		partner_transferred_amount: TokenAmount::zero(),
	};
	let result = chain::state_transition(chain_info.chain_state.clone(), state_change.into())
		.expect("Channel settled should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone());
	assert_eq!(channel_state, None);

	let state_change = ContractReceiveChannelSettled {
		transaction_hash: Some(TransactionHash::random()),
		block_number: BlockNumber::from(1u64),
		block_hash,
		canonical_identifier: canonical_identifier.clone(),
		our_onchain_locksroot: our_locksroot,
		partner_onchain_locksroot: *LOCKSROOT_OF_NO_LOCKS,
		our_transferred_amount: TokenAmount::zero(),
		partner_transferred_amount: TokenAmount::zero(),
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
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
			canonical_identifier,
			sender: channel_state.partner_state.address,
		}
		.into()
	)
}

#[test]
fn test_channel_batch_unlock() {
	let mut chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let token_network_registry_state = chain_info
		.chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&chain_info.token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&chain_info.token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&canonical_identifier.channel_identifier)
		.expect("Channel should exist");

	channel_state.settle_transaction = Some(TransactionExecutionStatus {
		started_block_number: Some(BlockNumber::from(1u64)),
		finished_block_number: Some(BlockNumber::from(2u64)),
		result: Some(TransactionResult::Success),
	});

	let state_change = ContractReceiveChannelBatchUnlock {
		canonical_identifier: canonical_identifier.clone(),
		receiver: channel_state.our_state.address,
		sender: channel_state.partner_state.address,
		locksroot: *LOCKSROOT_OF_NO_LOCKS,
		unlocked_amount: TokenAmount::from(100u64),
		returned_tokens: TokenAmount::zero(),
		transaction_hash: Some(TransactionHash::zero()),
		block_number: BlockNumber::from(1u64),
		block_hash: BlockHash::zero(),
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("Should succeeed");

	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone());
	assert_eq!(channel_state, None);
}

#[test]
fn test_channel_action_withdraw() {
	let mut chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	// Withdraw with insufficient onchain balance
	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: TokenAmount::from(100u64),
		recipient_metadata: None,
	};
	let result = chain::state_transition(chain_info.chain_state.clone(), state_change.into())
		.expect("action withdraw should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionWithdraw {
			attemped_withdraw: TokenAmount::from(100u64),
			reason: "Insufficient balance: 0. Requested 100 for withdraw".to_owned(),
		}
		.into()
	);
	// Withdraw a zero amount
	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: TokenAmount::zero(),
		recipient_metadata: None,
	};
	let result = chain::state_transition(chain_info.chain_state.clone(), state_change.into())
		.expect("action withdraw should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionWithdraw {
			attemped_withdraw: TokenAmount::zero(),
			reason: "Total withdraw 0 did not increase".to_owned(),
		}
		.into()
	);

	let token_network_registry_state = chain_info
		.chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&chain_info.token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&chain_info.token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&canonical_identifier.channel_identifier)
		.expect("Channel should exist");

	channel_state.close_transaction = Some(TransactionExecutionStatus {
		started_block_number: Some(BlockNumber::from(1u64)),
		finished_block_number: Some(BlockNumber::from(1u64)),
		result: Some(TransactionResult::Success),
	});
	// Withdraw on a closed channel
	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: TokenAmount::from(100u64),
		recipient_metadata: None,
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("action withdraw should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionWithdraw {
			attemped_withdraw: TokenAmount::from(100u64),
			reason: "Invalid withdraw, the channel is not opened".to_owned(),
		}
		.into()
	);

	let mut chain_state = result.new_state;
	let token_network_registry_state = chain_state
		.identifiers_to_tokennetworkregistries
		.get_mut(&chain_info.token_network_registry_address)
		.expect("Registry should exist");
	let token_network_state = token_network_registry_state
		.tokennetworkaddresses_to_tokennetworks
		.get_mut(&chain_info.token_network_address)
		.expect("token network should exist");
	let channel_state = token_network_state
		.channelidentifiers_to_channels
		.get_mut(&canonical_identifier.channel_identifier)
		.expect("Channel should exist");

	// Successful withdraw
	channel_state.close_transaction = None;
	channel_state.our_state.contract_balance = TokenAmount::from(200u64);

	let state_change = ActionChannelWithdraw {
		canonical_identifier: canonical_identifier.clone(),
		total_withdraw: TokenAmount::from(100u64),
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
			total_withdraw: TokenAmount::from(100u64),
			participant: channel_state.our_state.address,
			nonce: channel_state.our_state.nonce,
			expiration: BlockExpiration::from(101u64),
			coop_settle: false,
		}
		.into()
	)
}

#[test]
fn test_channel_set_reveal_timeout() {
	let chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let channel_state = views::get_channel_by_canonical_identifier(
		&chain_info.chain_state,
		canonical_identifier.clone(),
	)
	.expect("Channel state should exist");

	assert_eq!(channel_state.reveal_timeout, RevealTimeout::from(DEFAULT_REVEAL_TIMEOUT));

	let state_change = ActionChannelSetRevealTimeout {
		canonical_identifier: canonical_identifier.clone(),
		reveal_timeout: RevealTimeout::from(6u64),
	};
	let result = chain::state_transition(chain_info.chain_state.clone(), state_change.into())
		.expect("Set reveal timeout should succeed");
	assert!(!result.events.is_empty());
	assert_eq!(
		result.events[0],
		ErrorInvalidActionSetRevealTimeout {
			reveal_timeout: RevealTimeout::from(6u64),
			reason: "Settle timeout should be at least twice as large as reveal timeout".to_owned(),
		}
		.into()
	);

	let reveal_timeout = channel_state.settle_timeout.div(2).sub(1).as_u64();
	let state_change = ActionChannelSetRevealTimeout {
		canonical_identifier: canonical_identifier.clone(),
		reveal_timeout: RevealTimeout::from(reveal_timeout),
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("Set reveal timeout should succeed");
	assert!(result.events.is_empty());
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier)
			.expect("Channel state should exist");

	assert_eq!(channel_state.reveal_timeout, RevealTimeout::from(reveal_timeout));
}
