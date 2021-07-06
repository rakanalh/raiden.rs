use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};
use web3::types::Address;

pub type FeeAmount = u32;
pub type ProportionalFeeAmount = u32;

pub const DEFAULT_MEDIATION_FLAT_FEE: FeeAmount = 0;
pub const DEFAULT_MEDIATION_PROPORTIONAL_FEE: ProportionalFeeAmount = 4000; // 0.4% in parts per million
pub const DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE: ProportionalFeeAmount = 3000; // 0.3% in parts per million

#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct MediationFeeConfig {
    pub token_to_flat_fee: HashMap<Address, FeeAmount>,
    token_to_proportional_fee: HashMap<Address, ProportionalFeeAmount>,
    token_to_proportional_imbalance_fee: HashMap<Address, ProportionalFeeAmount>,
    cap_meditation_fees: bool,
}

impl MediationFeeConfig {
    pub fn get_flat_fee(&self, token_address: &Address) -> FeeAmount {
        *self
            .token_to_flat_fee
            .get(token_address)
            .unwrap_or(&DEFAULT_MEDIATION_FLAT_FEE)
    }

    pub fn get_proportional_fee(&self, token_address: &Address) -> ProportionalFeeAmount {
        *self
            .token_to_proportional_fee
            .get(token_address)
            .unwrap_or(&DEFAULT_MEDIATION_PROPORTIONAL_FEE)
    }

    pub fn get_proportional_imbalance_fee(self, token_address: &Address) -> ProportionalFeeAmount {
        *self
            .token_to_proportional_imbalance_fee
            .get(token_address)
            .unwrap_or(&DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE)
    }
}
