use std::collections::HashMap;

use tokio::sync::oneshot;
use web3::types::Address;

use crate::primitives::U64;

pub struct Payment {
    identifier: U64,
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

    pub fn register(&mut self, target: Address, identifier: U64) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        self.payments.insert(target, Payment {
            identifier,
            notifier: sender,
        });
        receiver
    }
}
