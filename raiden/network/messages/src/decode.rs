use std::{
	collections::HashMap,
	sync::Arc,
};

use raiden_blockchain::{
	keys::{
		self,
		PrivateKey,
	},
	proxies::ProxyManager,
};
use raiden_primitives::types::{
	Address,
	Bytes,
	PaymentIdentifier,
	Secret,
	SecretHash,
	SecretRegistryAddress,
	Signature,
	TokenAmount,
};
use raiden_state_machine::{
	machine::channel::utils::hash_balance_data,
	types::{
		ActionInitMediator,
		ActionInitTarget,
		AddressMetadata,
		BalanceProofState,
		CanonicalIdentifier,
		ChainState,
		DecryptedSecret,
		HashTimeLockState,
		HopState,
		LockedTransferState,
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
use web3::signing::{
	self,
	keccak256,
};

use super::messages::{
	IncomingMessage,
	LockedTransfer,
};
use crate::messages::{
	LockExpired,
	Processed,
	SecretRequest,
	SecretReveal,
	SignedEnvelopeMessage,
	SignedMessage,
	Unlock,
	WithdrawConfirmation,
	WithdrawExpired,
	WithdrawRequest,
};

#[derive(Clone)]
pub struct MessageDecoder {
	pub private_key: PrivateKey,
	pub our_address: Address,
	pub proxy_manager: Arc<ProxyManager>,
	pub secret_registry_address: SecretRegistryAddress,
	pub pathfinding_service_url: String,
}

impl MessageDecoder {
	pub async fn decode(
		&self,
		chain_state: ChainState,
		body: String,
	) -> Result<Vec<StateChange>, String> {
		let message = self.into_message(body)?;

		match message.inner {
			crate::messages::MessageInner::LockedTransfer(message) => {
				let data = message.bytes_to_sign();
				let sender = self.get_sender(&data, &message.signature)?;
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
					signature: Some(Signature::from_slice(&message.signature)),
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

				if message.target == self.our_address {
					let mut init_target = ActionInitTarget {
						sender,
						balance_proof,
						from_hop,
						transfer: transfer.clone(),
						received_valid_secret: false,
					};

					if let Some(encrypted_secret) = message.metadata.secret {
						let decrypted_secret =
							decrypt_secret(encrypted_secret.into_bytes(), &self.private_key)?;
						if transfer.lock.amount < decrypted_secret.amount ||
							transfer.payment_identifier != decrypted_secret.payment_identifier
						{
							return Err(format!("Invalid Secret"))
						}

						init_target.received_valid_secret = true;
						return Ok(vec![
							StateChange::ActionInitTarget(init_target),
							StateChange::ReceiveSecretReveal(ReceiveSecretReveal {
								sender,
								secret: decrypted_secret.secret,
								secrethash: message.lock.secrethash.unwrap_or_default(),
							}),
						])
					}
					return Ok(vec![StateChange::ActionInitTarget(init_target)])
				} else {
					let mut filtered_route_states = vec![];
					for route_state in transfer.route_states.iter() {
						if let Some(next_hope_address) = route_state.hop_after(self.our_address) {
							if let Some(channel_state) =
								views::get_channel_by_token_network_and_partner(
									&chain_state,
									transfer
										.balance_proof
										.canonical_identifier
										.token_network_address,
									next_hope_address,
								) {
								filtered_route_states.push(route_state.clone());
							}
						}
					}
					return Ok(vec![StateChange::ActionInitMediator(ActionInitMediator {
						sender,
						balance_proof,
						from_hop,
						candidate_route_states: filtered_route_states,
						from_transfer: transfer,
					})])
				}
			},
			crate::messages::MessageInner::LockExpired(message) => {
				let sender =
					self.get_sender(&message.message_hash().as_bytes(), &message.signature)?;
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
					signature: Some(Signature::from_slice(&message.signature)),
					sender: Some(sender),
				};
				Ok(vec![StateChange::ReceiveLockExpired(ReceiveLockExpired {
					sender,
					secrethash: message.secrethash,
					message_identifier: message.message_identifier,
					balance_proof,
				})])
			},
			crate::messages::MessageInner::SecretRequest(message) => {
				let sender = self.get_sender(&message.bytes_to_sign(), &message.signature)?;
				Ok(vec![StateChange::ReceiveSecretRequest(ReceiveSecretRequest {
					sender,
					secrethash: message.secrethash,
					payment_identifier: message.payment_identifier,
					amount: message.amount,
					expiration: message.expiration,
					revealsecret: None,
				})])
			},
			crate::messages::MessageInner::SecretReveal(message) => {
				let sender = self.get_sender(&message.bytes_to_sign(), &message.signature)?;
				let mut secrethash = vec![];
				secrethash.extend_from_slice(&keccak256(&message.secret.0));
				Ok(vec![StateChange::ReceiveSecretReveal(ReceiveSecretReveal {
					sender,
					secrethash: SecretHash::from_slice(&secrethash),
					secret: message.secret,
				})])
			},
			crate::messages::MessageInner::Unlock(message) => {
				let sender = self.get_sender(&message.bytes_to_sign(), &message.signature)?;
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
					signature: Some(Signature::from_slice(&message.signature)),
					sender: Some(sender),
				};
				let mut secrethash = vec![];
				secrethash.extend_from_slice(&keccak256(&message.secret.0));
				Ok(vec![StateChange::ReceiveUnlock(ReceiveUnlock {
					sender,
					balance_proof,
					secrethash: SecretHash::from_slice(&secrethash),
					message_identifier: message.message_identifier,
					secret: message.secret,
				})])
			},
			crate::messages::MessageInner::WithdrawRequest(message) => {
				let sender = self.get_sender(&message.bytes_to_sign(), &message.signature)?;

				let sender_metadata = raiden_pathfinding::query_address_metadata(
					self.pathfinding_service_url.clone(),
					sender,
				)
				.await
				.map_err(|e| format!("Could not fetch address metadata: {:?}", sender))?;

				Ok(vec![StateChange::ReceiveWithdrawRequest(ReceiveWithdrawRequest {
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
					signature: Signature::from_slice(&message.signature),
					participant: message.participant,
					coop_settle: message.coop_settle,
					sender_metadata: Some(sender_metadata),
				})])
			},
			crate::messages::MessageInner::WithdrawConfirmation(message) => {
				let sender = self.get_sender(&message.bytes_to_sign(), &message.signature)?;
				Ok(vec![StateChange::ReceiveWithdrawConfirmation(ReceiveWithdrawConfirmation {
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
					signature: Signature::from_slice(&message.signature),
					participant: message.participant,
				})])
			},
			crate::messages::MessageInner::WithdrawExpired(message) => {
				let sender = self.get_sender(&message.bytes_to_sign(), &message.signature)?;
				Ok(vec![StateChange::ReceiveWithdrawExpired(ReceiveWithdrawExpired {
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
				})])
			},
			crate::messages::MessageInner::Processed(message) => {
				let sender = self.get_sender(&message.bytes_to_sign(), &message.signature)?;
				Ok(vec![StateChange::ReceiveProcessed(ReceiveProcessed {
					sender,
					message_identifier: message.message_identifier,
				})])
			},
		}
	}

	pub fn into_message(&self, body: String) -> Result<IncomingMessage, String> {
		let map: HashMap<String, serde_json::Value> =
			serde_json::from_str(&body).map_err(|e| format!("Could not parse json {}", e))?;

		let message_type = map
			.get("type")
			.map(|v| v.as_str())
			.flatten()
			.ok_or(format!("Message has no type"))?;

		match message_type {
			"LockedTransfer" => {
				let locked_transfer: LockedTransfer = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: locked_transfer.message_identifier,
					inner: crate::messages::MessageInner::LockedTransfer(locked_transfer),
				})
			},
			"LockExpired" => {
				let lock_expired: LockExpired = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: lock_expired.message_identifier,
					inner: crate::messages::MessageInner::LockExpired(lock_expired),
				})
			},
			"SecretRequest" => {
				let secret_request: SecretRequest = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: secret_request.message_identifier,
					inner: crate::messages::MessageInner::SecretRequest(secret_request),
				})
			},
			"SecretReveal" => {
				let secret_reveal: SecretReveal = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: secret_reveal.message_identifier,
					inner: crate::messages::MessageInner::SecretReveal(secret_reveal),
				})
			},
			"Unlock" => {
				let unlock: Unlock = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: unlock.message_identifier,
					inner: crate::messages::MessageInner::Unlock(unlock),
				})
			},
			"WithdrawRequest" => {
				let withdraw_request: WithdrawRequest = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: withdraw_request.message_identifier,
					inner: crate::messages::MessageInner::WithdrawRequest(withdraw_request),
				})
			},
			"WithdrawConfirmation" => {
				let withdraw_confirmation: WithdrawConfirmation =
					serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: withdraw_confirmation.message_identifier,
					inner: crate::messages::MessageInner::WithdrawConfirmation(
						withdraw_confirmation,
					),
				})
			},
			"WithdrawExpired" => {
				let withdraw_expired: WithdrawExpired = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: withdraw_expired.message_identifier,
					inner: crate::messages::MessageInner::WithdrawExpired(withdraw_expired),
				})
			},
			"Processed" => {
				let processed: Processed = serde_json::from_str(&body).unwrap();
				return Ok(IncomingMessage {
					message_identifier: processed.message_identifier,
					inner: crate::messages::MessageInner::Processed(processed),
				})
			},
			_ => return Err(format!("Message type {} is unknown", message_type)),
		};
	}

	fn get_sender(&self, data: &[u8], signature: &[u8]) -> Result<Address, String> {
		keys::recover(&data, &signature)
			.map_err(|e| format!("Could not recover address from signature: {}", e))
	}
}

pub fn encrypt_secret(
	secret: Secret,
	target_metadata: AddressMetadata,
	amount: TokenAmount,
	payment_identifier: PaymentIdentifier,
) -> Result<Bytes, String> {
	let message = target_metadata.user_id;
	let signature = hex::decode(target_metadata.displayname)
		.map_err(|e| format!("Could not decode signature: {:?}", e))?;
	let public_key = signing::recover(message.as_bytes(), &signature, 0)
		.map_err(|e| format!("Could not recover public key: {:?}", e))?;

	let data = DecryptedSecret { secret, amount, payment_identifier };

	let json = serde_json::to_string(&data)
		.map_err(|e| format!("Could not serialize encrypted secret"))?;

	Ok(Bytes(
		keys::encrypt(public_key.as_bytes(), json.as_bytes())
			.map_err(|e| format!("Could not encrypt secret: {:?}", e))?,
	))
}

pub fn decrypt_secret(
	encrypted_secret: Vec<u8>,
	private_key: &PrivateKey,
) -> Result<DecryptedSecret, String> {
	let decrypted_secret = keys::decrypt(&private_key, &encrypted_secret)
		.map_err(|e| format!("Could not decrypt secret: {:?}", e))?;
	let json = std::str::from_utf8(&decrypted_secret)
		.map_err(|e| format!("Invalid UTF-8 sequence: {}", e))?;
	serde_json::from_str(json).map_err(|e| format!("Could not deserialize secret: {:?}", e))
}
