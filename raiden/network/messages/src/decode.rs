use std::collections::HashMap;

use super::messages::{
	IncomingMessage,
	LockedTransfer,
};
use crate::messages::{
	LockExpired,
	Processed,
	SecretRequest,
	SecretReveal,
	Unlock,
	WithdrawConfirmation,
	WithdrawExpired,
	WithdrawRequest,
};

pub struct MessageDecoder {}

impl MessageDecoder {
	pub fn decode(body: serde_json::Value) -> Result<IncomingMessage, String> {
		let s = body.as_str().ok_or(format!("Could not convert message to string"))?.to_owned();

		let map: HashMap<String, serde_json::Value> =
			serde_json::from_str(&s).map_err(|e| format!("Could not parse json {}", e))?;

		let message_type = map
			.get("type")
			.map(|v| v.as_str())
			.flatten()
			.ok_or(format!("Message has no type"))?;

		match message_type {
			"LockedTransfer" => {
				let locked_transfer: LockedTransfer = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: locked_transfer.message_identifier,
					inner: crate::messages::MessageInner::LockedTransfer(locked_transfer),
				})
			},
			"LockExpired" => {
				let lock_expired: LockExpired = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: lock_expired.message_identifier,
					inner: crate::messages::MessageInner::LockExpired(lock_expired),
				})
			},
			"SecretRequest" => {
				let secret_request: SecretRequest = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: secret_request.message_identifier,
					inner: crate::messages::MessageInner::SecretRequest(secret_request),
				})
			},
			"SecretReveal" => {
				let secret_reveal: SecretReveal = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: secret_reveal.message_identifier,
					inner: crate::messages::MessageInner::SecretReveal(secret_reveal),
				})
			},
			"Unlock" => {
				let unlock: Unlock = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: unlock.message_identifier,
					inner: crate::messages::MessageInner::Unlock(unlock),
				})
			},
			"WithdrawRequest" => {
				let withdraw_request: WithdrawRequest = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: withdraw_request.message_identifier,
					inner: crate::messages::MessageInner::WithdrawRequest(withdraw_request),
				})
			},
			"WithdrawConfirmation" => {
				let withdraw_confirmation: WithdrawConfirmation = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: withdraw_confirmation.message_identifier,
					inner: crate::messages::MessageInner::WithdrawConfirmation(
						withdraw_confirmation,
					),
				})
			},
			"WithdrawExpired" => {
				let withdraw_expired: WithdrawExpired = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: withdraw_expired.message_identifier,
					inner: crate::messages::MessageInner::WithdrawExpired(withdraw_expired),
				})
			},
			"Processed" => {
				let processed: Processed = serde_json::from_str(&s).unwrap();
				return Ok(IncomingMessage {
					message_identifier: processed.message_identifier,
					inner: crate::messages::MessageInner::Processed(processed),
				})
			},
			_ => return Err(format!("Message type {} is unknown", message_type)),
		};
	}
}
