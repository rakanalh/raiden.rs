#![warn(clippy::missing_docs_in_private_items)]

use raiden_macros::IntoStateChange;
use raiden_primitives::types::{
	Address,
	AddressMetadata,
	BlockExpiration,
	BlockHash,
	BlockNumber,
	CanonicalIdentifier,
	ChainID,
	GasLimit,
	LockedAmount,
	Locksroot,
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
	U256,
};
use serde::{
	Deserialize,
	Serialize,
};

use crate::types::{
	event::SendSecretReveal,
	state::{
		BalanceProofState,
		HopState,
		LockedTransferState,
		RouteState,
		TransactionChannelDeposit,
		TransferDescriptionWithSecretState,
	},
	ChannelState,
	MediationFeeConfig,
	TokenNetworkRegistryState,
	TokenNetworkState,
};

/// An enum containing all possible state change variants.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
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

impl StateChange {
	/// Returns a string of the inner state change's type name.
	pub fn type_name(&self) -> &'static str {
		match self {
			StateChange::Block(_) => "Block",
			StateChange::ActionInitChain(_) => "ActionInitChain",
			StateChange::ActionInitInitiator(_) => "ActionInitInitiator",
			StateChange::ActionInitMediator(_) => "ActionInitMediator",
			StateChange::ActionInitTarget(_) => "ActionInitTarget",
			StateChange::ActionChannelClose(_) => "ActionChannelClose",
			StateChange::ActionChannelCoopSettle(_) => "ActionChannelCoopSettle",
			StateChange::ActionChannelSetRevealTimeout(_) => "ActionChannelSetRevealTimeout",
			StateChange::ActionChannelWithdraw(_) => "ActionChannelWithdraw",
			StateChange::ActionTransferReroute(_) => "ActionTransferReroute",
			StateChange::ActionCancelPayment(_) => "ActionCancelPayment",
			StateChange::ContractReceiveTokenNetworkRegistry(_) =>
				"ContractReceiveTokenNetworkRegistry",
			StateChange::ContractReceiveTokenNetworkCreated(_) =>
				"ContractReceiveTokenNetworkCreated",
			StateChange::ContractReceiveChannelOpened(_) => "ContractReceiveChannelOpened",
			StateChange::ContractReceiveChannelClosed(_) => "ContractReceiveChannelClosed",
			StateChange::ContractReceiveChannelSettled(_) => "ContractReceiveChannelSettled",
			StateChange::ContractReceiveChannelDeposit(_) => "ContractReceiveChannelDeposit",
			StateChange::ContractReceiveChannelWithdraw(_) => "ContractReceiveChannelWithdraw",
			StateChange::ContractReceiveChannelBatchUnlock(_) =>
				"ContractReceiveChannelBatchUnlock",
			StateChange::ContractReceiveSecretReveal(_) => "ContractReceiveSecretReveal",
			StateChange::ContractReceiveRouteNew(_) => "ContractReceiveRouteNew",
			StateChange::ContractReceiveUpdateTransfer(_) => "ContractReceiveUpdateTransfer",
			StateChange::ReceiveDelivered(_) => "ReceiveDelivered",
			StateChange::ReceiveProcessed(_) => "ReceiveProcessed",
			StateChange::ReceiveTransferCancelRoute(_) => "ReceiveTransferCancelRoute",
			StateChange::ReceiveSecretReveal(_) => "ReceiveSecretReveal",
			StateChange::ReceiveSecretRequest(_) => "ReceiveSecretRequest",
			StateChange::ReceiveTransferRefund(_) => "ReceiveTransferRefund",
			StateChange::ReceiveLockExpired(_) => "ReceiveLockExpired",
			StateChange::ReceiveUnlock(_) => "ReceiveUnlock",
			StateChange::ReceiveWithdrawRequest(_) => "ReceiveWithdrawRequest",
			StateChange::ReceiveWithdrawConfirmation(_) => "ReceiveWithdrawConfirmation",
			StateChange::ReceiveWithdrawExpired(_) => "ReceiveWithdrawExpired",
			StateChange::UpdateServicesAddresses(_) => "UpdateServicesAddresses",
		}
	}
}

/// Transition used when a new block is mined.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct Block {
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub gas_limit: GasLimit,
}

/// Transition to initialize chain state
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitChain {
	pub chain_id: ChainID,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub our_address: Address,
}

/// Change the reveal timeout value of a given channel.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelSetRevealTimeout {
	pub canonical_identifier: CanonicalIdentifier,
	pub reveal_timeout: RevealTimeout,
}

/// Withdraw funds from channel.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelWithdraw {
	pub canonical_identifier: CanonicalIdentifier,
	pub total_withdraw: TokenAmount,
	pub recipient_metadata: Option<AddressMetadata>,
}

/// Cooperatively withdraw funds from channel back to both parties and close the channel in a single
/// operation.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelCoopSettle {
	pub canonical_identifier: CanonicalIdentifier,
	pub recipient_metadata: Option<AddressMetadata>,
}

/// User is closing an existing channel.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionChannelClose {
	pub canonical_identifier: CanonicalIdentifier,
}

/// Registers a new token network registry.
/// A token network registry corresponds to a registry smart contract.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveTokenNetworkRegistry {
	pub transaction_hash: Option<TransactionHash>,
	pub token_network_registry: TokenNetworkRegistryState,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
}

/// A new token was registered with the token network registry.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveTokenNetworkCreated {
	pub transaction_hash: Option<TransactionHash>,
	pub token_network_registry_address: TokenNetworkRegistryAddress,
	pub token_network: TokenNetworkState,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
}

/// A new channel was created and this node IS a participant.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelOpened {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub channel_state: ChannelState,
}

/// A channel to which this node IS a participant was closed.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelClosed {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub transaction_from: Address,
	pub canonical_identifier: CanonicalIdentifier,
}

/// A channel to which this node IS a participant was settled.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelSettled {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub canonical_identifier: CanonicalIdentifier,
	pub our_onchain_locksroot: Locksroot,
	pub partner_onchain_locksroot: Locksroot,
	pub our_transferred_amount: TokenAmount,
	pub partner_transferred_amount: TokenAmount,
}

/// A channel to which this node IS a participant had a deposit.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelDeposit {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub canonical_identifier: CanonicalIdentifier,
	pub deposit_transaction: TransactionChannelDeposit,
	pub fee_config: MediationFeeConfig,
}

/// A channel to which this node IS a participant had a withdraw.
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

/// All the locks were claimed via the blockchain.

/// Used when all the hash time locks were unlocked and a log ChannelUnlocked is emitted
/// by the token network contract.
/// Note:
///     For this state change the contract caller is not important but only the
///     receiving address. `receiver` is the address to which the `unlocked_amount`
///     was transferred. `returned_tokens` was transferred to the channel partner.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveChannelBatchUnlock {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub canonical_identifier: CanonicalIdentifier,
	pub receiver: Address,
	pub sender: Address,
	pub locksroot: Locksroot,
	pub unlocked_amount: LockedAmount,
	pub returned_tokens: TokenAmount,
}

/// A new secret was registered with the SecretRegistry contract.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveSecretReveal {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub secret_registry_address: SecretRegistryAddress,
	pub secrethash: SecretHash,
	pub secret: Secret,
}

/// New channel was created and this node is NOT a participant.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveRouteNew {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub canonical_identifier: CanonicalIdentifier,
	pub participant1: Address,
	pub participant2: Address,
}

/// Participant updated the latest balance proof on-chain.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ContractReceiveUpdateTransfer {
	pub transaction_hash: Option<TransactionHash>,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub canonical_identifier: CanonicalIdentifier,
	pub nonce: Nonce,
}

/// Initial state of a new mediated transfer.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitInitiator {
	pub transfer: TransferDescriptionWithSecretState,
	pub routes: Vec<RouteState>,
}

/// Initial state for a new mediator.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitMediator {
	pub sender: Address,
	pub balance_proof: BalanceProofState,
	pub from_hop: HopState,
	pub candidate_route_states: Vec<RouteState>,
	pub from_transfer: LockedTransferState,
}

/// Initial state for a new target.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionInitTarget {
	pub sender: Address,
	pub balance_proof: BalanceProofState,
	pub from_hop: HopState,
	pub transfer: LockedTransferState,
	pub received_valid_secret: bool,
}

/// A transfer will be rerouted.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionTransferReroute {
	pub transfer: LockedTransferState,
	pub secret: Secret,
	pub secrethash: SecretHash,
}

/// The user requests the transfer to be cancelled.
/// This state change can fail, it depends on the node's role and the current
/// state of the transfer.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ActionCancelPayment {
	pub payment_identifier: PaymentIdentifier,
}

/// A mediator sends us a refund due to a failed route.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveTransferCancelRoute {
	pub transfer: LockedTransferState,
}

/// A SecretRequest message received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveSecretRequest {
	pub sender: Address,
	pub payment_identifier: PaymentIdentifier,
	pub amount: TokenAmount,
	pub expiration: BlockExpiration,
	pub secrethash: SecretHash,
	pub revealsecret: Option<SendSecretReveal>,
}

/// A SecretReveal message received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveSecretReveal {
	pub sender: Address,
	pub secret: Secret,
	pub secrethash: SecretHash,
}

/// A LockExpired message received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveLockExpired {
	pub sender: Address,
	pub secrethash: SecretHash,
	pub message_identifier: MessageIdentifier,
	pub balance_proof: BalanceProofState,
}

/// A RefundTransfer message received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveTransferRefund {
	pub transfer: LockedTransferState,
	pub balance_proof: BalanceProofState,
}

/// An Unlock message received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveUnlock {
	pub sender: Address,
	pub message_identifier: MessageIdentifier,
	pub secret: Secret,
	pub secrethash: SecretHash,
	pub balance_proof: BalanceProofState,
}

/// A Withdraw message received.
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

/// A Withdraw message was received.
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

/// A WithdrawExpired message was received.
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

/// A Delivered message was received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveDelivered {
	pub sender: Address,
	pub message_identifier: MessageIdentifier,
}

/// A processed message was received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct ReceiveProcessed {
	pub sender: Address,
	pub message_identifier: MessageIdentifier,
}

/// A `RegisteredService` contract event was received.
#[derive(Serialize, Deserialize, Clone, Debug, IntoStateChange)]
pub struct UpdateServicesAddresses {
	pub service: Address,
	pub valid_till: U256,
}
