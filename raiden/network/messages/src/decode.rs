use std::collections::HashMap;

use super::messages::{
	IncomingMessage,
	LockedTransfer,
};
use crate::messages::{
	Delivered,
	LockExpired,
	Processed,
	SecretRequest,
	SecretReveal,
	Unlock,
	WithdrawConfirmation,
	WithdrawExpired,
	WithdrawRequest,
};

impl TryFrom<String> for IncomingMessage {
	type Error = String;

	fn try_from(body: String) -> Result<Self, Self::Error> {
		let map: HashMap<String, serde_json::Value> =
			serde_json::from_str(&body).map_err(|e| format!("Could not parse json {}", e))?;

		let message_type = map
			.get("type")
			.map(|v| v.as_str())
			.flatten()
			.ok_or(format!("Message has no type"))?;

		match message_type {
			"LockedTransfer" => {
				let locked_transfer: LockedTransfer = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse LockedTransfer message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: locked_transfer.message_identifier,
					inner: crate::messages::MessageInner::LockedTransfer(locked_transfer),
				})
			},
			"LockExpired" => {
				let lock_expired: LockExpired = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse LockExpired message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: lock_expired.message_identifier,
					inner: crate::messages::MessageInner::LockExpired(lock_expired),
				})
			},
			"SecretRequest" => {
				let secret_request: SecretRequest = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse SecretRequest message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: secret_request.message_identifier,
					inner: crate::messages::MessageInner::SecretRequest(secret_request),
				})
			},
			"RevealSecret" => {
				let secret_reveal: SecretReveal = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse RevealSecret message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: secret_reveal.message_identifier,
					inner: crate::messages::MessageInner::SecretReveal(secret_reveal),
				})
			},
			"Unlock" => {
				let unlock: Unlock = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse Unlock message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: unlock.message_identifier,
					inner: crate::messages::MessageInner::Unlock(unlock),
				})
			},
			"WithdrawRequest" => {
				let withdraw_request: WithdrawRequest = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse WithdrawRequest message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: withdraw_request.message_identifier,
					inner: crate::messages::MessageInner::WithdrawRequest(withdraw_request),
				})
			},
			"WithdrawConfirmation" => {
				let withdraw_confirmation: WithdrawConfirmation = serde_json::from_str(&body)
					.map_err(|e| {
						format!("Could not parse WithdrawConfirmation message: {:?}", e)
					})?;
				return Ok(IncomingMessage {
					message_identifier: withdraw_confirmation.message_identifier,
					inner: crate::messages::MessageInner::WithdrawConfirmation(
						withdraw_confirmation,
					),
				})
			},
			"WithdrawExpired" => {
				let withdraw_expired: WithdrawExpired = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse WithdrawExpired message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: withdraw_expired.message_identifier,
					inner: crate::messages::MessageInner::WithdrawExpired(withdraw_expired),
				})
			},
			"Processed" => {
				let processed: Processed = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse Processed message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: processed.message_identifier,
					inner: crate::messages::MessageInner::Processed(processed),
				})
			},
			"Delivered" => {
				let delivered: Delivered = serde_json::from_str(&body)
					.map_err(|e| format!("Could not parse Delivered message: {:?}", e))?;
				return Ok(IncomingMessage {
					message_identifier: delivered.delivered_message_identifier,
					inner: crate::messages::MessageInner::Delivered(delivered),
				})
			},
			_ => return Err(format!("Message type {} is unknown", message_type)),
		};
	}
}
