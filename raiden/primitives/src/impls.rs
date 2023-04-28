#![warn(clippy::missing_docs_in_private_items)]

use web3::{
	signing::{
		keccak256,
		Signature,
	},
	types::{
		Address,
		Bytes,
		U256,
	},
};

use crate::traits::{
	Checksum,
	Stringify,
	ToBytes,
	ToPexAddress,
};

impl ToBytes for U256 {
	fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = [0u8; 32];
		self.to_big_endian(&mut bytes);
		bytes.to_vec()
	}
}

impl ToBytes for Signature {
	fn to_bytes(&self) -> Vec<u8> {
		let rb = self.r.to_fixed_bytes();
		let sb = self.s.to_fixed_bytes();
		let sv = self.v.to_be_bytes();

		let mut b = vec![];
		b.extend(&rb);
		b.extend(&sb);
		b.push(sv[sv.len() - 1]);
		b
	}
}

impl Stringify for Signature {
	fn as_string(&self) -> String {
		let bytes = self.to_bytes();
		format!("0x{}", hex::encode(&bytes))
	}
}

impl Stringify for Bytes {
	fn as_string(&self) -> String {
		let bytes = &self.0;
		format!("0x{}", hex::encode(&bytes))
	}
}

/// Adapted from: https://github.com/gakonst/ethers-rs/blob/da743fc8b29ffeb650c767f622bb19eba2f057b7/ethers-core/src/utils/mod.rs#L407
impl Checksum for Address {
	fn checksum(&self) -> String {
		let prefixed_address = format!("{self:x}");
		let hash = hex::encode(keccak256(prefixed_address.as_bytes()));
		let hash = hash.as_bytes();

		let addr_hex = hex::encode(self.as_bytes());
		let addr_hex = addr_hex.as_bytes();

		addr_hex.iter().zip(hash).fold("0x".to_owned(), |mut encoded, (addr, hash)| {
			encoded.push(if *hash >= 56 {
				addr.to_ascii_uppercase() as char
			} else {
				addr.to_ascii_lowercase() as char
			});
			encoded
		})
	}
}

impl Checksum for Option<Address> {
	fn checksum(&self) -> String {
		if let Some(address) = self {
			address.checksum()
		} else {
			String::new()
		}
	}
}

impl ToPexAddress for Address {
	fn pex(&self) -> String {
		hex::encode(&self.checksum()[..8])
	}
}
