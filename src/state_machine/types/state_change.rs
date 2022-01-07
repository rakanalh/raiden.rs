use crate::{
    primitives::{
        AddressMetadata,
        BlockHash,
        BlockNumber,
        CanonicalIdentifier,
        ChainID,
        GasLimit,
        Locksroot,
        MediationFeeConfig,
        Nonce,
        RawSecret,
        RevealTimeout,
        SecretHash,
        TokenAmount,
        TransactionHash,
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
use web3::types::Address;

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
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub gas_limit: GasLimit,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitChain {
    pub chain_id: ChainID,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub our_address: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionChannelSetRevealTimeout {
    pub canonical_identifier: CanonicalIdentifier,
    pub reveal_timeout: RevealTimeout,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionChannelWithdraw {
    pub canonical_identifier: CanonicalIdentifier,
    pub total_withdraw: TokenAmount,
    pub recipient_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkRegistry {
    pub transaction_hash: Option<TransactionHash>,
    pub token_network_registry: TokenNetworkRegistryState,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkCreated {
    pub transaction_hash: Option<TransactionHash>,
    pub token_network_registry_address: Address,
    pub token_network: TokenNetworkState,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelOpened {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub channel_state: ChannelState,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelClosed {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub transaction_from: Address,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelSettled {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub canonical_identifier: CanonicalIdentifier,
    pub our_onchain_locksroot: Locksroot,
    pub partner_onchain_locksroot: Locksroot,
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
    pub total_withdraw: TokenAmount,
    pub fee_config: MediationFeeConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelBatchUnlock {
    pub canonical_identifier: CanonicalIdentifier,
    pub receiver: Address,
    pub sender: Address,
    pub locksroot: Locksroot,
    pub unlocked_amount: TokenAmount,
    pub returned_tokens: TokenAmount,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveSecretReveal {
    pub secret_registry_address: Address,
    pub secrethash: SecretHash,
    pub secret: RawSecret,
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
    pub nonce: Nonce,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitInitiator {
    pub transfer: TransferDescriptionWithSecretState,
    pub routes: Vec<RouteState>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitMediator {
    pub sender: Address,
    pub balance_proof: BalanceProofState,
    pub from_hop: HopState,
    pub candidate_route_states: Vec<RouteState>,
    pub from_transfer: LockedTransferSignedState,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitTarget {
    pub sender: Address,
    pub balance_proof: BalanceProofState,
    pub from_hop: HopState,
    pub transfer: LockedTransferSignedState,
}
