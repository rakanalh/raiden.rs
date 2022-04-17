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
    U256,
};

use crate::{
    constants::{
        DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS,
        MAXIMUM_PENDING_TRANSFERS,
    },
    errors::ChannelError,
    primitives::{
        AddressMetadata,
        AmountToBytes,
        BalanceHash,
        BalanceProofData,
        BlockExpiration,
        BlockHash,
        BlockNumber,
        BlockTimeout,
        CanonicalIdentifier,
        ChainID,
        ChannelIdentifier,
        EncodedLock,
        LockTimeout,
        LockedAmount,
        Locksroot,
        MediationFeeConfig,
        MessageHash,
        MessageIdentifier,
        Nonce,
        PaymentIdentifier,
        QueueIdentifier,
        Random,
        RevealTimeout,
        Secret,
        SecretHash,
        SettleTimeout,
        Signature,
        TokenAddress,
        TokenAmount,
        TokenNetworkAddress,
        TokenNetworkRegistryAddress,
        TransactionExecutionStatus,
        TransactionResult,
    },
    state_machine::views,
};

use super::{
    ContractSendEvent,
    SendMessageEvent,
};

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TransferRole {
    Initiator,
    Mediator,
    Target,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TransferTask {
    Initiator(InitiatorTask),
    Mediator(MediatorTask),
    Target(TargetTask),
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TransferState {
    Pending,
    Expired,
    SecretRevealed,
    Canceled,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum PayeeState {
    Pending,
    SecretRevealed,
    ContractUnlock,
    BalanceProof,
    Expired,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum PayerState {
    Pending,
    SecretRevealed,
    WaitingUnlock,
    WaitingSecretReveal,
    BalanceProof,
    Expired,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum TargetState {
    Expired,
    OffchainSecretReveal,
    OnchainSecretReveal,
    OnchainUnlock,
    SecretRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum WaitingTransferStatus {
    Waiting,
    Expired,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct InitiatorTransferState {
    pub route: RouteState,
    pub transfer_description: TransferDescriptionWithSecretState,
    pub channel_identifier: ChannelIdentifier,
    pub transfer: LockedTransferState,
    pub received_secret_request: bool,
    pub transfer_state: TransferState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct InitiatorPaymentState {
    pub routes: Vec<RouteState>,
    pub initiator_transfers: HashMap<SecretHash, InitiatorTransferState>,
    pub cancelled_channels: Vec<ChannelIdentifier>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct InitiatorTask {
    pub role: TransferRole,
    pub token_network_address: TokenNetworkAddress,
    pub manager_state: InitiatorPaymentState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct WaitingTransferState {
    pub transfer: LockedTransferState,
    pub status: WaitingTransferStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MediationPairState {
    pub payer_transfer: LockedTransferState,
    pub payee_address: Address,
    pub payee_transfer: LockedTransferState,
    pub payer_state: PayerState,
    pub payee_state: PayeeState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MediatorTransferState {
    pub secrethash: SecretHash,
    pub routes: Vec<RouteState>,
    pub refunded_channels: Vec<ChannelIdentifier>,
    pub secret: Option<Secret>,
    pub transfers_pair: Vec<MediationPairState>,
    pub waiting_transfer: Option<WaitingTransferState>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MediatorTask {
    pub role: TransferRole,
    pub token_network_address: TokenNetworkAddress,
    pub mediator_state: MediatorTransferState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TargetTask {
    pub role: TransferRole,
    pub token_network_address: TokenNetworkAddress,
    pub target_state: TargetTransferState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TargetTransferState {
    pub from_hop: HopState,
    pub transfer: LockedTransferState,
    pub secret: Option<Secret>,
    pub state: TargetState,
    pub initiator_address_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct PaymentMappingState {
    pub secrethashes_to_task: HashMap<SecretHash, TransferTask>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ChainState {
    pub chain_id: ChainID,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub our_address: Address,
    pub identifiers_to_tokennetworkregistries: HashMap<Address, TokenNetworkRegistryState>,
    pub queueids_to_queues: HashMap<QueueIdentifier, Vec<SendMessageEvent>>,
    pub payment_mapping: PaymentMappingState,
    pub pending_transactions: Vec<ContractSendEvent>,
    pub pseudo_random_number_generator: Random,
}

impl ChainState {
    pub fn new(
        chain_id: ChainID,
        block_number: BlockNumber,
        block_hash: BlockHash,
        our_address: Address,
    ) -> ChainState {
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
            pending_transactions: vec![],
            pseudo_random_number_generator: Random::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TokenNetworkState {
    pub address: Address,
    pub token_address: TokenAddress,
    pub network_graph: TokenNetworkGraphState,
    pub channelidentifiers_to_channels: HashMap<U256, ChannelState>,
    pub partneraddresses_to_channelidentifiers: HashMap<Address, Vec<ChannelIdentifier>>,
}

impl TokenNetworkState {
    pub fn new(address: Address, token_address: TokenAddress) -> TokenNetworkState {
        TokenNetworkState {
            address,
            token_address,
            network_graph: TokenNetworkGraphState::default(),
            channelidentifiers_to_channels: HashMap::new(),
            partneraddresses_to_channelidentifiers: HashMap::new(),
        }
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ChannelState {
    pub canonical_identifier: CanonicalIdentifier,
    pub token_address: TokenAddress,
    pub token_network_registry_address: TokenNetworkRegistryAddress,
    pub reveal_timeout: RevealTimeout,
    pub settle_timeout: SettleTimeout,
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
        token_address: TokenAddress,
        token_network_registry_address: TokenNetworkRegistryAddress,
        our_address: Address,
        partner_address: Address,
        reveal_timeout: RevealTimeout,
        settle_timeout: SettleTimeout,
        open_transaction: TransactionExecutionStatus,
        fee_config: MediationFeeConfig,
    ) -> Result<ChannelState, ChannelError> {
        if SettleTimeout::from(reveal_timeout) >= settle_timeout {
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

    pub fn our_total_deposit(&self) -> TokenAmount {
        self.our_state.contract_balance
    }

    pub fn partner_total_deposit(&self) -> TokenAmount {
        self.partner_state.contract_balance
    }

    pub fn our_total_withdraw(&self) -> TokenAmount {
        self.our_state.total_withdraw()
    }

    pub fn partner_total_withdraw(&self) -> TokenAmount {
        self.partner_state.total_withdraw()
    }

    pub fn capacity(&self) -> TokenAmount {
        self.our_state.contract_balance - self.our_state.total_withdraw() + self.partner_state.contract_balance
            - self.partner_state.total_withdraw()
    }

    pub fn is_usable_for_new_transfer(&self, amount: TokenAmount, lock_timeout: Option<LockTimeout>) -> bool {
        let pending_transfers = self.our_state.count_pending_transfers();
        let distributable = views::channel_distributable(&self.our_state, &self.partner_state);
        let lock_timeout_valid = match lock_timeout {
            Some(lock_timeout) => lock_timeout <= self.settle_timeout && lock_timeout > self.reveal_timeout,
            None => true,
        };
        let is_valid_settle_timeout = self.settle_timeout >= self.reveal_timeout * 2;

        if self.status() != ChannelStatus::Opened {
            return false;
        }

        if !is_valid_settle_timeout {
            return false;
        }

        if pending_transfers >= MAXIMUM_PENDING_TRANSFERS {
            return false;
        }

        if amount > distributable {
            return false;
        }

        if !self.our_state.is_valid_amount(amount) {
            return false;
        }

        if !lock_timeout_valid {
            return false;
        }

        true
    }

    pub fn is_usable_for_mediation(&self, transfer_amount: TokenAmount, lock_timeout: BlockTimeout) -> bool {
        self.is_usable_for_new_transfer(transfer_amount, Some(lock_timeout))
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ChannelEndState {
    pub address: Address,
    pub contract_balance: TokenAmount,
    pub onchain_total_withdraw: TokenAmount,
    pub withdraws_pending: HashMap<U256, PendingWithdrawState>,
    pub withdraws_expired: Vec<ExpiredWithdrawState>,
    pub initiated_coop_settle: Option<CoopSettleState>,
    pub expired_coop_settles: Vec<CoopSettleState>,
    pub secrethashes_to_lockedlocks: HashMap<SecretHash, HashTimeLockState>,
    pub secrethashes_to_unlockedlocks: HashMap<SecretHash, UnlockPartialProofState>,
    pub secrethashes_to_onchain_unlockedlocks: HashMap<SecretHash, UnlockPartialProofState>,
    pub balance_proof: Option<BalanceProofState>,
    pub pending_locks: PendingLocksState,
    pub onchain_locksroot: Bytes,
    pub nonce: Nonce,
}

impl ChannelEndState {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            contract_balance: TokenAmount::zero(),
            onchain_total_withdraw: TokenAmount::zero(),
            withdraws_pending: HashMap::new(),
            withdraws_expired: vec![],
            secrethashes_to_lockedlocks: HashMap::new(),
            secrethashes_to_unlockedlocks: HashMap::new(),
            secrethashes_to_onchain_unlockedlocks: HashMap::new(),
            balance_proof: None,
            pending_locks: PendingLocksState::default(),
            onchain_locksroot: Bytes(vec![]),
            nonce: Nonce::zero(),
            initiated_coop_settle: None,
            expired_coop_settles: vec![],
        }
    }

    pub fn offchain_total_withdraw(&self) -> TokenAmount {
        self.withdraws_pending
            .values()
            .map(|w| w.total_withdraw)
            .fold(TokenAmount::zero(), |a, b| max(a, b))
    }

    pub fn total_withdraw(&self) -> TokenAmount {
        max(self.offchain_total_withdraw(), self.onchain_total_withdraw)
    }

    pub fn next_nonce(&self) -> Nonce {
        self.nonce + 1
    }

    pub fn count_pending_transfers(&self) -> usize {
        self.pending_locks.locks.len()
    }

    pub fn locked_amount(&self) -> TokenAmount {
        let total_pending: TokenAmount = self
            .secrethashes_to_lockedlocks
            .values()
            .map(|lock| lock.amount)
            .fold(TokenAmount::zero(), |acc, x| acc + x);
        let total_unclaimed: TokenAmount = self
            .secrethashes_to_unlockedlocks
            .values()
            .map(|unlock| unlock.lock.amount)
            .fold(TokenAmount::zero(), |acc, x| acc + x);
        let total_unclaimed_onchain: TokenAmount = self
            .secrethashes_to_onchain_unlockedlocks
            .values()
            .map(|unlock| unlock.lock.amount)
            .fold(TokenAmount::zero(), |acc, x| acc + x);
        total_pending + total_unclaimed + total_unclaimed_onchain
    }

    pub fn get_current_balanceproof(&self) -> BalanceProofData {
        match &self.balance_proof {
            Some(bp) => (bp.locksroot.clone(), bp.nonce, bp.transferred_amount, bp.locked_amount),
            None => (
                Locksroot::default(),
                Nonce::default(),
                TokenAmount::zero(),
                TokenAmount::zero(),
            ),
        }
    }

    pub fn is_valid_amount(&self, amount: TokenAmount) -> bool {
        let (_, _, transferred_amount, locked_amount) = self.get_current_balanceproof();
        let transferred_amount_after_unlock = transferred_amount
            .checked_add(locked_amount)
            .map(|r| r.saturating_add(amount));
        transferred_amount_after_unlock.is_some()
    }

    pub fn is_secret_known(&self, secrethash: SecretHash) -> bool {
        self.is_secret_known_offchain(secrethash) || self.secret_known_onchain(secrethash)
    }

    pub fn secret_known_onchain(&self, secrethash: SecretHash) -> bool {
        self.secrethashes_to_onchain_unlockedlocks.contains_key(&secrethash)
    }

    pub fn is_secret_known_offchain(&self, secrethash: SecretHash) -> bool {
        self.secrethashes_to_unlockedlocks.contains_key(&secrethash)
            || self.secrethashes_to_onchain_unlockedlocks.contains_key(&secrethash)
    }

    pub fn get_secret(&self, secrethash: SecretHash) -> Option<Secret> {
        let mut partial_unlock_proof = self.secrethashes_to_unlockedlocks.get(&secrethash);
        if partial_unlock_proof.is_none() {
            partial_unlock_proof = self.secrethashes_to_onchain_unlockedlocks.get(&secrethash);
        }

        if let Some(partial_unlock_proof) = partial_unlock_proof {
            return Some(partial_unlock_proof.secret.clone());
        }

        None
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BalanceProofState {
    pub nonce: Nonce,
    pub transferred_amount: TokenAmount,
    pub locked_amount: LockedAmount,
    pub locksroot: Locksroot,
    pub canonical_identifier: CanonicalIdentifier,
    pub balance_hash: BalanceHash,
    pub message_hash: Option<MessageHash>,
    pub signature: Option<Signature>,
    pub sender: Option<Address>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct PendingLocksState {
    pub locks: Vec<EncodedLock>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct UnlockPartialProofState {
    pub lock: HashTimeLockState,
    pub secret: Secret,
    pub amount: TokenAmount,
    pub expiration: BlockExpiration,
    pub secrethash: SecretHash,
    pub encoded: EncodedLock,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct HashTimeLockState {
    pub amount: TokenAmount,
    pub expiration: BlockExpiration,
    pub secrethash: SecretHash,
    pub encoded: EncodedLock,
}

impl HashTimeLockState {
    pub fn create(amount: TokenAmount, expiration: BlockExpiration, secrethash: SecretHash) -> Self {
        let mut data = vec![];
        data.extend_from_slice(amount.to_bytes());
        data.extend_from_slice(expiration.as_bytes());
        data.extend_from_slice(secrethash.as_bytes());
        Self {
            amount,
            expiration,
            secrethash,
            encoded: Bytes(data),
        }
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ExpiredWithdrawState {
    pub total_withdraw: TokenAmount,
    pub expiration: BlockExpiration,
    pub nonce: Nonce,
    pub recipient_metadata: Option<AddressMetadata>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct PendingWithdrawState {
    pub total_withdraw: TokenAmount,
    pub expiration: BlockExpiration,
    pub nonce: Nonce,
    pub recipient_metadata: Option<AddressMetadata>,
}

impl PendingWithdrawState {
    pub fn expiration_threshold(&self) -> BlockExpiration {
        self.expiration
            .saturating_add(DEFAULT_NUMBER_OF_BLOCK_CONFIRMATIONS.saturating_mul(2).into())
            .into()
    }

    pub fn has_expired(&self, current_block: BlockNumber) -> bool {
        let threshold = self.expiration_threshold();
        current_block >= threshold
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct CoopSettleState {
    pub total_withdraw_initiator: TokenAmount,
    pub total_withdraw_partner: TokenAmount,
    pub expiration: BlockExpiration,
    pub partner_signature_request: Option<Signature>,
    pub partner_signature_confirmation: Option<Signature>,
    pub transaction: Option<TransactionExecutionStatus>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct FeeScheduleState {
    pub cap_fees: bool,
    pub flat: U256,
    pub proportional: U256,
    pub imbalance_penalty: Option<Vec<(U256, U256)>>,
    //penalty_func: Option<u64>,
}

impl Default for FeeScheduleState {
    fn default() -> Self {
        Self {
            cap_fees: true,
            flat: U256::zero(),
            proportional: U256::zero(),
            imbalance_penalty: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionChannelDeposit {
    pub participant_address: Address,
    pub contract_balance: TokenAmount,
    pub deposit_block_number: BlockNumber,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct HopState {
    pub node_address: Address,
    pub channel_identifier: ChannelIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct RouteState {
    pub route: Vec<Address>,
    pub address_to_metadata: HashMap<Address, AddressMetadata>,
    pub swaps: HashMap<Address, Address>,
    pub estimated_fee: TokenAmount,
}

impl RouteState {
    pub fn hop_after(&self, address: Address) -> Option<Address> {
        if let Some(index) = self.route.iter().position(|route| route == &address) {
            if index + 1 < self.route.len() {
                return Some(self.route[index + 1]);
            }
        }

        None
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct TransferDescriptionWithSecretState {
    pub token_network_registry_address: TokenNetworkRegistryAddress,
    pub payment_identifier: PaymentIdentifier,
    pub amount: TokenAmount,
    pub token_network_address: TokenNetworkAddress,
    pub initiator: Address,
    pub target: Address,
    pub secret: Secret,
    pub secrethash: SecretHash,
    pub lock_timeout: Option<BlockTimeout>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct LockedTransferState {
    pub payment_identifier: PaymentIdentifier,
    pub token: Address,
    pub lock: HashTimeLockState,
    pub initiator: Address,
    pub target: Address,
    pub message_identifier: MessageIdentifier,
    pub route_states: Vec<RouteState>,
    pub balance_proof: BalanceProofState,
}
