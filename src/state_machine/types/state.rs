use std::{
    cmp::max,
    collections::HashMap,
};

use serde::{
    Deserialize,
    Serialize,
};
use web3::types::{
    Address,
    H256,
    U256,
    U64,
};

use crate::{
    constants::DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS,
    errors::ChannelError,
    primitives::{
        AddressMetadata,
        CanonicalIdentifier,
        ChainID,
        QueueIdentifier,
        Random,
        TransactionExecutionStatus,
        TransactionResult,
        TransferTask,
    },
};

use super::SendMessageEvent;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaymentMappingState {
    pub secrethashes_to_task: HashMap<H256, TransferTask>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChainState {
    pub chain_id: ChainID,
    pub block_number: U64,
    pub block_hash: H256,
    pub our_address: Address,
    pub identifiers_to_tokennetworkregistries: HashMap<Address, TokenNetworkRegistryState>,
    pub queueids_to_queues: HashMap<QueueIdentifier, Vec<SendMessageEvent>>,
    pub payment_mapping: PaymentMappingState,
    pub pseudo_random_number_generator: Random,
}

impl ChainState {
    pub fn new(chain_id: ChainID, block_number: U64, block_hash: H256, our_address: Address) -> ChainState {
        ChainState {
            chain_id,
            block_number,
            block_hash,
            our_address,
            queueids_to_queues: HashMap::new(),
            identifiers_to_tokennetworkregistries: HashMap::new(),
            payment_mapping: PaymentMappingState {
                secrethashes_to_task: HashMap::new(),
            },
            pseudo_random_number_generator: Random::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenNetworkRegistryState {
    pub address: Address,
    pub tokennetworkaddresses_to_tokennetworks: HashMap<Address, TokenNetworkState>,
    pub tokenaddresses_to_tokennetworkaddresses: HashMap<Address, Address>,
}

impl TokenNetworkRegistryState {
    pub fn new(address: Address, token_network_list: Vec<TokenNetworkState>) -> TokenNetworkRegistryState {
        let mut registry_state = TokenNetworkRegistryState {
            address: Address::zero(),
            tokennetworkaddresses_to_tokennetworks: HashMap::new(),
            tokenaddresses_to_tokennetworkaddresses: HashMap::new(),
        };
        for token_network in token_network_list.iter() {
            let token_network_address = token_network.address;
            let token_address = token_network.token_address;
            registry_state
                .tokennetworkaddresses_to_tokennetworks
                .insert(token_network_address, token_network.clone());

            registry_state
                .tokenaddresses_to_tokennetworkaddresses
                .insert(token_address, token_network.address);
        }
        registry_state.address = address;
        registry_state
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenNetworkState {
    pub address: Address,
    pub token_address: Address,
    pub network_graph: TokenNetworkGraphState,
    pub channelidentifiers_to_channels: HashMap<U256, ChannelState>,
    pub partneraddresses_to_channelidentifiers: HashMap<Address, Vec<U256>>,
}

impl TokenNetworkState {
    pub fn new(address: Address, token_address: Address) -> TokenNetworkState {
        TokenNetworkState {
            address,
            token_address,
            network_graph: TokenNetworkGraphState::default(),
            channelidentifiers_to_channels: HashMap::new(),
            partneraddresses_to_channelidentifiers: HashMap::new(),
        }
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct TokenNetworkGraphState {}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelStatus {
    Opened,
    Closing,
    Closed,
    Settling,
    Settled,
    Unusable,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelState {
    pub canonical_identifier: CanonicalIdentifier,
    pub token_address: Address,
    pub token_network_registry_address: Address,
    pub reveal_timeout: U64,
    pub settle_timeout: U64,
    pub our_state: ChannelEndState,
    pub partner_state: ChannelEndState,
    pub open_transaction: TransactionExecutionStatus,
    pub close_transaction: Option<TransactionExecutionStatus>,
    pub settle_transaction: Option<TransactionExecutionStatus>,
    pub update_transaction: Option<TransactionExecutionStatus>,
}

impl ChannelState {
    pub fn new(
        canonical_identifier: CanonicalIdentifier,
        token_address: Address,
        token_network_registry_address: Address,
        our_address: Address,
        partner_address: Address,
        reveal_timeout: U64,
        settle_timeout: U64,
        open_transaction: TransactionExecutionStatus,
    ) -> Result<ChannelState, ChannelError> {
        if reveal_timeout >= settle_timeout {
            return Err(ChannelError {
                msg: format!(
                    "reveal_timeout({}) must be smaller than settle_timeout({})",
                    reveal_timeout, settle_timeout,
                ),
            });
        }

        let our_state = ChannelEndState::new(our_address);

        let partner_state = ChannelEndState::new(partner_address);

        Ok(ChannelState {
            canonical_identifier,
            token_address,
            token_network_registry_address,
            reveal_timeout,
            settle_timeout,
            our_state,
            partner_state,
            open_transaction,
            close_transaction: None,
            settle_transaction: None,
            update_transaction: None,
        })
    }

    pub fn status(&self) -> ChannelStatus {
        let mut status = ChannelStatus::Opened;

        if let Some(settle_transaction) = &self.settle_transaction {
            let finished_successfully = settle_transaction.result == Some(TransactionResult::Success);
            let running = settle_transaction.finished_block_number.is_none();

            if finished_successfully {
                status = ChannelStatus::Settled;
            } else if running {
                status = ChannelStatus::Settling;
            } else {
                status = ChannelStatus::Unusable;
            }
        } else if let Some(close_transaction) = &self.close_transaction {
            let finished_successfully = close_transaction.result == Some(TransactionResult::Success);
            let running = close_transaction.finished_block_number.is_none();

            if finished_successfully {
                status = ChannelStatus::Closed;
            } else if running {
                status = ChannelStatus::Closing;
            } else {
                status = ChannelStatus::Unusable;
            }
        }

        status
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelEndState {
    pub address: Address,
    pub contract_balance: u64,
    pub onchain_total_withdraw: u64,
    pub withdraws_pending: HashMap<u64, PendingWithdrawState>,
    pub withdraws_expired: Vec<ExpiredWithdrawState>,
    pub secrethashes_to_lockedlocks: HashMap<H256, HashTimeLockState>,
    pub secrethashes_to_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    pub secrethashes_to_onchain_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    pub balance_proof: Option<BalanceProofState>,
    pub pending_locks: PendingLocksState,
    pub onchain_locksroot: H256,
    pub nonce: u64,
}

impl ChannelEndState {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            contract_balance: 0,
            onchain_total_withdraw: 0,
            withdraws_pending: HashMap::new(),
            withdraws_expired: vec![],
            secrethashes_to_lockedlocks: HashMap::new(),
            secrethashes_to_unlockedlocks: HashMap::new(),
            secrethashes_to_onchain_unlockedlocks: HashMap::new(),
            balance_proof: None,
            pending_locks: PendingLocksState::default(),
            onchain_locksroot: H256::zero(),
            nonce: 0,
        }
    }

    pub fn offchain_total_withdraw(&self) -> u64 {
        self.withdraws_pending
            .values()
            .map(|w| w.total_withdraw)
            .fold(0, |a, b| max(a, b))
    }

    pub fn total_withdraw(&self) -> u64 {
        max(self.offchain_total_withdraw(), self.onchain_total_withdraw)
    }

    pub fn next_nonce(&self) -> u64 {
        self.nonce + 1
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BalanceProofState {
    pub nonce: u64,
    pub transferred_amount: u64,
    pub locked_amount: u64,
    pub locksroot: H256,
    pub canonical_identifier: CanonicalIdentifier,
    pub balance_hash: H256,
    pub message_hash: Option<H256>,
    pub signature: Option<H256>,
    pub sender: Option<Address>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct PendingLocksState {
    locks: Vec<H256>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UnlockPartialProofState {
    lock: HashTimeLockState,
    secret: H256,
    amount: u64,
    expiration: u16,
    secrethash: H256,
    encoded: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HashTimeLockState {
    amount: u64,
    expiration: u16,
    secrethash: H256,
    encoded: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExpiredWithdrawState {
    pub total_withdraw: u64,
    pub expiration: U64,
    pub nonce: u64,
    pub recipient_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PendingWithdrawState {
    pub total_withdraw: u64,
    pub expiration: U64,
    pub nonce: u64,
    pub recipient_metadata: Option<AddressMetadata>,
}

impl PendingWithdrawState {
    pub fn expiration_threshold(&self) -> U64 {
        self.expiration + DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS * 2
    }

    pub fn has_expired(&self, current_block: U64) -> bool {
        let threshold = self.expiration_threshold();
        current_block >= threshold
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FeeScheduleState {
    flat: u64,
    proportional: u64,
    imbalance_penalty: Option<Vec<(u64, u64)>>,
    penalty_func: Option<u64>,
}
