pub trait ToBytes {
	fn to_bytes(&self) -> Vec<u8>;
}

pub trait ToString {
	fn to_string(&self) -> String;
}
