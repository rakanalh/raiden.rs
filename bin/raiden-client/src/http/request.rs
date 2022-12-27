use raiden::{
    primitives::{
        BlockTimeout, PaymentIdentifier, RevealTimeout, SecretHash, SettleTimeout, TokenAddress, TokenAmount,
    },
    state_machine::types::ChannelStatus,
};
use serde::Deserialize;
use web3::types::{Address, U256};

#[derive(Deserialize)]
pub struct ChannelOpenParams {
    pub registry_address: Address,
    pub partner_address: Address,
    pub token_address: TokenAddress,
    pub settle_timeout: Option<SettleTimeout>,
    pub reveal_timeout: Option<RevealTimeout>,
    pub total_deposit: Option<TokenAmount>,
}

#[derive(Deserialize)]
pub struct ChannelPatchParams {
    pub registry_address: Address,
    pub token_address: TokenAddress,
    pub partner_address: Address,
    pub total_deposit: Option<TokenAmount>,
    pub total_withdraw: Option<TokenAmount>,
    pub reveal_timeout: Option<RevealTimeout>,
    pub state: Option<ChannelStatus>,
}

#[derive(Deserialize)]
pub struct InitiatePaymentParams {
    pub amount: U256,
    pub payment_identifier: Option<PaymentIdentifier>,
    pub secret: Option<String>,
    pub secret_hash: Option<SecretHash>,
    pub lock_timeout: Option<BlockTimeout>,
}
