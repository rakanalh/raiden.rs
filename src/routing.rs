use web3::types::{
    Address,
    U256,
};

use crate::state_machine::{
    types::ChainState,
    views,
};

pub async fn get_best_routes(
    chain_state: ChainState,
    token_network_address: Address,
    to_address: Address,
    amount: U256,
) -> Result<(), String> {
    let token_network = match views::get_token_network_by_address(&chain_state, token_network_address) {
        Some(token_network) => token_network,
        None => return Err("Token network does not exist".to_owned()),
    };

    // Always use a direct channel if available:
    // - There are no race conditions and the capacity is guaranteed to be
    //   available.
    // - There will be no mediation fees
    // - The transfer will be faster
    if token_network
        .partneraddresses_to_channelidentifiers
        .contains_key(&to_address)
    {
        for channel_id in token_network.partneraddresses_to_channelidentifiers[&to_address].iter() {
            let channel_state = &token_network.channelidentifiers_to_channels[&channel_id];

            // Direct channels don't have fees.
            let payment_with_fee_amount = amount;
            if channel_state.is_usable_for_new_transfer(payment_with_fee_amount, None) {}
        }
    }

    Ok(())
}
