use std::{
	error::Error,
	path::PathBuf,
};

use raiden_bin_common::parse_address;
use raiden_network_transport::{
	config::{
		MatrixTransportConfig,
		TransportConfig,
	},
	matrix::constants::MATRIX_AUTO_SELECT_SERVER,
	types::EnvironmentType,
};
use raiden_pathfinding::{
	config::ServicesConfig,
	types::RoutingMode,
};
use raiden_primitives::{
	constants::{
		PFS_DEFAULT_IOU_TIMEOUT,
		PFS_DEFAULT_MAX_FEE,
		PFS_DEFAULT_MAX_PATHS,
	},
	types::{
		Address,
		TokenAmount,
		U256,
	},
};
use structopt::{
	clap::arg_enum,
	StructOpt,
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

fn parse_chain_id(src: &str) -> Result<u64, Box<dyn Error + Send + Sync + 'static>> {
	match src {
		"mainnet" => Ok(1),
		"ropsten" => Ok(3),
		"rinkeby" => Ok(4),
		"goerli" => Ok(5),
		value => {
			let id: u64 = value.parse().map_err(|e| format!("Invalid chain ID: {:?}", e))?;
			Ok(id)
		},
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
	pub pathfinding_service_address: String,
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
		let max_paths = if s.pathfinding_max_paths < 1 {
			*PFS_DEFAULT_MAX_PATHS
		} else {
			s.pathfinding_max_paths
		};
		let max_fee = if s.pathfinding_max_fee == U256::zero() {
			*PFS_DEFAULT_MAX_FEE
		} else {
			s.pathfinding_max_fee
		};
		let iou_timeout = if s.pathfinding_iou_timeout == 0 {
			*PFS_DEFAULT_IOU_TIMEOUT
		} else {
			s.pathfinding_iou_timeout.into()
		};
		ServicesConfig {
			routing_mode: s.routing_mode.into(),
			pathfinding_service_random_address: s.pathfinding_service_random_address,
			pathfinding_service_address: s.pathfinding_service_address,
			pathfinding_max_paths: max_paths,
			pathfinding_max_fee: max_fee,
			pathfinding_iou_timeout: iou_timeout,
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

impl Into<TransportConfig> for CliMatrixTransportConfig {
	fn into(self) -> TransportConfig {
		TransportConfig {
			retry_timeout: self.retry_timeout,
			retry_timeout_max: self.retry_timeout_max,
			retry_count: self.retry_count,
			matrix: MatrixTransportConfig { homeserver_url: self.matrix_server },
		}
	}
}

#[derive(StructOpt, Debug)]
#[structopt(name = "Raiden unofficial rust client")]
pub struct Opt {
	/// Specify the blockchain to run Raiden on.
	#[structopt(
		short("c"),
		long,
		parse(try_from_str = parse_chain_id),
		default_value = "1",
		required = true,
		takes_value = true
	)]
	pub chain_id: u64,

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

	/// Specify the http server host
	#[structopt(long, required = true, takes_value = true, default_value = "127.0.0.1:3000")]
	pub api_address: String,

	#[structopt(short("k"), long, parse(from_os_str), required = true, takes_value = true)]
	pub keystore_path: PathBuf,

	#[structopt(short("a"), long, parse(try_from_str = parse_address), takes_value = true)]
	pub address: Option<Address>,

	#[structopt(long, parse(from_os_str), takes_value = true)]
	pub password_file: Option<PathBuf>,

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

	#[structopt(long, required = false, takes_value = true, default_value = "info")]
	pub log_config: String,

	#[structopt(long)]
	pub log_json: bool,

	#[structopt(long, parse(from_os_str), takes_value = true)]
	pub log_file: Option<PathBuf>,
}
