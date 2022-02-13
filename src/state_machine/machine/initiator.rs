use std::ops::Div;

use crate::{
    constants::{
        DEFAULT_MEDIATION_FEE_MARGIN,
        MAX_MEDIATION_FEE_PERC,
        PAYMENT_AMOUNT_BASED_FEE_MARGIN,
    },
    errors::StateTransitionError,
    primitives::{
        BlockNumber,
        FeeAmount,
        MessageIdentifier,
        TokenAmount,
    },
    state_machine::{
        types::{
            ChainState,
            ChannelState,
            Event,
            EventPaymentSentFailed,
            InitiatorTransferState,
            RouteState,
            SendLockedTransfer,
            TransferDescriptionWithSecretState,
        },
        views,
    },
};

use super::{
    channel,
    routes,
};

pub(super) type TransitionResult = std::result::Result<InitiatorTransition, StateTransitionError>;

pub struct InitiatorTransition {
    pub new_state: Option<InitiatorTransferState>,
    pub events: Vec<Event>,
}

fn calculate_fee_margin(payment_amount: TokenAmount, estimated_fee: FeeAmount) -> FeeAmount {
    if estimated_fee.is_zero() {
        return FeeAmount::zero();
    }

    ((estimated_fee * DEFAULT_MEDIATION_FEE_MARGIN.0) / DEFAULT_MEDIATION_FEE_MARGIN.1)
        + ((payment_amount * PAYMENT_AMOUNT_BASED_FEE_MARGIN.0) / PAYMENT_AMOUNT_BASED_FEE_MARGIN.1)
}

fn calculate_safe_amount_with_fee(payment_amount: TokenAmount, estimated_fee: FeeAmount) -> TokenAmount {
    payment_amount + estimated_fee + calculate_fee_margin(payment_amount, estimated_fee)
}

fn update_channel(mut chain_state: ChainState, channel_state: ChannelState) -> Result<(), StateTransitionError> {
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

fn send_locked_transfer(
    transfer_description: TransferDescriptionWithSecretState,
    channel_state: ChannelState,
    route_state: RouteState,
    route_states: Vec<RouteState>,
    message_identifier: MessageIdentifier,
    block_number: BlockNumber,
) -> Result<(ChannelState, SendLockedTransfer), StateTransitionError> {
    let lock_expiration = channel::get_safe_initial_expiration(
        block_number,
        channel_state.reveal_timeout,
        transfer_description.lock_timeout,
    );
    let total_amount = calculate_safe_amount_with_fee(transfer_description.amount, route_state.estimated_fee);
    let recipient_address = channel_state.partner_state.address;
    let recipient_metadata = channel::get_address_metadata(recipient_address, route_states.clone());

    channel::send_locked_transfer(
        channel_state,
        transfer_description.initiator,
        transfer_description.target,
        total_amount,
        lock_expiration,
        transfer_description.secrethash,
        message_identifier,
        transfer_description.payment_identifier,
        routes::prune_route_table(route_states, route_state),
        recipient_metadata,
    )
}

pub fn try_new_route(
    mut chain_state: ChainState,
    candidate_route_states: Vec<RouteState>,
    transfer_description: TransferDescriptionWithSecretState,
) -> TransitionResult {
    let mut route_fee_exceeds_max = false;

    let selected = loop {
        let route_state = match candidate_route_states.iter().next() {
            Some(route_state) => route_state,
            None => break None,
        };

        let next_hop_address = match route_state.next_hop_address() {
            Some(next_hop_address) => next_hop_address,
            None => continue,
        };

        let candidate_channel_state = match views::get_channel_by_token_network_and_partner(
            &chain_state,
            transfer_description.token_network_address,
            next_hop_address,
        ) {
            Some(channel_state) => channel_state.clone(),
            None => continue,
        };

        let amount_with_fee = calculate_safe_amount_with_fee(transfer_description.amount, route_state.estimated_fee);

        let max_amount_limit = transfer_description.amount
            + (transfer_description
                .amount
                .saturating_mul(MAX_MEDIATION_FEE_PERC.0.into())
                .div(MAX_MEDIATION_FEE_PERC.1));
        if amount_with_fee > max_amount_limit {
            route_fee_exceeds_max = true;
            continue;
        }

        let is_channel_usable =
            candidate_channel_state.is_usable_for_new_transfer(amount_with_fee, transfer_description.lock_timeout);
        if is_channel_usable {
            break Some((route_state, candidate_channel_state));
        }
    };

    let (initiator_state, events) = if let Some((route_state, channel_state)) = selected {
        let message_identifier = chain_state.pseudo_random_number_generator.next();
        let (channel_state, locked_transfer_event) = send_locked_transfer(
            transfer_description.clone(),
            channel_state,
            route_state.clone(),
            candidate_route_states.clone(),
            message_identifier,
            chain_state.block_number,
        )?;
        let initiator_state = InitiatorTransferState {
            route: route_state.clone(),
            transfer_description,
            channel_identifier: channel_state.canonical_identifier.channel_identifier,
            transfer: locked_transfer_event.transfer.clone(),
        };
        update_channel(chain_state, channel_state)?;
        (
            Some(initiator_state),
            vec![Event::SendLockedTransfer(locked_transfer_event)],
        )
    } else {
        let mut reason = "None of the available routes could be used".to_owned();
        if route_fee_exceeds_max {
            reason += " and at least one of them exceeded the maximum fee limit";
        }
        let transfer_failed = Event::PaymentSentFailed(EventPaymentSentFailed {
            token_network_registry_address: transfer_description.token_network_registry_address,
            token_network_address: transfer_description.token_network_address,
            identifier: transfer_description.payment_identifier,
            target: transfer_description.target,
            reason,
        });

        (None, vec![transfer_failed])
    };

    Ok(InitiatorTransition {
        new_state: initiator_state,
        events,
    })
}
