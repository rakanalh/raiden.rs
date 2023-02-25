pub trait ToBytes {
	fn as_vec(&self) -> Vec<u8>;
	fn to_bytes(&self) -> &[u8];
}

pub trait ToString {
	fn to_string(&self) -> String;
}
