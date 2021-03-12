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
use clap::ArgMatches;
use ethsign::SecretKey;
use parking_lot::RwLock;
use raiden::{
    blockchain::contracts::{
        self,
        ContractsManager,
    },
    state_machine::types::ChainID,
    state_manager::StateManager,
    storage::Storage,
};
use rusqlite::Connection;
use slog::{
    Drain,
    Logger,
};
use std::{
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};
use web3::{
    transports::{
        Http,
        WebSocket,
    },
    types::Address,
    Web3,
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone)]
pub struct Config {
    pub chain_id: ChainID,
    pub datadir: PathBuf,
    pub keystore_path: PathBuf,
    pub eth_http_rpc_endpoint: String,
    pub eth_socket_rpc_endpoint: String,
}

impl Config {
    pub fn new(args: ArgMatches) -> Result<Self> {
        // TODO: No unwrap
        let chain_name = args.value_of("chain-id").unwrap();
        let chain_id = chain_name.parse().unwrap();

        let eth_rpc_http_endpoint = args.value_of("eth-rpc-endpoint").unwrap();
        let eth_rpc_socket_endpoint = args.value_of("eth-rpc-socket-endpoint").unwrap();
        let http_endpoint = eth_rpc_http_endpoint.to_http();
        if let Err(e) = http_endpoint {
            return Err(format!("Invalid RPC endpoint: {}", e));
        }

        let socket_endpoint = eth_rpc_socket_endpoint.to_socket();
        if let Err(e) = socket_endpoint {
            return Err(format!("Invalid RPC endpoint: {}", e));
        }

        let keystore_path = Path::new(args.value_of("keystore-path").unwrap());
        let datadir = expanduser::expanduser(args.value_of("datadir").unwrap()).unwrap();

        Ok(Self {
            chain_id,
            datadir,
            keystore_path: keystore_path.to_path_buf(),
            eth_http_rpc_endpoint: http_endpoint.unwrap(),
            eth_socket_rpc_endpoint: socket_endpoint.unwrap(),
        })
    }
}

pub struct RaidenApp {
    config: Config,
    web3: Web3<Http>,
    node_address: Address,
    private_key: SecretKey,
    contracts_manager: Arc<ContractsManager>,
    storage: Arc<Storage>,
    state_manager: Arc<RwLock<StateManager>>,
    logger: Logger,
}

impl RaidenApp {
    pub fn new(config: Config, node_address: Address, private_key: SecretKey) -> Result<Self> {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();

        let logger = slog::Logger::root(drain, o!());

        let http = web3::transports::Http::new(&config.eth_http_rpc_endpoint).unwrap();
        let web3 = web3::Web3::new(http);

        let contracts_manager = match contracts::ContractsManager::new(config.chain_id.clone()) {
            Ok(contracts_manager) => contracts_manager,
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

        let state_manager = match StateManager::restore_or_init_state(
            storage.clone(),
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

        Ok(Self {
            config,
            web3,
            node_address,
            private_key,
            contracts_manager: Arc::new(contracts_manager),
            storage,
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
            self.state_manager.clone(),
            self.contracts_manager.clone(),
            transition_service.clone(),
            self.logger.clone(),
        );
        sync_service.sync(sync_start_block_number, latest_block_number).await;

        let block_monitor = match BlockMonitorService::new(
            ws,
            self.config.chain_id.clone(),
            self.state_manager.clone(),
            transition_service.clone(),
            sync_service,
            self.logger.clone(),
        ) {
            Ok(bm) => bm,
            Err(_) => return,
        };

        futures::join!(
            block_monitor.start(),
            crate::http::HttpServer::new(
                self.state_manager.clone(),
                self.contracts_manager.clone(),
                self.logger.clone()
            )
            .start()
        );
    }
}
