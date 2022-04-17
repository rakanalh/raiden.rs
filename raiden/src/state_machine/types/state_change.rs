use super::{
    BalanceProofState,
    HopState,
    LockedTransferState,
    RouteState,
    SendSecretReveal,
    TransactionChannelDeposit,
    TransferDescriptionWithSecretState,
};
use crate::{
    primitives::{
        AddressMetadata,
        BlockExpiration,
        BlockHash,
        BlockNumber,
        CanonicalIdentifier,
        ChainID,
        GasLimit,
        Locksroot,
        MediationFeeConfig,
        MessageIdentifier,
        Nonce,
        PaymentIdentifier,
        RevealTimeout,
        Secret,
        SecretHash,
        SecretRegistryAddress,
        Signature,
        TokenAmount,
        TokenNetworkRegistryAddress,
        TransactionHash,
    },
    state_machine::types::{
        ChannelState,
        TokenNetworkRegistryState,
        TokenNetworkState,
    },
};
use raiden_macros::IntoStateChange;
use serde::{
    Deserialize,
    Serialize,
};
use web3::types::Address;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum StateChange {
    Block(Block),
    ActionInitChain(ActionInitChain),
    ActionInitInitiator(ActionInitInitiator),
    ActionInitMediator(ActionInitMediator),
    ActionInitTarget(ActionInitTarget),
    ActionChannelClose(ActionChannelClose),
    ActionChannelCoopSettle(ActionChannelCoopSettle),
    ActionChannelSetRevealTimeout(ActionChannelSetRevealTimeout),
    ActionChannelWithdraw(ActionChannelWithdraw),
    ActionTransferReroute(ActionTransferReroute),
    ActionCancelPayment(ActionCancelPayment),
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
    ReceiveDelivered(ReceiveDelivered),
    ReceiveProcessed(ReceiveProcessed),
    ReceiveTransferCancelRoute(ReceiveTransferCancelRoute),
    ReceiveSecretReveal(ReceiveSecretReveal),
    ReceiveSecretRequest(ReceiveSecretRequest),
    ReceiveTransferRefund(ReceiveTransferRefund),
    ReceiveLockExpired(ReceiveLockExpired),
    ReceiveUnlock(ReceiveUnlock),
    ReceiveWithdrawRequest(ReceiveWithdrawRequest),
    ReceiveWithdrawConfirmation(ReceiveWithdrawConfirmation),
    ReceiveWithdrawExpired(ReceiveWithdrawExpired),
    UpdateServicesAddresses(UpdateServicesAddresses),
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct Block {
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub gas_limit: GasLimit,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitChain {
    pub chain_id: ChainID,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub our_address: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelSetRevealTimeout {
    pub canonical_identifier: CanonicalIdentifier,
    pub reveal_timeout: RevealTimeout,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelWithdraw {
    pub canonical_identifier: CanonicalIdentifier,
    pub total_withdraw: TokenAmount,
    pub recipient_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelCoopSettle {
    pub canonical_identifier: CanonicalIdentifier,
    pub recipient_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelClose {
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveTokenNetworkRegistry {
    pub transaction_hash: Option<TransactionHash>,
    pub token_network_registry: TokenNetworkRegistryState,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveTokenNetworkCreated {
    pub transaction_hash: Option<TransactionHash>,
    pub token_network_registry_address: TokenNetworkRegistryAddress,
    pub token_network: TokenNetworkState,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelOpened {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub channel_state: ChannelState,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelClosed {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub transaction_from: Address,
    pub canonical_identifier: CanonicalIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelSettled {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub canonical_identifier: CanonicalIdentifier,
    pub our_onchain_locksroot: Locksroot,
    pub partner_onchain_locksroot: Locksroot,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelDeposit {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub canonical_identifier: CanonicalIdentifier,
    pub deposit_transaction: TransactionChannelDeposit,
    pub fee_config: MediationFeeConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelWithdraw {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub canonical_identifier: CanonicalIdentifier,
    pub participant: Address,
    pub total_withdraw: TokenAmount,
    pub fee_config: MediationFeeConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelBatchUnlock {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub canonical_identifier: CanonicalIdentifier,
    pub receiver: Address,
    pub sender: Address,
    pub locksroot: Locksroot,
    pub unlocked_amount: TokenAmount,
    pub returned_tokens: TokenAmount,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveSecretReveal {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub secret_registry_address: SecretRegistryAddress,
    pub secrethash: SecretHash,
    pub secret: Secret,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveRouteNew {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub canonical_identifier: CanonicalIdentifier,
    pub participant1: Address,
    pub participant2: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveUpdateTransfer {
    pub transaction_hash: Option<TransactionHash>,
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub canonical_identifier: CanonicalIdentifier,
    pub nonce: Nonce,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitInitiator {
    pub transfer: TransferDescriptionWithSecretState,
    pub routes: Vec<RouteState>,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitMediator {
    pub sender: Address,
    pub balance_proof: BalanceProofState,
    pub from_hop: HopState,
    pub candidate_route_states: Vec<RouteState>,
    pub from_transfer: LockedTransferState,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitTarget {
    pub sender: Address,
    pub balance_proof: BalanceProofState,
    pub from_hop: HopState,
    pub transfer: LockedTransferState,
    pub received_valid_secret: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionTransferReroute {
    pub transfer: LockedTransferState,
    pub secret: Secret,
    pub secrethash: SecretHash,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionCancelPayment {
    pub payment_identifier: PaymentIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveTransferCancelRoute {
    pub transfer: LockedTransferState,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveSecretRequest {
    pub sender: Address,
    pub payment_identifier: PaymentIdentifier,
    pub amount: TokenAmount,
    pub expiration: BlockExpiration,
    pub secrethash: SecretHash,
    pub revealsecret: Option<SendSecretReveal>,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveSecretReveal {
    pub sender: Address,
    pub secret: Secret,
    pub secrethash: SecretHash,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveLockExpired {
    pub sender: Address,
    pub secrethash: SecretHash,
    pub message_identifier: MessageIdentifier,
    pub balance_proof: BalanceProofState,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveTransferRefund {
    pub transfer: LockedTransferState,
    pub balance_proof: BalanceProofState,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveUnlock {
    pub message_identifier: MessageIdentifier,
    pub secret: Secret,
    pub secrethash: SecretHash,
    pub balance_proof: BalanceProofState,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveWithdrawRequest {
    pub sender: Address,
    pub message_identifier: MessageIdentifier,
    pub canonical_identifier: CanonicalIdentifier,
    pub total_withdraw: TokenAmount,
    pub nonce: Nonce,
    pub expiration: BlockExpiration,
    pub signature: Signature,
    pub participant: Address,
    pub coop_settle: bool,
    pub sender_metadata: Option<AddressMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveWithdrawConfirmation {
    pub sender: Address,
    pub message_identifier: MessageIdentifier,
    pub canonical_identifier: CanonicalIdentifier,
    pub total_withdraw: TokenAmount,
    pub nonce: Nonce,
    pub expiration: BlockExpiration,
    pub signature: Signature,
    pub participant: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveWithdrawExpired {
    pub sender: Address,
    pub message_identifier: MessageIdentifier,
    pub canonical_identifier: CanonicalIdentifier,
    pub total_withdraw: TokenAmount,
    pub nonce: Nonce,
    pub expiration: BlockExpiration,
    pub participant: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveDelivered {
    pub sender: Address,
    pub message_identifier: MessageIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveProcessed {
    pub sender: Address,
    pub message_identifier: MessageIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct UpdateServicesAddresses {
    pub service: Address,
    pub valid_til: u32,
}
