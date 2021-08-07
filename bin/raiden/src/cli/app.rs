use crate::{
    event_handler::EventHandler,
    services::{
        BlockMonitorService,
        SyncService,
    },
};
use parking_lot::RwLock;
use raiden::{
    api::Api,
    blockchain::{
        contracts::{
            self,
            ContractsManager,
        },
        proxies::ProxyManager,
    },
    primitives::{
        RaidenConfig,
        U64,
    },
    services::{
        TransitionService,
        Transitioner,
    },
    state_manager::StateManager,
    storage::Storage,
};
use rusqlite::Connection;
use slog::Logger;
use std::sync::Arc;
use web3::{
    transports::{
        Http,
        WebSocket,
    },
    Web3,
};

type Result<T> = std::result::Result<T, String>;

pub struct RaidenApp {
    config: RaidenConfig,
    web3: Web3<Http>,
    contracts_manager: Arc<ContractsManager>,
    proxy_manager: Arc<ProxyManager>,
    state_manager: Arc<RwLock<StateManager>>,
    transition_service: Arc<dyn Transitioner + Send + Sync>,
    sync_start_block_number: U64,
    logger: Logger,
}

impl RaidenApp {
    pub fn new(config: RaidenConfig, web3: Web3<Http>, logger: Logger) -> Result<Self> {
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
        let (state_manager, sync_start_block_number) = match StateManager::restore_or_init_state(
            storage,
            config.chain_id.clone(),
            config.account.address(),
            token_network_registry_deployed_contract.address,
            token_network_registry_deployed_contract.block,
        ) {
            Ok((state_manager, block_number)) => (Arc::new(RwLock::new(state_manager)), block_number),
            Err(e) => {
                return Err(format!("Failed to initialize state {}", e));
            }
        };

        let proxy_manager = ProxyManager::new(web3.clone(), contracts_manager.clone())
            .map(|pm| Arc::new(pm))
            .map_err(|e| format!("Failed to initialize proxy manager: {}", e))?;

        let sm = state_manager.clone();
        let transition_service = Arc::new(TransitionService::new(state_manager.clone(), move |event| {
            let event_handler = EventHandler::new(sm.clone());
            async move { event_handler.handle_event(event).await }
        }));

        Ok(Self {
            config,
            web3,
            contracts_manager,
            proxy_manager,
            state_manager,
            transition_service,
            sync_start_block_number,
            logger,
        })
    }

    pub async fn run(&self) {
        let latest_block_number = self.web3.eth().block_number().await.unwrap();

        let ws = match WebSocket::new(&self.config.eth_socket_rpc_endpoint).await {
            Ok(ws) => ws,
            Err(_) => return,
        };

        let mut sync_service = SyncService::new(
            self.web3.clone(),
            self.config.clone(),
            self.state_manager.clone(),
            self.contracts_manager.clone(),
            self.proxy_manager.clone(),
            self.transition_service.clone(),
            self.logger.clone(),
        );

        sync_service
            .sync(self.sync_start_block_number, latest_block_number.into())
            .await;

        let block_monitor = match BlockMonitorService::new(
            ws,
            self.state_manager.clone(),
            self.transition_service.clone(),
            sync_service,
            self.logger.clone(),
        ) {
            Ok(bm) => bm,
            Err(_) => return,
        };

        let api = Api::new(
            self.state_manager.clone(),
            self.proxy_manager.clone(),
            self.transition_service.clone(),
            self.logger.clone(),
        );

        futures::join!(
            block_monitor.start(),
            crate::http::HttpServer::new(
                Arc::new(api),
                self.config.account.clone(),
                self.state_manager.clone(),
                self.contracts_manager.clone(),
                self.proxy_manager.clone(),
                self.logger.clone()
            )
            .start()
        );
    }
}
