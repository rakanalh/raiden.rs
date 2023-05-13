use std::{
	collections::HashMap,
	sync::Arc,
};

use parking_lot::RwLock;
use raiden_blockchain::{
	keys::PrivateKey,
	secret::decrypt_secret,
};
use raiden_network_messages::{
	messages,
	messages::{
		Delivered,
		IncomingMessage,
		MessageInner,
		OutgoingMessage,
		SignedEnvelopeMessage,
		SignedMessage,
		TransportServiceMessage,
	},
};
use raiden_primitives::{
	constants::CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
	hashing::{
		hash_balance_data,
		hash_secret,
	},
	signing,
	types::{
		Address,
		AddressMetadata,
		CanonicalIdentifier,
		QueueIdentifier,
		SecretHash,
		Signature,
	},
};
use raiden_state_machine::{
	types::{
		ActionInitMediator,
		ActionInitTarget,
		BalanceProofState,
		HashTimeLockState,
		HopState,
		LockedTransferState,
		ReceiveDelivered,
		ReceiveLockExpired,
		ReceiveProcessed,
		ReceiveSecretRequest,
		ReceiveSecretReveal,
		ReceiveUnlock,
		ReceiveWithdrawConfirmation,
		ReceiveWithdrawExpired,
		ReceiveWithdrawRequest,
		RouteState,
		StateChange,
	},
	views,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{
	debug,
	error,
};
use web3::signing::Key;

use crate::{
	manager::StateManager,
	Transitioner,
};

pub struct MessageHandler {
	private_key: PrivateKey,
	pathfinding_service_url: String,
	transport_sender: UnboundedSender<TransportServiceMessage>,
	state_manager: Arc<RwLock<StateManager>>,
	transition_service: Arc<Transitioner>,
	metadata_cache: HashMap<Address, AddressMetadata>,
}

impl MessageHandler {
	pub fn new(
		private_key: PrivateKey,
		pathfinding_service_url: String,
		transport_sender: UnboundedSender<TransportServiceMessage>,
		state_manager: Arc<RwLock<StateManager>>,
		transition_service: Arc<Transitioner>,
	) -> Self {
		Self {
			private_key,
			pathfinding_service_url,
			transport_sender,
			state_manager,
			transition_service,
			metadata_cache: HashMap::new(),
		}
	}

	pub async fn handle(&mut self, message: IncomingMessage) -> Result<(), String> {
		debug!(message = "Received message", msg_type = message.type_name());
		let state_changes = self.convert(message).await?;
		let transition_service = self.transition_service.clone();
		tokio::spawn(async move {
			if let Err(e) = transition_service.transition(state_changes).await {
				error!(message = "Error handling message", error = format!("{:?}", e));
			}
		});
		Ok(())
	}

	async fn convert(&mut self, message: IncomingMessage) -> Result<Vec<StateChange>, String> {
		let (sender, state_changes) = match message.inner {
			messages::MessageInner::LockedTransfer(message) => {
				let data = message.bytes_to_sign();
				let sender = get_sender(&data, &message.signature.0)?;
				let balance_hash = hash_balance_data(
					message.transferred_amount,
					message.locked_amount,
					message.locksroot.clone(),
				)?;
				let balance_proof = BalanceProofState {
					nonce: message.nonce,
					transferred_amount: message.transferred_amount,
					locked_amount: message.locked_amount,
					locksroot: message.locksroot.clone(),
					canonical_identifier: CanonicalIdentifier {
						chain_identifier: message.chain_id,
						token_network_address: message.token_network_address,
						channel_identifier: message.channel_identifier,
					},
					balance_hash,
					message_hash: Some(message.message_hash()),
					signature: Some(Signature::from(message.signature.0)),
					sender: Some(sender),
				};
				let route_states: Vec<RouteState> = message
					.metadata
					.routes
					.iter()
					.map(|route| RouteState {
						route: route.route.clone(),
						address_to_metadata: route.address_metadata.clone(),
						swaps: Default::default(),
						estimated_fee: Default::default(),
					})
					.collect();

				let transfer = LockedTransferState {
					payment_identifier: message.payment_identifier,
					token: message.token,
					lock: HashTimeLockState::create(
						message.lock.amount,
						message.lock.expiration,
						message.lock.secrethash.unwrap_or_default(),
					),
					initiator: message.initiator,
					target: message.target,
					message_identifier: message.message_identifier,
					route_states,
					balance_proof: balance_proof.clone(),
					secret: message.secret,
				};

				let from_hop = HopState {
					node_address: sender,
					channel_identifier: message.channel_identifier,
				};
				let state_changes = if message.target == self.private_key.address() {
					let mut init_target = ActionInitTarget {
						sender,
						balance_proof,
						from_hop,
						transfer: transfer.clone(),
						received_valid_secret: false,
					};

					let secret_reveal = if let Some(encrypted_secret) = message.metadata.secret {
						let decrypted_secret =
							decrypt_secret(encrypted_secret.0, &self.private_key)?;
						if transfer.lock.amount < decrypted_secret.amount ||
							transfer.payment_identifier != decrypted_secret.payment_identifier
						{
							return Err(format!("Invalid Secret"))
						}

						init_target.received_valid_secret = true;

						Some(StateChange::ReceiveSecretReveal(ReceiveSecretReveal {
							sender,
							secret: decrypted_secret.secret,
							secrethash: message.lock.secrethash.unwrap_or_default(),
						}))
					} else {
						None
					};
					let mut ret = vec![StateChange::ActionInitTarget(init_target.clone())];
					if let Some(secret_reveal) = secret_reveal {
						ret.push(secret_reveal);
					}
					ret
				} else {
					let chain_state = &self.state_manager.read().current_state;
					let mut filtered_route_states = vec![];
					for route_state in transfer.route_states.iter() {
						if let Some(next_hope_address) =
							route_state.hop_after(self.private_key.address())
						{
							if views::get_channel_by_token_network_and_partner(
								&chain_state,
								transfer.balance_proof.canonical_identifier.token_network_address,
								next_hope_address,
							)
							.is_some()
							{
								filtered_route_states.push(route_state.clone());
							}
						}
					}
					vec![StateChange::ActionInitMediator(ActionInitMediator {
						sender,
						balance_proof,
						from_hop,
						candidate_route_states: filtered_route_states,
						from_transfer: transfer,
					})]
				};
				(sender, state_changes)
			},
			messages::MessageInner::LockExpired(message) => {
				let sender = get_sender(&message.message_hash().as_bytes(), &message.signature.0)?;
				let balance_hash = hash_balance_data(
					message.transferred_amount,
					message.locked_amount,
					message.locksroot.clone(),
				)?;
				let balance_proof = BalanceProofState {
					nonce: message.nonce,
					transferred_amount: message.transferred_amount,
					locked_amount: message.locked_amount,
					locksroot: message.locksroot.clone(),
					canonical_identifier: CanonicalIdentifier {
						chain_identifier: message.chain_id,
						token_network_address: message.token_network_address,
						channel_identifier: message.channel_identifier,
					},
					balance_hash,
					message_hash: Some(message.message_hash()),
					signature: Some(Signature::from(message.signature.0)),
					sender: Some(sender),
				};
				(
					sender,
					vec![StateChange::ReceiveLockExpired(ReceiveLockExpired {
						sender,
						secrethash: message.secrethash,
						message_identifier: message.message_identifier,
						balance_proof,
					})],
				)
			},
			messages::MessageInner::SecretRequest(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;
				(
					sender,
					vec![StateChange::ReceiveSecretRequest(ReceiveSecretRequest {
						sender,
						secrethash: message.secrethash,
						payment_identifier: message.payment_identifier,
						amount: message.amount,
						expiration: message.expiration,
						revealsecret: None,
					})],
				)
			},
			messages::MessageInner::SecretReveal(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;
				let secrethash = hash_secret(&message.secret.0);
				let secrethash = SecretHash::from_slice(&secrethash);
				(
					sender,
					vec![StateChange::ReceiveSecretReveal(ReceiveSecretReveal {
						sender,
						secrethash,
						secret: message.secret,
					})],
				)
			},
			messages::MessageInner::Unlock(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;
				let balance_hash = hash_balance_data(
					message.transferred_amount,
					message.locked_amount,
					message.locksroot.clone(),
				)?;
				let balance_proof = BalanceProofState {
					nonce: message.nonce,
					transferred_amount: message.transferred_amount,
					locked_amount: message.locked_amount,
					locksroot: message.locksroot.clone(),
					canonical_identifier: CanonicalIdentifier {
						chain_identifier: message.chain_id,
						token_network_address: message.token_network_address,
						channel_identifier: message.channel_identifier,
					},
					balance_hash,
					message_hash: Some(message.message_hash()),
					signature: Some(Signature::from(message.signature.0)),
					sender: Some(sender),
				};
				let secrethash = SecretHash::from_slice(&hash_secret(&message.secret.0));
				(
					sender,
					vec![StateChange::ReceiveUnlock(ReceiveUnlock {
						sender,
						balance_proof,
						secrethash,
						message_identifier: message.message_identifier,
						secret: message.secret,
					})],
				)
			},
			messages::MessageInner::WithdrawRequest(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;

				let sender_metadata = raiden_pathfinding::query_address_metadata(
					self.pathfinding_service_url.clone(),
					sender,
				)
				.await
				.map_err(|e| format!("Could not fetch address metadata {:?}: {}", sender, e))?;
				(
					sender,
					vec![StateChange::ReceiveWithdrawRequest(ReceiveWithdrawRequest {
						sender,
						message_identifier: message.message_identifier,
						canonical_identifier: CanonicalIdentifier {
							chain_identifier: message.chain_id,
							token_network_address: message.token_network_address,
							channel_identifier: message.channel_identifier,
						},
						total_withdraw: message.total_withdraw,
						nonce: message.nonce,
						expiration: message.expiration,
						signature: Signature::from(message.signature.0),
						participant: message.participant,
						coop_settle: message.coop_settle,
						sender_metadata: Some(sender_metadata),
					})],
				)
			},
			messages::MessageInner::WithdrawConfirmation(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;
				(
					sender,
					vec![StateChange::ReceiveWithdrawConfirmation(ReceiveWithdrawConfirmation {
						sender,
						message_identifier: message.message_identifier,
						canonical_identifier: CanonicalIdentifier {
							chain_identifier: message.chain_id,
							token_network_address: message.token_network_address,
							channel_identifier: message.channel_identifier,
						},
						total_withdraw: message.total_withdraw,
						nonce: message.nonce,
						expiration: message.expiration,
						signature: Signature::from(message.signature.0),
						participant: message.participant,
					})],
				)
			},
			messages::MessageInner::WithdrawExpired(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;
				(
					sender,
					vec![StateChange::ReceiveWithdrawExpired(ReceiveWithdrawExpired {
						sender,
						message_identifier: message.message_identifier,
						canonical_identifier: CanonicalIdentifier {
							chain_identifier: message.chain_id,
							token_network_address: message.token_network_address,
							channel_identifier: message.channel_identifier,
						},
						total_withdraw: message.total_withdraw,
						nonce: message.nonce,
						expiration: message.expiration,
						participant: message.participant,
					})],
				)
			},
			messages::MessageInner::Processed(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;

				(
					sender,
					vec![StateChange::ReceiveProcessed(ReceiveProcessed {
						sender,
						message_identifier: message.message_identifier,
					})],
				)
			},
			messages::MessageInner::Delivered(message) => {
				let sender = get_sender(&message.bytes_to_sign(), &message.signature.0)?;

				// We do not send `Delivered` when reciving one. Skip the step after.
				return Ok(vec![StateChange::ReceiveDelivered(ReceiveDelivered {
					sender,
					message_identifier: message.delivered_message_identifier,
				})])
			},
			messages::MessageInner::PFSCapacityUpdate(_) |
			messages::MessageInner::PFSFeeUpdate(_) |
			messages::MessageInner::MSUpdate(_) => {
				// We should not receive those messages.
				// IGNORE
				return Ok(vec![])
			},
		};

		let sender_metadata = get_address_metadata(
			&mut self.metadata_cache,
			sender,
			self.pathfinding_service_url.clone(),
		)
		.await?;
		let mut delivered = Delivered {
			delivered_message_identifier: message.message_identifier,
			signature: Signature::default(),
		};
		let _ = delivered.sign(self.private_key.clone());
		let delivered = OutgoingMessage {
			message_identifier: message.message_identifier,
			recipient: sender,
			recipient_metadata: sender_metadata,
			inner: MessageInner::Delivered(delivered),
		};
		let _ = self.transport_sender.send(TransportServiceMessage::Enqueue((
			QueueIdentifier {
				recipient: sender,
				canonical_identifier: CANONICAL_IDENTIFIER_UNORDERED_QUEUE,
			},
			delivered,
		)));

		Ok(state_changes)
	}
}

fn get_sender(data: &[u8], signature: &[u8]) -> Result<Address, String> {
	signing::recover(&data, &signature)
		.map_err(|e| format!("Could not recover address from signature: {}", e))
}

async fn get_address_metadata(
	metadata_cache: &mut HashMap<Address, AddressMetadata>,
	address: Address,
	pathfinding_service_url: String,
) -> Result<AddressMetadata, String> {
	match metadata_cache.get(&address) {
		Some(metadata) => Ok(metadata.clone()),
		None => {
			let metadata =
				raiden_pathfinding::query_address_metadata(pathfinding_service_url, address)
					.await
					.map_err(|e| {
						format!("Could not fetch address metadata {:?}: {}", address, e)
					})?;
			metadata_cache.insert(address, metadata.clone());
			Ok(metadata)
		},
	}
}
