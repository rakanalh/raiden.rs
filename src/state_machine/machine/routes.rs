use crate::state_machine::types::RouteState;

pub fn prune_route_table(route_states: Vec<RouteState>, selected_route: RouteState) -> Vec<RouteState> {
    route_states
        .iter()
        .filter(|route_state| route_state.next_hop_address() == selected_route.next_hop_address())
        .map(|route_state| RouteState {
            route: route_state.route[1..].to_vec(),
            ..route_state.clone()
        })
        .collect()
}
