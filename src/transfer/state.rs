use crate::enums::ChainID;
use crate::errors::ChannelError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use web3::types::{Address, H256, U256, U64};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CanonicalIdentifier {
    pub chain_identifier: u64,
    pub token_network_address: Address,
    pub channel_identifier: U256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChainState {
    pub chain_id: ChainID,
    pub block_number: U64,
    pub our_address: Address,
    pub identifiers_to_tokennetworkregistries: HashMap<Address, TokenNetworkRegistryState>,
}

impl ChainState {
    pub fn new(chain_id: ChainID, block_number: U64, our_address: Address) -> ChainState {
        ChainState {
            chain_id,
            block_number,
            our_address,
            identifiers_to_tokennetworkregistries: HashMap::new(),
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
    pub channelidentifiers_to_channels: HashMap<u64, ChannelState>,
    pub partneraddresses_to_channelidentifiers: HashMap<Address, Vec<u64>>,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenNetworkGraphState {}

impl TokenNetworkGraphState {
    pub fn default() -> TokenNetworkGraphState {
        TokenNetworkGraphState {}
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelState {
    pub canonical_identifier: CanonicalIdentifier,
    pub token_address: Address,
    pub token_network_registry_address: Address,
    pub reveal_timeout: U256,
    pub settle_timeout: U256,
    pub our_state: OurEndState,
    pub partner_state: PartnerEndState,
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
        reveal_timeout: U256,
        settle_timeout: U256,
        open_transaction: TransactionExecutionStatus,
    ) -> Result<ChannelState, ChannelError> {
        if reveal_timeout >= settle_timeout {
            return Err(ChannelError {
                msg: "reveal_timeout must be smaller than settle_timeout".to_string(),
            });
        }

        let our_state = OurEndState::new(our_address);
        let partner_state = PartnerEndState::new(partner_address);

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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OurEndState {
    address: Address,
    contract_balance: u64,
    onchain_total_withdraw: u64,
    withdraws_pending: HashMap<u64, PendingWithdrawState>,
    withdraws_expired: Vec<ExpiredWithdrawState>,
    secrethashes_to_lockedlocks: HashMap<H256, HashTimeLockState>,
    secrethashes_to_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    secrethashes_to_onchain_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    balance_proof: Option<BalanceProofUnsignedState>,
    pending_locks: PendingLocksState,
    onchain_locksroot: H256,
    nonce: u64,
}

impl OurEndState {
    pub fn new(address: Address) -> OurEndState {
        OurEndState {
            address,
            contract_balance: 0,
            onchain_total_withdraw: 0,
            withdraws_pending: HashMap::new(),
            withdraws_expired: vec![],
            secrethashes_to_lockedlocks: HashMap::new(),
            secrethashes_to_unlockedlocks: HashMap::new(),
            secrethashes_to_onchain_unlockedlocks: HashMap::new(),
            balance_proof: None,
            pending_locks: PendingLocksState::new(),
            onchain_locksroot: H256::zero(),
            nonce: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PartnerEndState {
    address: Address,
    contract_balance: u64,
    onchain_total_withdraw: u64,
    withdraws_pending: HashMap<u16, PendingWithdrawState>,
    withdraws_expired: Vec<ExpiredWithdrawState>,
    secrethashes_to_lockedlocks: HashMap<H256, HashTimeLockState>,
    secrethashes_to_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    secrethashes_to_onchain_unlockedlocks: HashMap<H256, UnlockPartialProofState>,
    balance_proof: Option<BalanceProofSignedState>,
    pending_locks: PendingLocksState,
    onchain_locksroot: H256,
    nonce: u64,
}

impl PartnerEndState {
    pub fn new(address: Address) -> PartnerEndState {
        PartnerEndState {
            address,
            contract_balance: 0,
            onchain_total_withdraw: 0,
            withdraws_pending: HashMap::new(),
            withdraws_expired: vec![],
            secrethashes_to_lockedlocks: HashMap::new(),
            secrethashes_to_unlockedlocks: HashMap::new(),
            secrethashes_to_onchain_unlockedlocks: HashMap::new(),
            balance_proof: None,
            pending_locks: PendingLocksState::new(),
            onchain_locksroot: H256::zero(),
            nonce: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BalanceProofUnsignedState {
    nonce: u64,
    transferred_amount: u64,
    locked_amount: u64,
    locksroot: H256,
    canonical_identifier: CanonicalIdentifier,
    balance_hash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BalanceProofSignedState {
    nonce: u64,
    transferred_amount: u64,
    locked_amount: u64,
    locksroot: H256,
    message_hash: H256,
    signature: H256,
    sender: Address,
    canonical_identifier: CanonicalIdentifier,
    balance_hash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PendingLocksState {
    locks: Vec<H256>,
}

impl PendingLocksState {
    fn new() -> Self {
        PendingLocksState { locks: vec![] }
    }
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

impl HashTimeLockState {
    pub fn new(amount: u64, expiration: u16, secrethash: H256, encoded: H256) -> HashTimeLockState {
        HashTimeLockState {
            amount,
            expiration,
            secrethash,
            encoded,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExpiredWithdrawState {
    total_withdraw: u64,
    expiration: u16,
    nonce: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PendingWithdrawState {
    total_withdraw: u64,
    expiration: u16,
    nonce: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FeeScheduleState {
    flat: u64,
    proportional: u64,
    imbalance_penalty: Option<Vec<(u64, u64)>>,
    penalty_func: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TransactionResult {
    SUCCESS,
    FAILURE,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionExecutionStatus {
    pub started_block_number: Option<U64>,
    pub finished_block_number: Option<U64>,
    pub result: Option<TransactionResult>,
}
