use web3::types::U256;

use crate::traits::ToBytes;

impl ToBytes for U256 {
	fn to_bytes(&self) -> &[u8] {
		let mut bytes = vec![];
		self.to_big_endian(&mut bytes);

		let r: &mut [u8] = Default::default();
		r.clone_from_slice(&bytes[..]);
		r
	}
}
