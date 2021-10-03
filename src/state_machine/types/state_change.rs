use crate::{
    primitives::{
        AddressMetadata,
        CanonicalIdentifier,
        ChainID,
        MediationFeeConfig,
        U64,
    },
    state_machine::types::{
        ChannelState,
        TokenNetworkRegistryState,
        TokenNetworkState,
    },
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

use super::{
    BalanceProofState,
    HopState,
    LockedTransferSignedState,
    RouteState,
    TransactionChannelDeposit,
    TransferDescriptionWithSecretState,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum StateChange {
    Block(Block),
    ActionInitChain(ActionInitChain),
    ActionChannelSetRevealTimeout(ActionChannelSetRevealTimeout),
    ActionChannelWithdraw(ActionChannelWithdraw),
    ContractReceiveTokenNetworkRegistry(ContractReceiveTokenNetworkRegistry),
    ContractReceiveTokenNetworkCreated(ContractReceiveTokenNetworkCreated),
    ContractReceiveChannelOpened(ContractReceiveChannelOpened),
    ContractReceiveChannelClosed(ContractReceiveChannelClosed),
    ContractReceiveChannelSettled(ContractReceiveChannelSettled),
    ContractReceiveChannelDeposit(ContractReceiveChannelDeposit),
    ContractReceiveChannelWithdraw(ContractReceiveChannelWithdraw),
    ContractReceiveChannelBatchUnlock(ContractReceiveChannelBatchUnlock),
    ContractReceiveSecretReveal(ContractReceiveSecretReveal),
    ContractReceiveRouteNew(ContractReceiveRouteNew),
    ContractReceiveUpdateTransfer(ContractReceiveUpdateTransfer),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    pub block_number: U64,
    pub block_hash: H256,
    pub gas_limit: U256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitChain {
    pub chain_id: ChainID,
    pub block_number: U64,
    pub block_hash: H256,
    pub our_address: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionChannelSetRevealTimeout {
    pub canonical_identifier: CanonicalIdentifier,
    pub reveal_timeout: U64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionChannelWithdraw {
    pub canonical_identifier: CanonicalIdentifier,
    pub total_withdraw: U256,
    pub recipient_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkRegistry {
    pub transaction_hash: Option<H256>,
    pub token_network_registry: TokenNetworkRegistryState,
    pub block_number: U64,
    pub block_hash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkCreated {
    pub transaction_hash: Option<H256>,
    pub token_network_registry_address: Address,
    pub token_network: TokenNetworkState,
    pub block_number: U64,
    pub block_hash: H256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelOpened {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub channel_state: ChannelState,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelClosed {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub transaction_from: Address,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelSettled {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub canonical_identifier: CanonicalIdentifier,
    pub our_onchain_locksroot: Bytes,
    pub partner_onchain_locksroot: Bytes,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelDeposit {
    pub canonical_identifier: CanonicalIdentifier,
    pub deposit_transaction: TransactionChannelDeposit,
    pub fee_config: MediationFeeConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelWithdraw {
    pub canonical_identifier: CanonicalIdentifier,
    pub participant: Address,
    pub total_withdraw: U256,
    pub fee_config: MediationFeeConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelBatchUnlock {
    pub canonical_identifier: CanonicalIdentifier,
    pub receiver: Address,
    pub sender: Address,
    pub locksroot: H256,
    pub unlocked_amount: u32,
    pub returned_tokens: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveSecretReveal {
    pub secret_registry_address: Address,
    pub secrethash: H256,
    pub secret: Bytes,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveRouteNew {
    pub canonical_identifier: CanonicalIdentifier,
    pub participant1: Address,
    pub participant2: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveUpdateTransfer {
    pub canonical_identifier: CanonicalIdentifier,
    pub nonce: U256,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitInitiator {
    transfer: TransferDescriptionWithSecretState,
    routes: Vec<RouteState>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitMediator {
    sender: Address,
    balance_proof: BalanceProofState,
    from_hop: HopState,
    candidate_route_states: Vec<RouteState>,
    from_transfer: LockedTransferSignedState,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitTarget {
    sender: Address,
    balance_proof: BalanceProofState,
    from_hop: HopState,
    transfer: LockedTransferSignedState,
}
