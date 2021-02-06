use crate::enums::Event;
use crate::service::RaidenService;

pub struct EventHandler {}

impl EventHandler {
    pub async fn handle_event(raiden: &mut RaidenService, event: Event) -> bool {
        match event {
            Event::TokenNetworkCreated(event) => {
                let token_network_address = event.token_network.address;
				let _ = raiden.contracts_registry.add_token_network(
                    token_network_address.into(),
                    event.block_number.into(),
                );
				true
            }
        }
    }
}
