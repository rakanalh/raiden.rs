use serde::Serialize;

#[derive(Serialize)]
pub enum ChannelStatus {
    Closed,
    Closing,
    Opened,
    Settled,
    Settling,
    Unusable,
}
