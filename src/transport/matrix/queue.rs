use std::time::Duration;

use tokio::{
    select,
    sync::mpsc::{
        self,
        UnboundedReceiver,
        UnboundedSender,
    },
    time::sleep,
};

use crate::{
    primitives::QueueIdentifier,
    transport::messages::{
        Message,
        TransportServiceMessage,
    },
};

pub struct RetryMessageQueue {
    pub identifier: QueueIdentifier,
    queue: Vec<Message>,
    transport_sender: UnboundedSender<TransportServiceMessage>,
    receiver: UnboundedReceiver<Message>,
}

impl RetryMessageQueue {
    pub fn new(
        identifier: QueueIdentifier,
        transport_sender: UnboundedSender<TransportServiceMessage>,
    ) -> (Self, UnboundedSender<Message>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                queue: vec![],
                identifier,
                receiver,
                transport_sender,
            },
            sender,
        )
    }

    pub fn enqueue(&mut self, message: Message) {
        self.queue.push(message);
    }

    pub async fn run(mut self) {
        let delay = sleep(Duration::from_millis(1000));
        tokio::pin!(delay);

        loop {
            select! {
                Some(message) = self.receiver.recv() => {
                    self.queue.push(message);
                }
                _ = &mut delay => {
                    if !self.queue.is_empty() {
                        let message = self.queue.first().expect("should have a message").clone();
                        let _ = self.transport_sender.send(TransportServiceMessage::Send(message));
                    }
                }
            }
        }
    }
}
