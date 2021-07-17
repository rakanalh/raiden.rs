use serde::Serialize;

use raiden::{
    primitives::U64,
    state_machine::{
        types::{
            ChannelState,
            ChannelStatus,
        },
        views,
    },
};
use web3::types::{
    Address,
    U256,
};

#[derive(Serialize)]
pub struct AddressResponse {
    pub our_address: Address,
}

#[derive(Serialize)]
pub struct ChannelResponse {
    channel_identifier: U256,
    token_network_address: Address,
    token_address: Address,
    partner_address: Address,
    settle_timeout: U256,
    reveal_timeout: U64,
    balance: U256,
    state: ChannelStatus,
    total_deposit: U256,
    total_withdraw: U256,
}

#[derive(Serialize)]
pub struct CreateChannelResponse {
    token_address: Address,
    partner_address: Address,
    reveal_timeout: U64,
    settle_timeout: U64,
    total_deposit: U256,
}

impl From<ChannelState> for ChannelResponse {
    fn from(channel: ChannelState) -> Self {
        ChannelResponse {
            channel_identifier: channel.canonical_identifier.channel_identifier,
            token_network_address: channel.canonical_identifier.token_network_address,
            token_address: channel.token_address,
            partner_address: channel.partner_state.address,
            settle_timeout: channel.settle_timeout,
            reveal_timeout: channel.reveal_timeout,
            total_deposit: channel.our_state.contract_balance,
            total_withdraw: channel.our_state.total_withdraw(),
            state: views::get_channel_status(&channel),
            balance: views::get_channel_balance(&channel.our_state, &channel.partner_state),
        }
    }
}
