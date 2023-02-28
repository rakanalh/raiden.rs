use web3::{
	signing::Signature,
	types::U256,
};

use crate::traits::{
	ToBytes,
	ToString,
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

impl ToString for Signature {
	fn to_string(&self) -> String {
		let bytes = self.to_bytes();
		format!("0x{}", hex::encode(&bytes))
	}
}
