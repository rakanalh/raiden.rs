use crate::{
    constants::CHANNEL_STATES_UP_TO_CLOSE,
    primitives::{
        BlockExpiration,
        BlockHash,
        Secret,
    },
    state_machine::types::{
        ChannelState,
        ContractSendEventInner,
        ContractSendSecretReveal,
        Event,
    },
};

pub(super) fn events_for_onchain_secretreveal(
    channel_state: &ChannelState,
    secret: Secret,
    expiration: BlockExpiration,
    block_hash: BlockHash,
) -> Vec<Event> {
    let mut events = vec![];

    if CHANNEL_STATES_UP_TO_CLOSE.contains(&channel_state.status()) {
        let reveal_event = ContractSendSecretReveal {
            inner: ContractSendEventInner {
                triggered_by_blockhash: block_hash,
            },
            expiration,
            secret,
        };

        events.push(reveal_event.into());
    }

    events
}
