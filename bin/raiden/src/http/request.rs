use raiden::{
    primitives::U64,
    state_machine::types::ChannelStatus,
};
use serde::Deserialize;
use web3::types::{
    Address,
    H256,
    U256,
};

#[derive(Deserialize)]
pub struct ChannelOpenParams {
    pub registry_address: Address,
    pub partner_address: Address,
    pub token_address: Address,
    pub settle_timeout: Option<U256>,
    pub reveal_timeout: Option<U64>,
    pub total_deposit: Option<U256>,
}

#[derive(Deserialize)]
pub struct ChannelPatchParams {
    pub registry_address: Address,
    pub token_address: Address,
    pub partner_address: Address,
    pub total_deposit: Option<U256>,
    pub total_withdraw: Option<U256>,
    pub reveal_timeout: Option<U64>,
    pub state: Option<ChannelStatus>,
}

#[derive(Deserialize)]
pub struct InitiatePaymentParams {
    pub amount: U256,
    pub payment_identifier: Option<u64>,
    pub secret: Option<String>,
    pub secret_hash: Option<H256>,
}
