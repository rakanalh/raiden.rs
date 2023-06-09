use std::collections::HashMap;

use raiden_primitives::{
	hashing::hash_secret,
	types::{
		Address,
		BlockExpiration,
		BlockHash,
		BlockNumber,
		Bytes,
		CanonicalIdentifier,
		GasLimit,
		LockTimeout,
		PaymentIdentifier,
		Secret,
		SecretHash,
		SecretRegistryAddress,
		TokenAmount,
		TransactionHash,
		H256,
	},
};

use crate::{
	machine::chain,
	tests::factories::{
		ChainStateBuilder,
		Generator,
		Keyring,
	},
	types::{
		ActionChannelClose,
		ActionInitInitiator,
		Block,
		ChainState,
		ContractReceiveSecretReveal,
		Event,
		ReceiveSecretRequest,
		ReceiveSecretReveal,
		RouteState,
		TransferDescriptionWithSecretState,
	},
	views,
};

fn setup_initiator() -> (ChainState, CanonicalIdentifier, PaymentIdentifier, Secret, SecretHash) {
	let chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::from(1000)),
			(Keyring::Bob.address(), TokenAmount::zero()),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let channel_state = views::get_channel_by_canonical_identifier(
		&chain_info.chain_state,
		canonical_identifier.clone(),
	)
	.expect("Channel state should exist");

	let lock_timeout = Some(LockTimeout::from(100));
	let transfer_identifier = PaymentIdentifier::from(1);

	let secret = Generator::random_secret();
	let secrethash = SecretHash::from_slice(&hash_secret(&secret.0));
	let state_change = ActionInitInitiator {
		transfer: TransferDescriptionWithSecretState {
			token_network_registry_address: chain_info.token_network_registry_address,
			token_network_address: chain_info.token_network_address,
			lock_timeout,
			payment_identifier: transfer_identifier,
			amount: TokenAmount::from(100),
			initiator: channel_state.our_state.address,
			target: channel_state.partner_state.address,
			secret: secret.clone(),
			secrethash,
		},
		routes: vec![RouteState {
			route: vec![channel_state.our_state.address, channel_state.partner_state.address],
			address_to_metadata: HashMap::new(),
			swaps: HashMap::new(),
			estimated_fee: TokenAmount::zero(),
		}],
	};
	let result = chain::state_transition(chain_info.chain_state, state_change.into())
		.expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendLockedTransfer { .. }));
	(result.new_state, canonical_identifier, transfer_identifier, secret, secrethash)
}

#[test]
fn test_init_initiator() {
	let chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::from(1000)),
			(Keyring::Bob.address(), TokenAmount::from(1000)),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let channel_state = views::get_channel_by_canonical_identifier(
		&chain_info.chain_state,
		canonical_identifier.clone(),
	)
	.expect("Channel state should exist");

	let lock_timeout = Some(LockTimeout::from(100));
	let transfer_identifier = PaymentIdentifier::from(1);

	let secret = Generator::random_secret();
	let secrethash = SecretHash::from_slice(&hash_secret(&secret.0));
	let mut state_change = ActionInitInitiator {
		transfer: TransferDescriptionWithSecretState {
			token_network_registry_address: chain_info.token_network_registry_address,
			token_network_address: chain_info.token_network_address,
			lock_timeout,
			payment_identifier: transfer_identifier,
			amount: TokenAmount::from(100),
			initiator: channel_state.our_state.address,
			target: channel_state.partner_state.address,
			secret,
			secrethash,
		},
		routes: vec![RouteState {
			route: vec![],
			address_to_metadata: HashMap::new(),
			swaps: HashMap::new(),
			estimated_fee: TokenAmount::zero(),
		}],
	};

	// Fails because no route is available
	let result =
		chain::state_transition(chain_info.chain_state.clone(), state_change.clone().into())
			.expect("Should succeed");
	assert!(matches!(result.events[0], Event::ErrorPaymentSentFailed { .. }));

	state_change.routes[0].route =
		vec![channel_state.our_state.address, channel_state.partner_state.address];

	let result = chain::state_transition(chain_info.chain_state, state_change.clone().into())
		.expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendLockedTransfer { .. }));

	let channel_close = ActionChannelClose { canonical_identifier };
	let result = chain::state_transition(result.new_state, channel_close.into());

	let secret = Generator::random_secret();
	let secrethash = SecretHash::from_slice(&hash_secret(&secret.0));
	state_change.transfer.secret = secret;
	state_change.transfer.secrethash = secrethash;
	state_change.transfer.amount = TokenAmount::from(100);

	// Fails because no channel is usable
	let result =
		chain::state_transition(result.expect("Should succeed").new_state, state_change.into())
			.expect("Should succeed");
	assert!(matches!(result.events[0], Event::ErrorPaymentSentFailed { .. }));
}

#[test]
fn test_initiator_lock_expired() {
	let (chain_state, _canonical_identifier, _transfer_identifier, _secret, _secrethash) =
		setup_initiator();

	let block = Block {
		block_number: BlockNumber::from(112),
		block_hash: BlockHash::random(),
		gas_limit: GasLimit::default(),
	};

	let result = chain::state_transition(chain_state, block.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendLockExpired { .. }));
}

#[test]
fn test_initiator_secret_request() {
	let (chain_state, canonical_identifier, transfer_identifier, _secret, secrethash) =
		setup_initiator();

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier)
			.expect("Channel state should exist");

	// Random partner, no response
	let secret_request = ReceiveSecretRequest {
		sender: Address::random(),
		payment_identifier: transfer_identifier,
		amount: TokenAmount::from(100),
		expiration: BlockExpiration::from(100),
		secrethash,
		revealsecret: None,
	};

	let result = chain::state_transition(chain_state.clone(), secret_request.into())
		.expect("Should succeed");
	assert_eq!(result.events, vec![]);

	// Random partner, no response
	let secret_request = ReceiveSecretRequest {
		sender: channel_state.partner_state.address,
		payment_identifier: transfer_identifier,
		amount: TokenAmount::from(100),
		expiration: BlockExpiration::from(101),
		secrethash,
		revealsecret: None,
	};

	let result =
		chain::state_transition(result.new_state, secret_request.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendSecretReveal { .. }));
}

#[test]
fn test_initiator_offchain_secret_reveal() {
	let (chain_state, canonical_identifier, _transfer_identifier, secret, secrethash) =
		setup_initiator();

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier)
			.expect("Channel state should exist");

	// Random participant, no unlock
	let secret_reveal =
		ReceiveSecretReveal { sender: Address::random(), secret: secret.clone(), secrethash };
	let result =
		chain::state_transition(chain_state.clone(), secret_reveal.into()).expect("Should succeed");

	assert_eq!(result.events, vec![]);

	// Wrong Secret, no unlock
	let secret_reveal = ReceiveSecretReveal {
		sender: Address::random(),
		secret: Bytes(H256::random().0.to_vec()),
		secrethash,
	};
	let result =
		chain::state_transition(result.new_state, secret_reveal.into()).expect("Should succeed");

	assert_eq!(result.events, vec![]);

	assert!(result.new_state.payment_mapping.secrethashes_to_task.get(&secrethash).is_some());

	let secret_reveal =
		ReceiveSecretReveal { sender: channel_state.partner_state.address, secret, secrethash };
	let result =
		chain::state_transition(result.new_state, secret_reveal.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendUnlock { .. }));

	assert!(result.new_state.payment_mapping.secrethashes_to_task.get(&secrethash).is_none());
}

#[test]
fn test_initiator_onchain_secret_reveal() {
	let (chain_state, canonical_identifier, _transfer_identifier, secret, secrethash) =
		setup_initiator();

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier)
			.expect("Channel state should exist");

	// Random participant, no unlock
	let secret_reveal =
		ReceiveSecretReveal { sender: Address::random(), secret: secret.clone(), secrethash };
	let result =
		chain::state_transition(chain_state.clone(), secret_reveal.into()).expect("Should succeed");

	assert_eq!(result.events, vec![]);

	// Wrong Secret, no unlock
	let secret_reveal = ContractReceiveSecretReveal {
		transaction_hash: Some(TransactionHash::random()),
		block_number: BlockNumber::from(200),
		block_hash: BlockHash::random(),
		secret_registry_address: SecretRegistryAddress::random(),
		secrethash,
		secret: secret.clone(),
	};
	let result =
		chain::state_transition(result.new_state, secret_reveal.into()).expect("Should succeed");

	assert_eq!(result.events, vec![]);

	assert!(result.new_state.payment_mapping.secrethashes_to_task.get(&secrethash).is_some());

	let secret_reveal =
		ReceiveSecretReveal { sender: channel_state.partner_state.address, secret, secrethash };
	let result =
		chain::state_transition(result.new_state, secret_reveal.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendUnlock { .. }));

	assert!(result.new_state.payment_mapping.secrethashes_to_task.get(&secrethash).is_none());
}

// #[test]
// fn test_initiator_receive_lock_expired() {
// 	let (chain_state, canonical_identifier, _transfer_identifier, _secret, secrethash) =
// 		setup_initiator();

// 	let channel_state =
// 		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
// 			.expect("Channel state should exist");

// 	let locked_amount = TokenAmount::from(100);
// 	let lock = HashTimeLockState::create(locked_amount, BlockExpiration::from(111), secrethash);
// 	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![lock.encoded] });

// 	let balance_hash = hash_balance_data(TokenAmount::zero(), locked_amount, locksroot.clone())
// 		.expect("Should hash balance data");

// 	let balance_proof = BalanceProofState {
// 		nonce: Nonce::from(1),
// 		transferred_amount: TokenAmount::zero(),
// 		locked_amount,
// 		locksroot: locksroot.clone(),
// 		canonical_identifier: canonical_identifier.clone(),
// 		balance_hash,
// 		message_hash: Some(H256::random()),
// 		signature: Some(Signature::from([0; 65].to_vec())),
// 		sender: Some(channel_state.partner_state.address),
// 	};
// 	let lock_expired = ReceiveLockExpired {
// 		sender: channel_state.partner_state.address,
// 		secrethash,
// 		message_identifier: 1u64,
// 		balance_proof,
// 	};
// 	let result = chain::state_transition(chain_state, lock_expired.into()).expect("Should succeed");
// 	assert!(matches!(result.events[0], Event::SendUnlock { .. }));

// 	assert!(result.new_state.payment_mapping.secrethashes_to_task.get(&secrethash).is_none());
// }
