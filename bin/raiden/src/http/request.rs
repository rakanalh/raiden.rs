use serde::Deserialize;
use web3::types::{
    Address,
    U256,
};

#[derive(Deserialize)]
pub struct ChannelOpenParams {
    pub registry_address: Address,
    pub partner_address: Address,
    pub token_address: Address,
    pub settle_timeout: Option<U256>,
    pub reveal_timeout: Option<U256>,
    pub total_deposit: Option<U256>,
}
