use raiden::primitives::{RoutingMode, ServicesConfig, TokenAmount};
use std::error::Error;
use std::path::PathBuf;
use structopt::{clap::arg_enum, StructOpt};

mod app;
mod helpers;
pub use self::app::*;
pub use self::helpers::*;
use raiden::{
    primitives::{ChainID, EnvironmentType},
    transport::matrix::constants::MATRIX_AUTO_SELECT_SERVER,
};

/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

arg_enum! {
    #[derive(Debug, PartialEq)]
    pub enum ArgChainID {
        Mainnet = 1,
        Ropsten = 3,
        Rinkeby = 4,
        Goerli = 5,
    }
}

impl From<ArgChainID> for ChainID {
    fn from(c: ArgChainID) -> Self {
        match c {
            ArgChainID::Goerli => ChainID::Goerli,
            ArgChainID::Mainnet => ChainID::Mainnet,
            ArgChainID::Rinkeby => ChainID::Rinkeby,
            ArgChainID::Ropsten => ChainID::Ropsten,
        }
    }
}

arg_enum! {
    #[derive(Debug, PartialEq)]
    pub enum ArgEnvironmentType {
        Production,
        Development,
    }
}

impl From<ArgEnvironmentType> for EnvironmentType {
    fn from(e: ArgEnvironmentType) -> Self {
        match e {
            ArgEnvironmentType::Development => EnvironmentType::Development,
            ArgEnvironmentType::Production => EnvironmentType::Production,
        }
    }
}

arg_enum! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum ArgRoutingMode {
        PFS,
        Private,
    }
}

impl From<ArgRoutingMode> for RoutingMode {
    fn from(m: ArgRoutingMode) -> Self {
        match m {
            ArgRoutingMode::PFS => RoutingMode::PFS,
            ArgRoutingMode::Private => RoutingMode::Private,
        }
    }
}

#[derive(StructOpt, Debug)]
pub struct CliMediationConfig {
    #[structopt(long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    pub flat_fee: Vec<(String, u64)>,

    #[structopt(long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    pub proportional_fee: Vec<(String, u64)>,

    #[structopt(long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    pub proportional_imbalance_fee: Vec<(String, u64)>,

    #[structopt(long)]
    pub cap_mediation_fees: bool,
}

#[derive(StructOpt, Clone, Debug)]
pub struct CliServicesConfig {
    #[structopt(
		possible_values = &ArgRoutingMode::variants(),
		default_value = "PFS",
		required = false,
		takes_value = true
	)]
    pub routing_mode: ArgRoutingMode,
    #[structopt(long)]
    pub pathfinding_service_random_address: bool,
    #[structopt(long, required = false, default_value = "")]
    pub pathfinding_service_specific_address: String,
    #[structopt(long, required = false, default_value = "0")]
    pub pathfinding_max_paths: usize,
    #[structopt(long, required = false, default_value = "0")]
    pub pathfinding_max_fee: TokenAmount,
    #[structopt(long, required = false, default_value = "0")]
    pub pathfinding_iou_timeout: u64,
    #[structopt(long)]
    pub monitoring_enabled: bool,
}

impl From<CliServicesConfig> for ServicesConfig {
    fn from(s: CliServicesConfig) -> ServicesConfig {
        ServicesConfig {
            routing_mode: s.routing_mode.into(),
            pathfinding_service_random_address: s.pathfinding_service_random_address,
            pathfinding_service_specific_address: s.pathfinding_service_specific_address,
            pathfinding_max_paths: s.pathfinding_max_paths,
            pathfinding_max_fee: s.pathfinding_max_fee,
            pathfinding_iou_timeout: s.pathfinding_iou_timeout.into(),
            monitoring_enabled: s.monitoring_enabled,
        }
    }
}

#[derive(StructOpt, Debug)]
pub struct CliMatrixTransportConfig {
    #[structopt(long, default_value = MATRIX_AUTO_SELECT_SERVER)]
    pub matrix_server: String,
    #[structopt(long, default_value = "1")]
    pub retry_count: u32,
    #[structopt(long, default_value = "5")]
    pub retry_timeout: u8,
    #[structopt(long, default_value = "60")]
    pub retry_timeout_max: u8,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "Raiden unofficial rust client")]
pub struct Opt {
    /// Specify the blockchain to run Raiden on.
    #[structopt(
		possible_values = &ArgChainID::variants(),
		short("c"), long,
		default_value = "mainnet",
		required = true,
		takes_value = true
	)]
    pub chain_id: ArgChainID,

    #[structopt(
		possible_values = &ArgEnvironmentType::variants(),
        short("e"),
        long,
        default_value = "Production",
        required = true,
        takes_value = true
    )]
    pub environment_type: ArgEnvironmentType,

    /// Specify the RPC endpoint to interact with.
    #[structopt(long, required = true, takes_value = true)]
    pub eth_rpc_endpoint: String,

    /// Specify the RPC endpoint to interact with.
    #[structopt(long, required = true, takes_value = true)]
    pub eth_rpc_socket_endpoint: String,

    #[structopt(short("k"), long, parse(from_os_str), required = true, takes_value = true)]
    pub keystore_path: PathBuf,

    #[structopt(
        short("d"),
        long,
        parse(from_os_str),
        required = true,
        takes_value = true,
        default_value = "~/.raiden/"
    )]
    pub datadir: PathBuf,

    // The number of occurrences of the `v/verbose` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: u8,

    #[structopt(flatten)]
    pub mediation_fees: CliMediationConfig,

    #[structopt(flatten)]
    pub matrix_transport_config: CliMatrixTransportConfig,

    #[structopt(flatten)]
    pub services_config: CliServicesConfig,
}
