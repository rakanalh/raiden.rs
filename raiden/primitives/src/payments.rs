#![warn(clippy::missing_docs_in_private_items)]

use std::collections::HashMap;

use tokio::sync::oneshot;

use crate::types::{
	Address,
	PaymentIdentifier,
	TokenAmount,
	TokenNetworkAddress,
};

pub enum PaymentStatus {
	Success(Address, PaymentIdentifier),
	Error(Address, PaymentIdentifier, String),
}

pub struct Payment {
	pub identifier: PaymentIdentifier,
	pub token_network_address: TokenNetworkAddress,
	pub amount: TokenAmount,
	pub notifier: Option<oneshot::Sender<PaymentStatus>>,
}

pub struct PaymentsRegistry {
	payments: HashMap<Address, HashMap<PaymentIdentifier, Payment>>,
}

impl PaymentsRegistry {
	pub fn new() -> Self {
		Self { payments: HashMap::new() }
	}

	pub fn get(&self, target: Address, identifier: PaymentIdentifier) -> Option<&Payment> {
		if let Some(payments) = self.payments.get(&target) {
			return payments.get(&identifier)
		}
		None
	}

	pub fn register(
		&mut self,
		token_network_address: TokenNetworkAddress,
		target: Address,
		identifier: PaymentIdentifier,
		amount: TokenAmount,
	) -> oneshot::Receiver<PaymentStatus> {
		let (sender, receiver) = oneshot::channel();

		if self.payments.get(&target).is_none() {
			self.payments.insert(target, HashMap::new());
		}

		let payments = self.payments.get_mut(&target).expect("Just created above");
		payments.insert(
			identifier,
			Payment { identifier, token_network_address, amount, notifier: Some(sender) },
		);
		receiver
	}

	pub fn complete(&mut self, status: PaymentStatus) {
		let (target, identifier) = match status {
			PaymentStatus::Success(target, identifier) => (target, identifier),
			PaymentStatus::Error(target, identifier, _) => (target, identifier),
		};
		let payments = match self.payments.get_mut(&target) {
			Some(payments) => payments,
			None => return,
		};

		let payment = match payments.get_mut(&identifier) {
			Some(payment) => payment,
			None => return,
		};

		if let Some(notifier) = payment.notifier.take() {
			let _ = notifier.send(status);
		}
	}
}
