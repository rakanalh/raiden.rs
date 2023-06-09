use std::collections::HashMap;

use raiden_primitives::{
	hashing::hash_secret,
	types::{
		BlockExpiration,
		BlockHash,
		BlockNumber,
		GasLimit,
		Nonce,
		PaymentIdentifier,
		Secret,
		SecretHash,
		SecretRegistryAddress,
		TokenAmount,
		TransactionHash,
	},
};

use super::factories::ChainStateInfo;
use crate::{
	machine::{
		chain,
		channel::utils::compute_locksroot,
	},
	tests::factories::{
		make_balance_proof,
		ChainStateBuilder,
		Generator,
		Keyring,
	},
	types::{
		ActionInitMediator,
		Block,
		ContractReceiveSecretReveal,
		Event,
		HashTimeLockState,
		HopState,
		LockedTransferState,
		PendingLocksState,
		ReceiveLockExpired,
		ReceiveSecretReveal,
		ReceiveUnlock,
		RouteState,
	},
	views,
};

fn setup_mediator() -> (ChainStateInfo, PaymentIdentifier, Secret, SecretHash) {
	let mut chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![
			(
				(Keyring::Bob.address(), TokenAmount::from(1000)),
				(Keyring::Alice.address(), TokenAmount::from(1000)),
			),
			(
				(Keyring::Bob.address(), TokenAmount::from(0)),
				(Keyring::Charlie.address(), TokenAmount::from(0)),
			),
		])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let secret = Generator::random_secret();
	let secrethash = SecretHash::from_slice(&hash_secret(&secret.0));
	let locked_amount = TokenAmount::from(100);
	let transferred_amount = TokenAmount::from(0);
	let lock = HashTimeLockState::create(locked_amount, BlockExpiration::from(111), secrethash);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![lock.encoded.clone()] });
	let transfer_identifier = PaymentIdentifier::from(1);

	let balance_proof = make_balance_proof(
		Keyring::Alice.private_key(),
		canonical_identifier.clone(),
		locked_amount,
		locksroot,
		transferred_amount,
		Keyring::Alice.address(),
		Nonce::from(1),
	);
	let route_states = vec![RouteState {
		route: vec![Keyring::Bob.address(), Keyring::Alice.address(), Keyring::Charlie.address()],
		address_to_metadata: HashMap::new(),
		swaps: HashMap::new(),
		estimated_fee: TokenAmount::zero(),
	}];
	let state_change = ActionInitMediator {
		sender: Keyring::Alice.address(),
		balance_proof: balance_proof.clone(),
		from_hop: HopState {
			node_address: Keyring::Alice.address(),
			channel_identifier: canonical_identifier.channel_identifier,
		},
		candidate_route_states: route_states.clone(),
		from_transfer: LockedTransferState {
			payment_identifier: transfer_identifier,
			token: chain_info.token_address,
			lock,
			initiator: Keyring::Alice.address(),
			target: Keyring::Charlie.address(),
			message_identifier: 1u64,
			route_states,
			balance_proof,
			secret: None,
		},
	};
	let result = chain::state_transition(chain_info.chain_state.clone(), state_change.into())
		.expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendProcessed { .. }));
	assert!(matches!(result.events[1], Event::SendLockedTransfer { .. }));

	chain_info.chain_state = result.new_state;
	(chain_info, transfer_identifier, secret, secrethash)
}

#[test]
fn test_mediator_sends_lock_expired() {
	let (chain_info, _, _, _) = setup_mediator();

	let chain_state = chain_info.chain_state;

	let block = Block {
		block_number: BlockNumber::from(125),
		block_hash: BlockHash::random(),
		gas_limit: GasLimit::zero(),
	};

	let result = chain::state_transition(chain_state, block.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendLockExpired { .. }));
	assert!(matches!(result.events[1], Event::ErrorUnlockFailed { .. }));
}

#[test]
fn test_mediator_onchain_secret_reveal() {
	let (chain_info, _, secret, secrethash) = setup_mediator();

	let chain_state = chain_info.chain_state;
	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let onchain_secret_reveal = ContractReceiveSecretReveal {
		transaction_hash: Some(TransactionHash::random()),
		block_number: BlockNumber::from(20),
		block_hash: BlockHash::random(),
		secret_registry_address: SecretRegistryAddress::random(),
		secrethash,
		secret,
	};
	let result =
		chain::state_transition(chain_state, onchain_secret_reveal.into()).expect("Should succeed");
	let channel_state =
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier)
			.expect("Channel state should exist");
	assert!(channel_state.our_state.is_secret_known(secrethash));
	assert!(channel_state.partner_state.is_secret_known(secrethash));
	assert!(!channel_state.our_state.secrethashes_to_lockedlocks.contains_key(&secrethash));
	assert!(!channel_state.our_state.secrethashes_to_unlockedlocks.contains_key(&secrethash));
	assert!(channel_state
		.our_state
		.secrethashes_to_onchain_unlockedlocks
		.contains_key(&secrethash));
	assert!(!channel_state
		.partner_state
		.secrethashes_to_lockedlocks
		.contains_key(&secrethash));
	assert!(!channel_state
		.partner_state
		.secrethashes_to_unlockedlocks
		.contains_key(&secrethash));
	assert!(channel_state
		.partner_state
		.secrethashes_to_onchain_unlockedlocks
		.contains_key(&secrethash));
}

#[test]
fn test_mediator_send_balance_proof_on_secret_learned() {
	let (chain_info, _, secret, secrethash) = setup_mediator();

	let chain_state = chain_info.chain_state;

	let offchain_secret_reveal =
		ReceiveSecretReveal { sender: Keyring::Alice.address(), secret, secrethash };

	let result = chain::state_transition(chain_state, offchain_secret_reveal.into())
		.expect("Should succeed");

	assert!(matches!(result.events[0], Event::SendSecretReveal { .. }));
	assert!(matches!(result.events[1], Event::SendUnlock { .. }));
	assert!(matches!(result.events[2], Event::UnlockSuccess { .. }));
}

#[test]
fn test_mediator_pair_expired_on_block() {
	let (chain_info, _, _, _) = setup_mediator();

	let chain_state = chain_info.chain_state;

	let block = Block {
		block_number: BlockNumber::from(125),
		block_hash: BlockHash::random(),
		gas_limit: GasLimit::zero(),
	};

	let result = chain::state_transition(chain_state, block.into()).expect("Should succeed");
	assert!(matches!(result.events[2], Event::ErrorUnlockClaimFailed { .. }));
}

#[test]
fn test_mediator_receive_lock_expired() {
	let (chain_info, _, _, secrethash) = setup_mediator();
	let chain_state = chain_info.chain_state;
	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel state should exist");

	let locked_amount = TokenAmount::from(0);
	let transferred_amount = TokenAmount::from(0);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![] });

	let balance_proof = make_balance_proof(
		Keyring::Alice.private_key(),
		canonical_identifier,
		locked_amount,
		locksroot,
		transferred_amount,
		Keyring::Alice.address(),
		Nonce::from(2),
	);

	let block = Block {
		block_number: BlockNumber::from(120),
		block_hash: BlockHash::random(),
		gas_limit: GasLimit::zero(),
	};
	let result =
		chain::state_transition(chain_state.clone(), block.into()).expect("Should succeed");

	let lock_expired = ReceiveLockExpired {
		sender: channel_state.partner_state.address,
		secrethash,
		message_identifier: 2u64,
		balance_proof,
	};

	let result =
		chain::state_transition(result.new_state, lock_expired.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendProcessed { .. }));
}

#[test]
fn test_mediator_receive_unlock() {
	let (chain_info, _, secret, secrethash) = setup_mediator();

	let chain_state = chain_info.chain_state;
	let canonical_identifier = chain_info.canonical_identifiers[0].clone();
	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel state should exist");

	let offchain_secret_reveal = ReceiveSecretReveal {
		sender: Keyring::Alice.address(),
		secret: secret.clone(),
		secrethash,
	};

	let result = chain::state_transition(chain_state.clone(), offchain_secret_reveal.into())
		.expect("Should succeed");

	let chain_state = result.new_state;

	// Wrong transferred amount
	let locked_amount = TokenAmount::from(0);
	let transferred_amount = TokenAmount::from(99);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![] });

	let balance_proof = make_balance_proof(
		Keyring::Alice.private_key(),
		canonical_identifier.clone(),
		locked_amount,
		locksroot,
		transferred_amount,
		Keyring::Alice.address(),
		Nonce::from(2),
	);

	let unlock = ReceiveUnlock {
		sender: channel_state.partner_state.address,
		secret: secret.clone(),
		secrethash,
		message_identifier: 2u64,
		balance_proof,
	};
	let result =
		chain::state_transition(chain_state.clone(), unlock.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::ErrorInvalidReceivedUnlock { .. }));

	let locked_amount = TokenAmount::from(0);
	let transferred_amount = TokenAmount::from(100);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![] });

	let balance_proof = make_balance_proof(
		Keyring::Alice.private_key(),
		canonical_identifier,
		locked_amount,
		locksroot,
		transferred_amount,
		Keyring::Alice.address(),
		Nonce::from(2),
	);

	let unlock = ReceiveUnlock {
		sender: channel_state.partner_state.address,
		secret,
		secrethash,
		message_identifier: 2u64,
		balance_proof,
	};

	let result = chain::state_transition(chain_state, unlock.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendProcessed { .. }));
	assert!(matches!(result.events[1], Event::UnlockClaimSuccess { .. }));
}
