use derive_more::Display;
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
    Bytes,
    H256,
    U256,
};

use crate::{
    constants::DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS,
    errors::ChannelError,
    primitives::{
        AddressMetadata,
        CanonicalIdentifier,
        ChainID,
        MediationFeeConfig,
        QueueIdentifier,
        Random,
        TransactionExecutionStatus,
        TransactionResult,
        TransferTask,
        U64,
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

#[derive(Clone, Display, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelStatus {
    #[display(fmt = "Opened")]
    Opened,
    #[display(fmt = "Closing")]
    Closing,
    #[display(fmt = "Closed")]
    Closed,
    #[display(fmt = "Settling")]
    Settling,
    #[display(fmt = "Settled")]
    Settled,
    #[display(fmt = "Removed")]
    Removed,
    #[display(fmt = "Unusable")]
    Unusable,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelState {
    pub canonical_identifier: CanonicalIdentifier,
    pub token_address: Address,
    pub token_network_registry_address: Address,
    pub reveal_timeout: U64,
    pub settle_timeout: U256,
    pub fee_schedule: FeeScheduleState,
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
        settle_timeout: U256,
        open_transaction: TransactionExecutionStatus,
        fee_config: MediationFeeConfig,
    ) -> Result<ChannelState, ChannelError> {
        if U256::from(reveal_timeout) >= settle_timeout {
            return Err(ChannelError {
                msg: format!(
                    "reveal_timeout({:?}) must be smaller than settle_timeout({:?})",
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
            fee_schedule: FeeScheduleState {
                cap_fees: fee_config.cap_meditation_fees,
                flat: fee_config.get_flat_fee(&token_address),
                proportional: fee_config.get_proportional_fee(&token_address),
                imbalance_penalty: None,
            },
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

    pub fn capacity(&self) -> U256 {
        self.our_state.contract_balance - self.our_state.total_withdraw() + self.partner_state.contract_balance
            - self.partner_state.total_withdraw()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelEndState {
    pub address: Address,
    pub contract_balance: U256,
    pub onchain_total_withdraw: U256,
    pub withdraws_pending: HashMap<U256, PendingWithdrawState>,
    pub withdraws_expired: Vec<ExpiredWithdrawState>,
    pub secrethashes_to_lockedlocks: HashMap<H256, HashTimeLockState>,
    pub secrethashes_to_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    pub secrethashes_to_onchain_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    pub balance_proof: Option<BalanceProofState>,
    pub pending_locks: PendingLocksState,
    pub onchain_locksroot: Bytes,
    pub nonce: U256,
}

impl ChannelEndState {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            contract_balance: U256::zero(),
            onchain_total_withdraw: U256::zero(),
            withdraws_pending: HashMap::new(),
            withdraws_expired: vec![],
            secrethashes_to_lockedlocks: HashMap::new(),
            secrethashes_to_unlockedlocks: HashMap::new(),
            secrethashes_to_onchain_unlockedlocks: HashMap::new(),
            balance_proof: None,
            pending_locks: PendingLocksState::default(),
            onchain_locksroot: Bytes(vec![]),
            nonce: U256::zero(),
        }
    }

    pub fn offchain_total_withdraw(&self) -> U256 {
        self.withdraws_pending
            .values()
            .map(|w| w.total_withdraw)
            .fold(U256::zero(), |a, b| max(a, b))
    }

    pub fn total_withdraw(&self) -> U256 {
        max(self.offchain_total_withdraw(), self.onchain_total_withdraw)
    }

    pub fn next_nonce(&self) -> U256 {
        self.nonce + 1
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BalanceProofState {
    pub nonce: U256,
    pub transferred_amount: U256,
    pub locked_amount: U256,
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
    amount: U256,
    expiration: U64,
    secrethash: H256,
    encoded: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HashTimeLockState {
    amount: U256,
    expiration: U64,
    secrethash: H256,
    encoded: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExpiredWithdrawState {
    pub total_withdraw: U256,
    pub expiration: U64,
    pub nonce: U256,
    pub recipient_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PendingWithdrawState {
    pub total_withdraw: U256,
    pub expiration: U64,
    pub nonce: U256,
    pub recipient_metadata: Option<AddressMetadata>,
}

impl PendingWithdrawState {
    pub fn expiration_threshold(&self) -> U64 {
        self.expiration
            .saturating_add(DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS.saturating_mul(2).into())
            .into()
    }

    pub fn has_expired(&self, current_block: U64) -> bool {
        let threshold = self.expiration_threshold();
        current_block >= threshold
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FeeScheduleState {
    pub cap_fees: bool,
    pub flat: U256,
    pub proportional: U256,
    pub imbalance_penalty: Option<Vec<(U256, U256)>>,
    //penalty_func: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionChannelDeposit {
    pub participant_address: Address,
    pub contract_balance: U256,
    pub deposit_block_number: U64,
}
