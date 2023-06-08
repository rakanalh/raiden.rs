use std::collections::HashMap;

use raiden_primitives::types::{
	Address,
	BlockHash,
	BlockNumber,
	CanonicalIdentifier,
	ChainID,
	ChannelIdentifier,
	RevealTimeout,
	SettleTimeout,
	TokenAmount,
	TransactionHash,
};

use super::Keyring;
use crate::{
	constants::{
		DEFAULT_REVEAL_TIMEOUT,
		DEFAULT_SETTLE_TIMEOUT,
	},
	machine::chain,
	types::{
		ChainState,
		ChannelEndState,
		ChannelState,
		ContractReceiveChannelDeposit,
		ContractReceiveChannelOpened,
		ContractReceiveTokenNetworkCreated,
		ContractReceiveTokenNetworkRegistry,
		FeeScheduleState,
		MediationFeeConfig,
		PaymentMappingState,
		Random,
		TokenNetworkRegistryState,
		TokenNetworkState,
		TransactionChannelDeposit,
		TransactionExecutionStatus,
		TransactionResult,
	},
};

pub struct ChainStateInfo {
	pub chain_state: ChainState,
	pub token_network_registry_address: Address,
	pub token_network_address: Address,
	pub token_address: Address,
	pub canonical_identifiers: Vec<CanonicalIdentifier>,
}

pub struct ChainStateBuilder {
	chain_state: ChainState,
	token_network_registry_address: Address,
	token_network_address: Address,
	token_address: Address,
	canonical_identifiers: Vec<CanonicalIdentifier>,
}

impl ChainStateBuilder {
	pub fn new() -> Self {
		Self {
			chain_state: ChainState {
				chain_id: ChainID::Goerli,
				block_number: BlockNumber::from(1u64),
				block_hash: BlockHash::zero(),
				our_address: Keyring::Alice.address(),
				identifiers_to_tokennetworkregistries: HashMap::new(),
				payment_mapping: PaymentMappingState { secrethashes_to_task: HashMap::new() },
				pending_transactions: vec![],
				pseudo_random_number_generator: Random::new(),
			},
			token_network_registry_address: Address::random(),
			token_network_address: Address::random(),
			token_address: Address::random(),
			canonical_identifiers: vec![],
		}
	}

	pub fn with_token_network_registry(mut self) -> Self {
		let state_change = ContractReceiveTokenNetworkRegistry {
			transaction_hash: Some(TransactionHash::random()),
			token_network_registry: TokenNetworkRegistryState {
				address: self.token_network_registry_address,
				tokennetworkaddresses_to_tokennetworks: HashMap::new(),
				tokenaddresses_to_tokennetworkaddresses: HashMap::new(),
			},
			block_number: BlockNumber::from(1u64),
			block_hash: BlockHash::random(),
		};

		let result = chain::state_transition(self.chain_state, state_change.into())
			.expect("State transition should succeed");

		assert!(result
			.new_state
			.identifiers_to_tokennetworkregistries
			.get(&self.token_network_registry_address)
			.is_some());

		self.chain_state = result.new_state;
		self
	}

	pub fn with_token_network(mut self) -> Self {
		let state_change = ContractReceiveTokenNetworkCreated {
			transaction_hash: Some(TransactionHash::random()),
			token_network_registry_address: self.token_network_registry_address,
			token_network: TokenNetworkState {
				address: self.token_network_address,
				token_address: self.token_address,
				channelidentifiers_to_channels: HashMap::new(),
				partneraddresses_to_channelidentifiers: HashMap::new(),
			},
			block_number: BlockNumber::from(1u64),
			block_hash: BlockHash::random(),
		};
		let result = chain::state_transition(self.chain_state, state_change.into())
			.expect("State transition should succeed");

		self.chain_state = result.new_state;
		self
	}

	pub fn with_channels(
		mut self,
		channels: Vec<((Address, TokenAmount), (Address, TokenAmount))>,
	) -> Self {
		let mut channel_index = 0;
		let mut chain_state = self.chain_state.clone();
		for ((participant_address, participant_balance), (partner_address, partner_balance)) in
			channels
		{
			let channel_identifier = ChannelIdentifier::from(channel_index + 1);
			let canonical_identifier = CanonicalIdentifier {
				chain_identifier: self.chain_state.chain_id.clone(),
				token_network_address: self.token_network_address,
				channel_identifier,
			};
			channel_index = channel_index + 1;
			let state_change = ContractReceiveChannelOpened {
				transaction_hash: Some(TransactionHash::random()),
				block_number: BlockNumber::from(1u64),
				block_hash: BlockHash::random(),
				channel_state: ChannelState {
					canonical_identifier: canonical_identifier.clone(),
					token_address: self.token_address,
					token_network_registry_address: self.token_network_registry_address,
					reveal_timeout: RevealTimeout::from(DEFAULT_REVEAL_TIMEOUT),
					settle_timeout: SettleTimeout::from(DEFAULT_SETTLE_TIMEOUT),
					fee_schedule: FeeScheduleState::default(),
					our_state: ChannelEndState {
						address: participant_address,
						..Default::default()
					},
					partner_state: ChannelEndState {
						address: partner_address,
						..Default::default()
					},
					open_transaction: TransactionExecutionStatus {
						started_block_number: Some(BlockNumber::from(1u64)),
						finished_block_number: Some(BlockNumber::from(2u64)),
						result: Some(TransactionResult::Success),
					},
					close_transaction: None,
					settle_transaction: None,
					update_transaction: None,
				},
			};
			self.canonical_identifiers.push(canonical_identifier.clone());

			let result = chain::state_transition(chain_state, state_change.into())
				.expect("channel creation should work");

			chain_state = result.new_state;

			if !participant_balance.is_zero() {
				let participant_deposit = ContractReceiveChannelDeposit {
					transaction_hash: Some(TransactionHash::random()),
					block_number: BlockNumber::from(1u64),
					block_hash: BlockHash::random(),
					canonical_identifier: canonical_identifier.clone(),
					deposit_transaction: TransactionChannelDeposit {
						participant_address,
						contract_balance: participant_balance,
						deposit_block_number: BlockNumber::from(1u64),
					},
					fee_config: MediationFeeConfig {
						token_to_flat_fee: HashMap::new(),
						token_to_proportional_fee: HashMap::new(),
						token_to_proportional_imbalance_fee: HashMap::new(),
						cap_meditation_fees: false,
					},
				};
				let result = chain::state_transition(chain_state, participant_deposit.into())
					.expect("channel creation should work");
				chain_state = result.new_state;
			}

			if !partner_balance.is_zero() {
				let participant_deposit = ContractReceiveChannelDeposit {
					transaction_hash: Some(TransactionHash::random()),
					block_number: BlockNumber::from(1u64),
					block_hash: BlockHash::random(),
					canonical_identifier: canonical_identifier.clone(),
					deposit_transaction: TransactionChannelDeposit {
						participant_address: partner_address,
						contract_balance: partner_balance,
						deposit_block_number: BlockNumber::from(1u64),
					},
					fee_config: MediationFeeConfig {
						token_to_flat_fee: HashMap::new(),
						token_to_proportional_fee: HashMap::new(),
						token_to_proportional_imbalance_fee: HashMap::new(),
						cap_meditation_fees: false,
					},
				};
				let result = chain::state_transition(chain_state, participant_deposit.into())
					.expect("channel creation should work");
				chain_state = result.new_state;
			}
		}
		self.chain_state = chain_state;
		self
	}

	pub fn build(self) -> ChainStateInfo {
		ChainStateInfo {
			chain_state: self.chain_state,
			token_network_registry_address: self.token_network_registry_address,
			token_network_address: self.token_network_address,
			token_address: self.token_address,
			canonical_identifiers: self.canonical_identifiers,
		}
	}
}
