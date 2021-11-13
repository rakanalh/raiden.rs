mod config;
mod iou;
mod keys;
mod numeric;
mod types;

pub use config::*;
pub use iou::*;
pub use keys::*;
pub use numeric::*;
pub use types::*;

use derive_more::Display;
use rand_chacha::{
    rand_core::{
        RngCore,
        SeedableRng,
    },
    ChaChaRng,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    collections::HashMap,
    str::FromStr,
};
use web3::types::{
    Address,
    U256,
};

use crate::constants::{
    DEFAULT_MEDIATION_FLAT_FEE,
    DEFAULT_MEDIATION_PROPORTIONAL_FEE,
    DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Random(ChaChaRng);

impl Random {
    pub fn new() -> Self {
        Self(ChaChaRng::seed_from_u64(0))
    }

    pub fn next(&mut self) -> u32 {
        self.0.next_u32()
    }
}

#[derive(Clone, Display, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ChainID {
    Mainnet = 1,
    Ropsten = 3,
    Rinkeby = 4,
    Goerli = 5,
    Kovan = 42,
}

impl FromStr for ChainID {
    type Err = ();

    fn from_str(s: &str) -> Result<ChainID, ()> {
        match s {
            "mainnet" => Ok(ChainID::Mainnet),
            "ropsten" => Ok(ChainID::Ropsten),
            "rinkeby" => Ok(ChainID::Rinkeby),
            "goerli" => Ok(ChainID::Goerli),
            "kovan" => Ok(ChainID::Kovan),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CanonicalIdentifier {
    pub chain_identifier: ChainID,
    pub token_network_address: Address,
    pub channel_identifier: U256,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct QueueIdentifier {
    pub recipient: Address,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TransferRole {
    Initiator,
    Mediator,
    Target,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TransferTask {
    pub role: TransferRole,
    pub token_network_address: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TransactionResult {
    Success,
    Failure,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TransactionExecutionStatus {
    pub started_block_number: Option<U64>,
    pub finished_block_number: Option<U64>,
    pub result: Option<TransactionResult>,
}

#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct MediationFeeConfig {
    pub token_to_flat_fee: HashMap<Address, FeeAmount>,
    pub token_to_proportional_fee: HashMap<Address, ProportionalFeeAmount>,
    pub token_to_proportional_imbalance_fee: HashMap<Address, ProportionalFeeAmount>,
    pub cap_meditation_fees: bool,
}

impl MediationFeeConfig {
    pub fn get_flat_fee(&self, token_address: &Address) -> FeeAmount {
        *self
            .token_to_flat_fee
            .get(token_address)
            .unwrap_or(&DEFAULT_MEDIATION_FLAT_FEE.into())
    }

    pub fn get_proportional_fee(&self, token_address: &Address) -> ProportionalFeeAmount {
        *self
            .token_to_proportional_fee
            .get(token_address)
            .unwrap_or(&DEFAULT_MEDIATION_PROPORTIONAL_FEE.into())
    }

    pub fn get_proportional_imbalance_fee(self, token_address: &Address) -> ProportionalFeeAmount {
        *self
            .token_to_proportional_imbalance_fee
            .get(token_address)
            .unwrap_or(&DEFAULT_MEDIATION_PROPORTIONAL_IMBALANCE_FEE.into())
    }
}
