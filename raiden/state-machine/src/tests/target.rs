use std::collections::HashMap;

use raiden_primitives::{
	hashing::hash_secret,
	types::{
		BlockExpiration,
		BlockHash,
		BlockNumber,
		CanonicalIdentifier,
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
		ActionInitTarget,
		Block,
		ChainState,
		ContractReceiveSecretReveal,
		Event,
		HashTimeLockState,
		HopState,
		LockedTransferState,
		PendingLocksState,
		ReceiveLockExpired,
		ReceiveUnlock,
		RouteState,
	},
	views,
};

fn setup_target() -> (ChainState, CanonicalIdentifier, PaymentIdentifier, Secret, SecretHash) {
	let chain_info = ChainStateBuilder::new()
		.with_token_network_registry()
		.with_token_network()
		.with_channels(vec![(
			(Keyring::Alice.address(), TokenAmount::zero()),
			(Keyring::Bob.address(), TokenAmount::from(1000)),
		)])
		.build();

	let canonical_identifier = chain_info.canonical_identifiers[0].clone();

	let channel_state = views::get_channel_by_canonical_identifier(
		&chain_info.chain_state,
		canonical_identifier.clone(),
	)
	.expect("Channel state should exist");

	let secret = Generator::random_secret();
	let secrethash = SecretHash::from_slice(&hash_secret(&secret.0));
	let locked_amount = TokenAmount::from(100);
	let transferred_amount = TokenAmount::from(0);
	let lock = HashTimeLockState::create(locked_amount, BlockExpiration::from(111), secrethash);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![lock.encoded.clone()] });
	let balance_proof = make_balance_proof(
		Keyring::Bob.private_key(),
		canonical_identifier.clone(),
		locked_amount,
		locksroot,
		transferred_amount,
		Keyring::Bob.address(),
		Nonce::from(1),
	);
	let transfer_identifier = PaymentIdentifier::from(1);
	let state_change = ActionInitTarget {
		sender: channel_state.partner_state.address,
		balance_proof: balance_proof.clone(),
		from_hop: HopState {
			node_address: channel_state.partner_state.address,
			channel_identifier: canonical_identifier.channel_identifier.clone(),
		},
		transfer: LockedTransferState {
			payment_identifier: transfer_identifier,
			token: channel_state.token_address,
			lock,
			initiator: channel_state.partner_state.address,
			target: channel_state.our_state.address,
			message_identifier: 1u64,
			route_states: vec![RouteState {
				route: vec![channel_state.our_state.address, channel_state.partner_state.address],
				address_to_metadata: HashMap::new(),
				swaps: HashMap::new(),
				estimated_fee: TokenAmount::zero(),
			}],
			balance_proof,
			secret: None,
		},
		received_valid_secret: false,
	};

	let result =
		chain::state_transition(chain_info.chain_state.clone(), state_change.clone().into())
			.expect("Should succeed");
	assert!(matches!(result.events[0], Event::SendProcessed { .. }));
	assert!(matches!(result.events[1], Event::SendSecretRequest { .. }));

	(result.new_state, canonical_identifier, transfer_identifier, secret, secrethash)
}

#[test]
fn test_target_expires_target() {
	let (chain_state, _, _, _, _) = setup_target();
	let block = Block {
		block_number: BlockNumber::from(120),
		block_hash: BlockHash::random(),
		gas_limit: GasLimit::zero(),
	};

	let result = chain::state_transition(chain_state, block.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::ErrorUnlockClaimFailed { .. }));
}

#[test]
fn test_target_onchain_secret_reveal() {
	let (chain_state, canonical_identifier, _, secret, secrethash) = setup_target();

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
		views::get_channel_by_canonical_identifier(&result.new_state, canonical_identifier.clone())
			.expect("Channel state should exist");

	assert!(channel_state
		.partner_state
		.secrethashes_to_onchain_unlockedlocks
		.contains_key(&secrethash));

	let block = Block {
		block_number: BlockNumber::from(100),
		block_hash: BlockHash::random(),
		gas_limit: GasLimit::zero(),
	};

	let result = chain::state_transition(result.new_state, block.into()).expect("Should succeed");

	assert!(matches!(result.events[0], Event::ContractSendSecretReveal { .. }));
}

#[test]
fn test_target_receive_lock_expired() {
	let (chain_state, canonical_identifier, _, _, secrethash) = setup_target();

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel state should exist");

	let transferred_amount = TokenAmount::from(0);
	let locked_amount = TokenAmount::from(0);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![] });
	let nonce = Nonce::from(2);
	let balance_proof = make_balance_proof(
		Keyring::Bob.private_key(),
		canonical_identifier,
		locked_amount,
		locksroot,
		transferred_amount,
		channel_state.partner_state.address,
		nonce,
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
	assert!(matches!(result.events[1], Event::ErrorUnlockClaimFailed { .. }));
}

#[test]
fn test_target_receive_unlock() {
	let (chain_state, canonical_identifier, _, secret, secrethash) = setup_target();

	let channel_state =
		views::get_channel_by_canonical_identifier(&chain_state, canonical_identifier.clone())
			.expect("Channel state should exist");

	// Wrong transferred amount
	let transferred_amount = TokenAmount::from(99);
	let locked_amount = TokenAmount::from(0);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![] });
	let nonce = Nonce::from(2);
	let balance_proof = make_balance_proof(
		Keyring::Bob.private_key(),
		canonical_identifier.clone(),
		locked_amount,
		locksroot,
		transferred_amount,
		channel_state.partner_state.address,
		nonce,
	);

	let unlock = ReceiveUnlock {
		sender: channel_state.partner_state.address,
		secret: secret.clone(),
		secrethash: secrethash.clone(),
		message_identifier: 2u64,
		balance_proof,
	};
	let result =
		chain::state_transition(chain_state.clone(), unlock.into()).expect("Should succeed");
	assert!(matches!(result.events[0], Event::ErrorInvalidReceivedUnlock { .. }));

	let transferred_amount = TokenAmount::from(100);
	let locked_amount = TokenAmount::from(0);
	let locksroot = compute_locksroot(&PendingLocksState { locks: vec![] });
	let nonce = Nonce::from(2);
	let balance_proof = make_balance_proof(
		Keyring::Bob.private_key(),
		canonical_identifier,
		locked_amount,
		locksroot,
		transferred_amount,
		channel_state.partner_state.address,
		nonce,
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
	assert!(matches!(result.events[1], Event::PaymentReceivedSuccess { .. }));
}
