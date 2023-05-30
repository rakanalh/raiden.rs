#![warn(clippy::missing_docs_in_private_items)]

use std::str::FromStr;

use derive_more::Display;
use web3::types::U256;

#[repr(u8)]
#[derive(Copy, Clone, Display, Debug, Eq, Hash, PartialEq)]
pub enum ChainID {
	Mainnet = 1,
	Ropsten = 3,
	Rinkeby = 4,
	Goerli = 5,
	Private(U256),
}

impl From<u64> for ChainID {
	fn from(value: u64) -> Self {
		match value {
			1 => ChainID::Mainnet,
			3 => ChainID::Ropsten,
			4 => ChainID::Rinkeby,
			5 => ChainID::Goerli,
			id => ChainID::Private(id.into()),
		}
	}
}

impl From<ChainID> for u64 {
	fn from(val: ChainID) -> Self {
		match val {
			ChainID::Mainnet => 1u64,
			ChainID::Ropsten => 3u64,
			ChainID::Rinkeby => 4u64,
			ChainID::Goerli => 5u64,
			ChainID::Private(id) => id.as_u64(),
		}
	}
}

impl From<U256> for ChainID {
	fn from(value: U256) -> Self {
		let mainnet: U256 = 1u64.into();
		let ropsten: U256 = 3u64.into();
		let rinkeby: U256 = 4u64.into();
		let goerli: U256 = 5u64.into();

		if value == mainnet {
			ChainID::Mainnet
		} else if value == ropsten {
			ChainID::Ropsten
		} else if value == rinkeby {
			ChainID::Rinkeby
		} else if value == goerli {
			ChainID::Goerli
		} else {
			ChainID::Private(value)
		}
	}
}

impl From<ChainID> for U256 {
	fn from(val: ChainID) -> Self {
		let chain_id: u64 = val.into();
		chain_id.into()
	}
}

impl From<ChainID> for Vec<u8> {
	fn from(val: ChainID) -> Self {
		let chain_id: u64 = val.into();
		chain_id.to_be_bytes().to_vec()
	}
}

impl FromStr for ChainID {
	type Err = ();

	fn from_str(s: &str) -> Result<ChainID, ()> {
		Ok(s.parse::<u64>().map_err(|_| ())?.into())
	}
}
