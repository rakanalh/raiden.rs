use std::collections::HashMap;

use tokio::sync::oneshot;
use web3::types::Address;

use crate::types::PaymentIdentifier;

pub struct Payment {
    identifier: PaymentIdentifier,
    notifier: oneshot::Sender<()>,
}

pub struct PaymentsRegistry {
    payments: HashMap<Address, Payment>,
}

impl PaymentsRegistry {
    pub fn new() -> Self {
        Self {
            payments: HashMap::new(),
        }
    }

    pub fn register(&mut self, target: Address, identifier: PaymentIdentifier) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        self.payments.insert(
            target,
            Payment {
                identifier,
                notifier: sender,
            },
        );
        receiver
    }
}
