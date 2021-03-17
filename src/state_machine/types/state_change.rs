use crate::{
    blockchain::events::Event as BlockchainEvent,
    state_machine::state::{
        CanonicalIdentifier,
        ChainState,
        ChannelState,
        TokenNetworkRegistryState,
        TokenNetworkState,
        TransactionExecutionStatus,
        TransactionResult,
    },
};
use crate::{
    constants,
    state_machine::{
        types::ChainID,
        views,
    },
};
use ethabi::Token;
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum StateChange {
    Block(Block),
    ActionInitChain(ActionInitChain),
    ContractReceiveTokenNetworkRegistry(ContractReceiveTokenNetworkRegistry),
    ContractReceiveTokenNetworkCreated(ContractReceiveTokenNetworkCreated),
    ContractReceiveChannelOpened(ContractReceiveChannelOpened),
}

impl StateChange {
    pub fn from_blockchain_event(chain_state: &ChainState, event: BlockchainEvent) -> Option<Self> {
        match event.name.as_ref() {
            "TokenNetworkCreated" => Self::create_token_network_created_state_change(event),
            "ChannelOpened" => Self::create_channel_opened_state_change(chain_state, event),
            _ => None,
        }
    }

    fn create_token_network_created_state_change(event: BlockchainEvent) -> Option<Self> {
        let token_address = match event.data.get("token_address")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let token_network_address = match event.data.get("token_network_address")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let token_network = TokenNetworkState::new(token_network_address, token_address);
        let token_network_registry_address = event.address;
        Some(StateChange::ContractReceiveTokenNetworkCreated(
            ContractReceiveTokenNetworkCreated {
                transaction_hash: Some(event.transaction_hash),
                block_number: event.block_number,
                block_hash: event.block_hash,
                token_network_registry_address,
                token_network,
            },
        ))
    }

    fn create_channel_opened_state_change(chain_state: &ChainState, event: BlockchainEvent) -> Option<Self> {
        let channel_identifier = match event.data.get("channel_identifier")? {
            Token::Uint(identifier) => identifier.clone(),
            _ => U256::zero(),
        };
        let participant1 = match event.data.get("participant1")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let participant2 = match event.data.get("participant2")? {
            Token::Address(address) => address.clone(),
            _ => Address::zero(),
        };
        let settle_timeout = match event.data.get("settle_timeout")? {
            Token::Uint(timeout) => timeout.clone(),
            _ => U256::zero(),
        };

        let partner_address: Address;
        let our_address = chain_state.our_address;
        if participant1 == our_address {
            partner_address = participant2;
        } else {
            partner_address = participant1;
        }

        let token_network_address = event.address;
        let _token_network_registry =
            views::get_token_network_registry_by_token_network_address(&chain_state, token_network_address)?;
        let token_network = views::get_token_network_by_address(&chain_state, token_network_address)?;
        let token_address = token_network.token_address;
        let token_network_registry_address = Address::zero();
        let reveal_timeout = U256::from(constants::DEFAULT_REVEAL_TIMEOUT);
        let open_transaction = TransactionExecutionStatus {
            started_block_number: Some(U64::from(0)),
            finished_block_number: Some(event.block_number),
            result: Some(TransactionResult::Success),
        };
        let channel_state = ChannelState::new(
            CanonicalIdentifier {
                chain_identifier: chain_state.chain_id.clone(),
                token_network_address,
                channel_identifier,
            },
            token_address,
            token_network_registry_address,
            our_address,
            partner_address,
            reveal_timeout,
            settle_timeout,
            open_transaction,
        )
        .ok()?;

        Some(StateChange::ContractReceiveChannelOpened(
            ContractReceiveChannelOpened {
                transaction_hash: Some(event.transaction_hash),
                block_number: event.block_number,
                block_hash: event.block_hash,
                channel_state,
            },
        ))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    pub block_number: U64,
    pub block_hash: H256,
    pub gas_limit: U256,
}

impl Block {
    pub fn new(block_number: U64, block_hash: H256, gas_limit: U256) -> Block {
        Block {
            block_number,
            block_hash,
            gas_limit,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionInitChain {
    pub chain_id: ChainID,
    pub block_number: U64,
    pub block_hash: H256,
    pub our_address: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkRegistry {
    pub transaction_hash: Option<H256>,
    pub token_network_registry: TokenNetworkRegistryState,
    pub block_number: U64,
    pub block_hash: H256,
}

impl ContractReceiveTokenNetworkRegistry {
    pub fn new(
        transaction_hash: H256,
        token_network_registry: TokenNetworkRegistryState,
        block_number: U64,
        block_hash: H256,
    ) -> Self {
        ContractReceiveTokenNetworkRegistry {
            transaction_hash: Some(transaction_hash),
            token_network_registry,
            block_number,
            block_hash,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveTokenNetworkCreated {
    pub transaction_hash: Option<H256>,
    pub token_network_registry_address: Address,
    pub token_network: TokenNetworkState,
    pub block_number: U64,
    pub block_hash: H256,
}

impl ContractReceiveTokenNetworkCreated {
    pub fn new(
        transaction_hash: H256,
        token_network_registry_address: Address,
        token_network: TokenNetworkState,
        block_number: U64,
        block_hash: H256,
    ) -> Self {
        ContractReceiveTokenNetworkCreated {
            transaction_hash: Some(transaction_hash),
            token_network_registry_address,
            token_network,
            block_number,
            block_hash,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractReceiveChannelOpened {
    pub transaction_hash: Option<H256>,
    pub block_number: U64,
    pub block_hash: H256,
    pub channel_state: ChannelState,
}

impl ContractReceiveChannelOpened {
    pub fn new(transaction_hash: H256, block_number: U64, block_hash: H256, channel_state: ChannelState) -> Self {
        ContractReceiveChannelOpened {
            transaction_hash: Some(transaction_hash),
            block_number,
            block_hash,
            channel_state,
        }
    }
}
