mod app;
mod helpers;

use std::path::PathBuf;

pub use self::app::*;
pub use self::helpers::*;

use clap::{
    Clap,
    ValueHint,
};
use raiden::primitives::ChainID;
use std::error::Error;

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

#[derive(Clap, Debug, PartialEq)]
pub enum ArgChainID {
    Mainnet = 1,
    Ropsten = 3,
    Rinkeby = 4,
    Goerli = 5,
    Kovan = 42,
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

#[derive(Clap, Debug)]
pub struct CliMediationConfig {
    #[clap(long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    pub flat_fee: Vec<(String, u64)>,

    #[clap(long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    pub proportional_fee: Vec<(String, u64)>,

    #[clap(long, parse(try_from_str = parse_key_val), number_of_values = 1)]
    pub proportional_imbalance_fee: Vec<(String, u64)>,

    #[clap(long)]
    pub cap_mediation_fees: bool,
}

/// A basic example
#[derive(Clap, Debug)]
#[clap(name = "Raiden unofficial rust client")]
pub struct Opt {
    /// Specify the blockchain to run Raiden on.
    #[clap(arg_enum, short('c'), long, default_value = "3", required = true, takes_value = true)]
    pub chain_id: ArgChainID,

    /// Specify the RPC endpoint to interact with.
    #[clap(long, required = true, takes_value = true)]
    pub eth_rpc_endpoint: String,

    /// Specify the RPC endpoint to interact with.
    #[clap(long, required = true, takes_value = true)]
    pub eth_rpc_socket_endpoint: String,

    #[clap(short('c'), long, parse(from_os_str), value_hint = ValueHint::FilePath, required = true, takes_value = true)]
    pub keystore_path: PathBuf,

    #[clap(short('d'), long, parse(from_os_str), value_hint = ValueHint::FilePath, required = true, takes_value = true, default_value = "~/.raiden")]
    pub datadir: PathBuf,

    // The number of occurrences of the `v/verbose` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[clap(short, long, parse(from_occurrences))]
    verbose: u8,

    #[clap(flatten)]
    mediation_fees: CliMediationConfig,
}
