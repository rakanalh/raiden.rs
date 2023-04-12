use std::sync::Arc;

use futures::future::join_all;
use parking_lot::RwLock;
use raiden_state_machine::types::{
	Event,
	StateChange,
};

use crate::{
	events::EventHandler,
	manager::StateManager,
};

pub mod events;
pub mod manager;
pub mod messages;
pub mod utils;

pub struct Transitioner {
	state_manager: Arc<RwLock<StateManager>>,
	event_handler: EventHandler,
}

impl Transitioner {
	pub fn new(state_manager: Arc<RwLock<StateManager>>, event_handler: EventHandler) -> Self {
		Self { state_manager, event_handler }
	}

	pub async fn transition(&self, state_changes: Vec<StateChange>) -> Result<(), String> {
		let mut raiden_events = vec![];
		for state_change in state_changes.clone() {
			let events =
				self.state_manager.write().transition(state_change.clone()).map_err(|e| e.msg)?;
			raiden_events.extend(events);
		}
		self.trigger_state_change_effects(state_changes, raiden_events).await;
		Ok(())
	}

	async fn trigger_state_change_effects(
		&self,
		state_changes: Vec<StateChange>,
		mut events: Vec<Event>,
	) {
		let mut pfs_fee_updates = vec![];
		let mut pfs_capacity_updates = vec![];

		for state_change in state_changes {
			// Monitoring service updates
			let balance_proof = match state_change.clone() {
				StateChange::ActionInitMediator(inner) => Some(inner.balance_proof),
				StateChange::ActionInitTarget(inner) => Some(inner.balance_proof),
				StateChange::ActionTransferReroute(inner) => Some(inner.transfer.balance_proof),
				StateChange::ReceiveTransferCancelRoute(inner) =>
					Some(inner.transfer.balance_proof),
				StateChange::ReceiveLockExpired(inner) => Some(inner.balance_proof),
				StateChange::ReceiveTransferRefund(inner) => Some(inner.balance_proof),
				_ => None,
			};

			if let Some(balance_proof) = balance_proof {
				events.push(Event::SendMSUpdate(balance_proof));
			}

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
				events.push(Event::ExpireServicesAddresses(inner.block_number));
			}
		}

		for event in events.iter() {
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

		for canonical_identifier in pfs_capacity_updates {
			events.push(Event::SendPFSUpdate(canonical_identifier, false));
		}

		for canonical_identifier in pfs_fee_updates {
			events.push(Event::SendPFSUpdate(canonical_identifier, true));
		}

		let mut tasks = vec![];
		for event in events {
			tasks.push(self.event_handler.handle_event(event));
		}
		join_all(tasks).await;
	}
}
