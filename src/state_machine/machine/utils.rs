use crate::{
    errors::StateTransitionError,
    state_machine::types::{
        ChainState,
        ChannelState,
    },
};

pub(super) fn update_channel(
    chain_state: &mut ChainState,
    channel_state: ChannelState,
) -> Result<(), StateTransitionError> {
    let token_network_registries = &mut chain_state.identifiers_to_tokennetworkregistries;
    let token_network_registry = match token_network_registries.get_mut(&channel_state.token_network_registry_address) {
        Some(token_network_registry) => token_network_registry,
        None => {
            return Err(StateTransitionError {
                msg: format!(
                    "Token network registry {} was not found",
                    channel_state.token_network_registry_address
                ),
            });
        }
    };
    let token_network = match token_network_registry
        .tokennetworkaddresses_to_tokennetworks
        .get_mut(&channel_state.canonical_identifier.token_network_address)
    {
        Some(token_network) => token_network,
        None => {
            return Err(StateTransitionError {
                msg: format!(
                    "Token network {} was not found",
                    channel_state.canonical_identifier.token_network_address
                ),
            });
        }
    };

    token_network
        .channelidentifiers_to_channels
        .insert(channel_state.canonical_identifier.channel_identifier, channel_state);

    Ok(())
}
