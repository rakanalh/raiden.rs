use std::sync::Arc;

use parking_lot::RwLock;
use tokio::time::{
    sleep,
    Duration,
};
use web3::types::Address;

use crate::{
    constants::DEFAULT_RETRY_TIMEOUT,
    state_machine::views,
    state_manager::StateManager,
};

pub async fn wait_for_new_channel(
    state_manager: Arc<RwLock<StateManager>>,
    registry_address: Address,
    token_address: Address,
    partner_address: Address,
    retry_timeout: Option<u64>,
) {
    let retry_timeout = match retry_timeout {
        Some(timeout) => Duration::from_millis(timeout),
        None => Duration::from_millis(DEFAULT_RETRY_TIMEOUT),
    };
    let chain_state = state_manager.read().current_state.clone();
    let mut channel_state =
        views::get_channel_state_for(&chain_state, registry_address, token_address, partner_address);

    while let None = channel_state {
        sleep(retry_timeout).await;
        channel_state = views::get_channel_state_for(&chain_state, registry_address, token_address, partner_address);
    }
}
