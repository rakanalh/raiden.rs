use std::sync::Arc;

use parking_lot::RwLock;
use raiden_primitives::types::CanonicalIdentifier;
use raiden_state_machine::types::{
	BalanceProofState,
	Event,
	StateChange,
};
use tracing::error;

use crate::{
	events::EventHandler,
	manager::StateManager,
};

pub mod events;
pub mod manager;
pub mod messages;

pub struct Transitioner {
	state_manager: Arc<RwLock<StateManager>>,
	event_handler: EventHandler,
}

impl Transitioner {
	pub fn new(state_manager: Arc<RwLock<StateManager>>, event_handler: EventHandler) -> Self {
		Self { state_manager, event_handler }
	}

	// TODO: Should return Result
	pub async fn transition(&self, state_change: StateChange) {
		let transition_result = self.state_manager.write().transition(state_change.clone());
		match transition_result {
			Ok(events) => {
				self.trigger_state_change_effects(state_change, events.clone()).await;
				for event in events {
					self.event_handler.handle_event(event).await;
				}
			},
			Err(e) => {
				// Maybe use an informant service for error logging
				error!("Error transitioning: {:?}", e);
			},
		}
	}

	async fn trigger_state_change_effects(&self, state_change: StateChange, events: Vec<Event>) {
		let mut pfs_fee_updates = vec![];
		let mut pfs_capacity_updates = vec![];

		// Monitoring service updates
		let balance_proof = match state_change.clone() {
			StateChange::ActionInitMediator(inner) => Some(inner.balance_proof),
			StateChange::ActionInitTarget(inner) => Some(inner.balance_proof),
			StateChange::ActionTransferReroute(inner) => Some(inner.transfer.balance_proof),
			StateChange::ReceiveTransferCancelRoute(inner) => Some(inner.transfer.balance_proof),
			StateChange::ReceiveLockExpired(inner) => Some(inner.balance_proof),
			StateChange::ReceiveTransferRefund(inner) => Some(inner.balance_proof),
			_ => None,
		};

		match state_change.clone() {
			StateChange::ContractReceiveChannelDeposit(inner) => {
				pfs_fee_updates.push(inner.canonical_identifier);
			},
			StateChange::ReceiveUnlock(inner) => {
				pfs_capacity_updates.push(inner.balance_proof.canonical_identifier);
			},
			StateChange::ReceiveWithdrawRequest(inner) => {
				pfs_fee_updates.push(inner.canonical_identifier);
			},
			StateChange::ReceiveWithdrawExpired(inner) => {
				pfs_fee_updates.push(inner.canonical_identifier);
			},
			StateChange::ReceiveLockExpired(inner) => {
				pfs_capacity_updates.push(inner.balance_proof.canonical_identifier);
			},
			StateChange::ReceiveTransferCancelRoute(inner) => {
				pfs_capacity_updates.push(inner.transfer.balance_proof.canonical_identifier);
			},
			StateChange::ReceiveTransferRefund(inner) => {
				pfs_capacity_updates.push(inner.balance_proof.canonical_identifier);
			},
			_ => {},
		};

		if let StateChange::Block(inner) = state_change {
			self.event_handler
				.handle_event(Event::ExpireServicesAddresses(inner.block_number))
				.await;
		}

		for event in events {
			match event {
				Event::SendWithdrawRequest(inner) => {
					pfs_fee_updates.push(inner.canonical_identifier.clone());
				},
				Event::SendWithdrawExpired(inner) => {
					pfs_fee_updates.push(inner.canonical_identifier.clone());
				},
				Event::SendUnlock(inner) => {
					pfs_capacity_updates.push(inner.canonical_identifier.clone());
				},
				Event::SendLockedTransfer(inner) => {
					pfs_capacity_updates.push(inner.canonical_identifier.clone());
				},
				_ => {},
			}
		}

		if let Some(balance_proof) = balance_proof {
			self.update_monitoring_service(balance_proof).await;
		}

		for canonical_identifier in pfs_capacity_updates {
			self.send_pfs_update(canonical_identifier, false).await;
		}

		for canonical_identifier in pfs_fee_updates {
			self.send_pfs_update(canonical_identifier, true).await;
		}
	}

	async fn update_monitoring_service(&self, balance_proof: BalanceProofState) {
		self.event_handler.handle_event(Event::SendMSUpdate(balance_proof)).await;
	}

	async fn send_pfs_update(
		&self,
		canonical_identifier: CanonicalIdentifier,
		update_fee_schedule: bool,
	) {
		self.event_handler
			.handle_event(Event::SendPFSUpdate(canonical_identifier, update_fee_schedule))
			.await;
	}
}
