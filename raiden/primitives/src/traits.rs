pub trait ToBytes {
	fn to_bytes(&self) -> Vec<u8>;
}

pub trait ToString {
	fn to_string(&self) -> String;
}

pub trait ToChecksummed {
	fn to_checksummed(&self) -> String;
}
