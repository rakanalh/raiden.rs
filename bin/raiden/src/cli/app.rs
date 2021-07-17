use crate::{
    event_handler::EventHandler,
    services::{
        BlockMonitorService,
        SyncService,
        TransitionService,
    },
    traits::{
        ToHTTPEndpoint,
        ToSocketEndpoint,
    },
};
use futures::executor;
use parking_lot::RwLock;
use raiden::{
    api::Api,
    blockchain::{
        contracts::{
            self,
            ContractsManager,
        },
        key::PrivateKey,
        proxies::ProxyManager,
    },
    primitives::{
        MediationFeeConfig,
        RaidenConfig,
    },
    state_manager::StateManager,
    storage::Storage,
};
use rusqlite::Connection;
use slog::Logger;
use std::{
    convert::TryFrom,
    path::Path,
    sync::Arc,
};
use web3::{
    signing::Key,
    transports::{
        Http,
        WebSocket,
    },
    types::Address,
    Web3,
};

use super::Opt;

type Result<T> = std::result::Result<T, String>;

impl TryFrom<Opt> for RaidenConfig {
    type Error = String;

    fn try_from(args: Opt) -> Result<Self> {
        // TODO: No unwrap
        let chain_id = args.chain_id.into();
        let eth_rpc_http_endpoint = args.eth_rpc_endpoint;
        let eth_rpc_socket_endpoint = args.eth_rpc_socket_endpoint;
        let http_endpoint = eth_rpc_http_endpoint.to_http();
        if let Err(e) = http_endpoint {
            return Err(format!("Invalid RPC endpoint: {}", e));
        }

        let socket_endpoint = eth_rpc_socket_endpoint.to_socket();
        if let Err(e) = socket_endpoint {
            return Err(format!("Invalid RPC endpoint: {}", e));
        }

        let keystore_path = Path::new(&args.keystore_path);
        let datadir = expanduser::expanduser(args.datadir.to_string_lossy()).unwrap();

        let mediation_config = MediationFeeConfig {
            token_to_flat_fee: args
                .mediation_fees
                .flat_fee
                .into_iter()
                .map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
                .collect(),
            token_to_proportional_fee: args
                .mediation_fees
                .proportional_fee
                .into_iter()
                .map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
                .collect(),
            token_to_proportional_imbalance_fee: args
                .mediation_fees
                .proportional_imbalance_fee
                .into_iter()
                .map(|(a, v)| (Address::from_slice(a.as_bytes()), v.into()))
                .collect(),
            cap_meditation_fees: args.mediation_fees.cap_mediation_fees,
        };

        Ok(Self {
            chain_id,
            datadir,
            mediation_config,
            keystore_path: keystore_path.to_path_buf(),
            eth_http_rpc_endpoint: http_endpoint.unwrap(),
            eth_socket_rpc_endpoint: socket_endpoint.unwrap(),
        })
    }
}

pub struct RaidenApp {
    config: RaidenConfig,
    web3: Web3<Http>,
    contracts_manager: Arc<ContractsManager>,
    proxy_manager: Arc<ProxyManager>,
    state_manager: Arc<RwLock<StateManager>>,
    logger: Logger,
}

impl RaidenApp {
    pub fn new(config: RaidenConfig, node_address: Address, private_key: PrivateKey, logger: Logger) -> Result<Self> {
        let http = web3::transports::Http::new(&config.eth_http_rpc_endpoint).unwrap();
        let web3 = web3::Web3::new(http);

        let contracts_manager = match contracts::ContractsManager::new(config.chain_id.clone()) {
            Ok(contracts_manager) => Arc::new(contracts_manager),
            Err(e) => {
                return Err(format!("Error creating contracts manager: {}", e));
            }
        };
        let conn = match Connection::open(config.datadir.join("raiden.db")) {
            Ok(conn) => conn,
            Err(e) => {
                return Err(format!("Could not connect to database: {}", e));
            }
        };
        let storage = Arc::new(Storage::new(conn));
        storage
            .setup_database()
            .map_err(|e| format!("Failed to setup storage {}", e))?;

        let token_network_registry_deployed_contract =
            match contracts_manager.get_deployed(contracts::ContractIdentifier::TokenNetworkRegistry) {
                Ok(contract) => contract,
                Err(e) => {
                    return Err(format!(
                        "Could not find token network registry deployment info: {:?}",
                        e
                    ))
                }
            };

        debug!(logger, "Restore state");
        let state_manager = match StateManager::restore_or_init_state(
            storage,
            config.chain_id.clone(),
            node_address.clone(),
            token_network_registry_deployed_contract.address,
            token_network_registry_deployed_contract.block,
        ) {
            Ok(state_manager) => state_manager,
            Err(e) => {
                return Err(format!("Failed to initialize state {}", e));
            }
        };

        let nonce = match executor::block_on(web3.eth().transaction_count(private_key.address(), None)) {
            Ok(nonce) => nonce,
            Err(e) => return Err(format!("Failed to fetch nonce: {}", e)),
        };

        let proxy_manager = ProxyManager::new(web3.clone(), contracts_manager.clone(), private_key, nonce)
            .map_err(|e| format!("Failed to initialize proxy manager: {}", e))?;

        Ok(Self {
            config,
            web3,
            contracts_manager,
            proxy_manager: Arc::new(proxy_manager),
            state_manager: Arc::new(RwLock::new(state_manager)),
            logger,
        })
    }

    pub async fn run(&self) {
        let latest_block_number = self.web3.eth().block_number().await.unwrap();

        let ws = match WebSocket::new(&self.config.eth_socket_rpc_endpoint).await {
            Ok(ws) => ws,
            Err(_) => return,
        };

        let event_handler = EventHandler::new(self.state_manager.clone());
        let transition_service = Arc::new(TransitionService::new(self.state_manager.clone(), event_handler));

        let sync_start_block_number = self.state_manager.read().current_state.block_number;

        let mut sync_service = SyncService::new(
            self.web3.clone(),
            self.config.clone(),
            self.state_manager.clone(),
            self.contracts_manager.clone(),
            self.proxy_manager.clone(),
            transition_service.clone(),
            self.logger.clone(),
        );
        sync_service
            .sync(sync_start_block_number, latest_block_number.into())
            .await;

        let block_monitor = match BlockMonitorService::new(
            ws,
            self.state_manager.clone(),
            transition_service.clone(),
            sync_service,
            self.logger.clone(),
        ) {
            Ok(bm) => bm,
            Err(_) => return,
        };

        let api = Api::new(self.state_manager.clone(), self.proxy_manager.clone());

        futures::join!(
            block_monitor.start(),
            crate::http::HttpServer::new(
                Arc::new(api),
                self.state_manager.clone(),
                self.contracts_manager.clone(),
                self.proxy_manager.clone(),
                self.logger.clone()
            )
            .start()
        );
    }
}
