use std::error::Error;
use std::path::PathBuf;
use structopt::{
    clap::arg_enum,
    StructOpt,
};

mod app;
mod helpers;
pub use self::app::*;
pub use self::helpers::*;
use raiden::{
    primitives::ChainID,
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
        Kovan = 42,
    }
}

impl From<ArgChainID> for ChainID {
    fn from(c: ArgChainID) -> Self {
        match c {
            ArgChainID::Goerli => ChainID::Goerli,
            ArgChainID::Kovan => ChainID::Kovan,
            ArgChainID::Mainnet => ChainID::Mainnet,
            ArgChainID::Rinkeby => ChainID::Rinkeby,
            ArgChainID::Ropsten => ChainID::Ropsten,
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

#[derive(StructOpt, Debug)]
pub struct CliMatrixTransportConfig {
    #[structopt(long, default_value = MATRIX_AUTO_SELECT_SERVER)]
    pub matrix_server: String,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "Raiden unofficial rust client")]
pub struct Opt {
    /// Specify the blockchain to run Raiden on.
    #[structopt(possible_values = &ArgChainID::variants(), short("c"), long, default_value = "3", required = true, takes_value = true)]
    pub chain_id: ArgChainID,

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

    #[clap(flatten)]
    pub matrix_transport_config: CliMatrixTransportConfig,
}
